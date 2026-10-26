use rusqlite::{params, Connection};
use rusqlite_migration::{Migrations, M};
use std::path::{Path, PathBuf};

use crate::{
    card::{Card, CardState, CardWithState, Meaning, Pronunciation, ReviewState, Source},
    deck::Deck,
    error::{Error, Result},
    importer::{DuplicateStrategy, ImportCard, ImportSummary},
    scheduler::ScheduledReview,
};

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("../../../../migrations/0001_init.sql")),
        M::up(include_str!(
            "../../../../migrations/0002_clean_empty_tags.sql"
        )),
    ])
}

pub struct Storage {
    conn: Connection,
}

/// `settings` key holding the per-deck daily new-card limit.
const DAILY_NEW_LIMIT_KEY: &str = "daily_new_limit";

/// Historical default, used when the setting is absent.
pub const DEFAULT_DAILY_NEW_LIMIT: i64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressCounts {
    pub due_count: i64,
    pub new_remaining: i64,
    pub new_reviewed_today: i64,
    pub reviewed_today: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyReviewStats {
    pub reviews: i64,
    pub again: i64,
    pub hard: i64,
    pub good: i64,
    pub easy: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyReviewBucket {
    pub date: String,
    pub stats: DailyReviewStats,
}

impl Storage {
    /// Open (or create) the database at `path` and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        // Legacy shims for databases created before the migration system existed.
        // ensure_card_metadata_columns runs first so that the tags column exists
        // before migration 0002 (clean_empty_tags) references it.
        ensure_card_metadata_columns(&conn)?;
        ensure_deck_metadata_columns(&conn)?;
        migrations()
            .to_latest(&mut conn)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        Ok(Self { conn })
    }

    /// Platform-appropriate default database path.
    ///
    /// - macOS:   `~/Library/Application Support/fishword/fishword.db`
    /// - Linux:   `~/.local/share/fishword/fishword.db`
    /// - Windows: `%APPDATA%\fishword\fishword.db`
    pub fn default_path() -> Result<PathBuf> {
        let base = dirs::data_dir().ok_or(Error::NoDataDir)?;
        Ok(base.join("fishword").join("fishword.db"))
    }

    // ── Deck ──────────────────────────────────────────────────────────────

    pub fn list_decks(&self) -> Result<Vec<Deck>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, created_at, catalog_id FROM decks ORDER BY created_at",
        )?;
        let decks = stmt
            .query_map([], |row| {
                Ok(Deck {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    catalog_id: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(decks)
    }

    pub fn get_active_deck_id(&self) -> Result<Option<i64>> {
        get_active_deck_id_on(&self.conn)
    }

    /// Maximum number of never-reviewed cards introduced per deck per day.
    ///
    /// Stored in `settings` so users can raise it without rebuilding the CLI.
    /// Falls back to [`DEFAULT_DAILY_NEW_LIMIT`] when unset, so existing
    /// databases keep the historical behaviour.
    pub fn daily_new_limit(&self) -> Result<i64> {
        match self.get_setting(DAILY_NEW_LIMIT_KEY)? {
            Some(value) => value.parse::<i64>().map_err(|error| {
                Error::InvalidInput(format!("invalid {DAILY_NEW_LIMIT_KEY} '{value}': {error}"))
            }),
            None => Ok(DEFAULT_DAILY_NEW_LIMIT),
        }
    }

    /// Persists a new daily new-card limit. Rejects non-positive values because a
    /// limit of 0 would permanently disable new cards with no way to distinguish
    /// it from a misconfiguration.
    pub fn set_daily_new_limit(&self, limit: i64) -> Result<()> {
        if limit <= 0 {
            return Err(Error::InvalidInput(format!(
                "{DAILY_NEW_LIMIT_KEY} must be a positive integer, got {limit}"
            )));
        }
        set_setting_on(&self.conn, DAILY_NEW_LIMIT_KEY, &limit.to_string())
    }

    pub fn get_active_deck(&self) -> Result<Option<Deck>> {
        self.get_active_deck_id()?
            .map(|deck_id| self.get_deck_by_id(deck_id))
            .transpose()
            .map(Option::flatten)
    }

    /// Sets (or clears) the active deck and clears the current card atomically.
    pub fn set_active_deck_id(&self, deck_id: Option<i64>) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        set_active_deck_id_on(&tx, deck_id)?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_deck(&self, name: &str, description: Option<&str>) -> Result<Deck> {
        match self.conn.execute(
            "INSERT INTO decks (name, description) VALUES (?1, ?2)",
            params![name, description],
        ) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Err(Error::AlreadyExists(format!("deck already exists: {name}")));
            }
            Err(e) => return Err(Error::Db(e)),
        }
        let id = self.conn.last_insert_rowid();
        let deck = self.conn.query_row(
            "SELECT id, name, description, created_at, catalog_id FROM decks WHERE id = ?1",
            params![id],
            |row| {
                Ok(Deck {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    catalog_id: row.get(4)?,
                })
            },
        )?;
        Ok(deck)
    }

    pub fn get_deck_by_name(&self, name: &str) -> Result<Option<Deck>> {
        let result = self.conn.query_row(
            "SELECT id, name, description, created_at, catalog_id FROM decks WHERE name = ?1",
            params![name],
            |row| {
                Ok(Deck {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    catalog_id: row.get(4)?,
                })
            },
        );
        match result {
            Ok(deck) => Ok(Some(deck)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Db(e)),
        }
    }

    pub fn get_deck_by_id(&self, id: i64) -> Result<Option<Deck>> {
        let result = self.conn.query_row(
            "SELECT id, name, description, created_at, catalog_id FROM decks WHERE id = ?1",
            params![id],
            |row| {
                Ok(Deck {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    catalog_id: row.get(4)?,
                })
            },
        );
        match result {
            Ok(deck) => Ok(Some(deck)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Db(e)),
        }
    }

    /// Find the deck previously created by `fishword catalog fetch <catalog_id>`, if any.
    /// Returns `None` for catalog ids that have never been fetched, including when a
    /// manually created deck happens to share the same display name.
    pub fn get_deck_by_catalog_id(&self, catalog_id: &str) -> Result<Option<Deck>> {
        let result = self.conn.query_row(
            "SELECT id, name, description, created_at, catalog_id FROM decks WHERE catalog_id = ?1",
            params![catalog_id],
            |row| {
                Ok(Deck {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    catalog_id: row.get(4)?,
                })
            },
        );
        match result {
            Ok(deck) => Ok(Some(deck)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Db(e)),
        }
    }

    /// Deletes a deck and all its cards (cascades via the `cards` foreign key).
    /// Clearing the active-deck pointer (if it pointed at this deck) and the delete
    /// itself happen in one transaction, so a crash never leaves the active deck
    /// pointing at an id that no longer exists alongside a half-deleted deck.
    pub fn delete_deck(&self, id: i64) -> Result<Deck> {
        let tx = self.conn.unchecked_transaction()?;
        let deck =
            get_deck_by_id_on(&tx, id)?.ok_or_else(|| Error::NotFound(format!("deck id {id}")))?;
        if get_active_deck_id_on(&tx)? == Some(id) {
            set_active_deck_id_on(&tx, None)?;
        }
        tx.execute("DELETE FROM decks WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(deck)
    }

    pub fn update_deck_name(&self, id: i64, new_name: &str) -> Result<Deck> {
        self.get_deck_by_id(id)?
            .ok_or_else(|| Error::NotFound(format!("deck id {id}")))?;
        match self.conn.execute(
            "UPDATE decks SET name = ?1 WHERE id = ?2",
            params![new_name, id],
        ) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Err(Error::AlreadyExists(format!(
                    "deck already exists: {new_name}"
                )));
            }
            Err(e) => return Err(Error::Db(e)),
        }
        self.get_deck_by_id(id)?
            .ok_or_else(|| Error::NotFound(format!("deck id {id}")))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn ensure_deck(&self, name: &str, description: Option<&str>) -> Result<Deck> {
        if let Some(deck) = self.get_deck_by_name(name)? {
            if deck.description.is_none() && description.is_some() {
                self.conn.execute(
                    "UPDATE decks SET description = ?1 WHERE id = ?2",
                    params![description, deck.id],
                )?;
                return self.get_deck_by_name(name)?.ok_or_else(|| {
                    Error::NotFound(format!("deck disappeared after update: {name}"))
                });
            }
            return Ok(deck);
        }
        self.insert_deck(name, description)
    }

    // ── Card ──────────────────────────────────────────────────────────────

    pub fn list_cards_by_deck(&self, deck_name: &str) -> Result<Vec<Card>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.deck_id, c.word, c.language, c.meanings, c.pronunciations,
                    c.tags, c.source_name, c.source_license, c.created_at
             FROM cards c
             JOIN decks d ON c.deck_id = d.id
             WHERE d.name = ?1
             ORDER BY c.created_at, c.id",
        )?;
        let rows = stmt
            .query_map(params![deck_name], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(
                |(
                    id,
                    deck_id,
                    word,
                    language,
                    meanings_json,
                    pronunciations_json,
                    tags_json,
                    source_name,
                    source_license,
                    created_at,
                )| {
                    Ok(Card {
                        id,
                        deck_id,
                        word,
                        language,
                        meanings: serde_json::from_str(&meanings_json)?,
                        pronunciations: serde_json::from_str(&pronunciations_json)?,
                        tags: serde_json::from_str(&tags_json)?,
                        source: source_name.map(|name| Source {
                            name,
                            license: source_license,
                        }),
                        created_at,
                    })
                },
            )
            .collect()
    }

    pub fn list_cards_by_deck_paginated(
        &self,
        deck_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Card>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.deck_id, c.word, c.language, c.meanings, c.pronunciations,
                    c.tags, c.source_name, c.source_license, c.created_at
             FROM cards c
             WHERE c.deck_id = ?1
             ORDER BY c.created_at, c.id
             LIMIT ?2 OFFSET ?3",
        )?;
        let cards = stmt
            .query_map(params![deck_id, limit, offset], card_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(cards)
    }

    pub fn insert_card(
        &self,
        deck_id: i64,
        word: &str,
        meanings: &[Meaning],
        pronunciations: &[Pronunciation],
    ) -> Result<Card> {
        self.insert_card_with_metadata(
            deck_id,
            &ImportCard {
                word: word.to_string(),
                language: "en".to_string(),
                meanings: meanings.to_vec(),
                pronunciations: pronunciations.to_vec(),
                tags: Vec::new(),
                source: None,
            },
        )
    }

    /// Inserts a card and its initial FSRS state atomically.
    pub fn insert_card_with_metadata(&self, deck_id: i64, card: &ImportCard) -> Result<Card> {
        let tx = self.conn.unchecked_transaction()?;
        let card = insert_card_with_metadata_on(&tx, deck_id, card)?;
        tx.commit()?;
        Ok(card)
    }

    pub fn import_cards(
        &self,
        deck_id: i64,
        cards: &[ImportCard],
        duplicate_strategy: DuplicateStrategy,
    ) -> Result<ImportSummary> {
        let deck = self
            .get_deck_by_id(deck_id)?
            .ok_or_else(|| Error::NotFound(format!("deck id {deck_id}")))?;
        let tx = self.conn.unchecked_transaction()?;
        let summary = import_cards_on(&tx, &deck, cards, duplicate_strategy)?;
        tx.commit()?;
        Ok(summary)
    }

    pub fn import_cards_into_new_deck(
        &self,
        name: &str,
        description: Option<&str>,
        cards: &[ImportCard],
        duplicate_strategy: DuplicateStrategy,
    ) -> Result<(Deck, ImportSummary)> {
        self.import_cards_into_new_deck_with_catalog_id(
            name,
            description,
            cards,
            duplicate_strategy,
            None,
        )
    }

    /// Same as [`Storage::import_cards_into_new_deck`], but tags the created deck with
    /// `catalog_id` so a later `fishword catalog fetch` of the same catalog deck can find
    /// it via [`Storage::get_deck_by_catalog_id`] instead of matching by display name.
    pub fn import_cards_into_new_deck_with_catalog_id(
        &self,
        name: &str,
        description: Option<&str>,
        cards: &[ImportCard],
        duplicate_strategy: DuplicateStrategy,
        catalog_id: Option<&str>,
    ) -> Result<(Deck, ImportSummary)> {
        let tx = self.conn.unchecked_transaction()?;
        let deck = insert_deck_on(&tx, name, description, catalog_id)?;
        let summary = import_cards_on(&tx, &deck, cards, duplicate_strategy)?;
        tx.commit()?;
        Ok((deck, summary))
    }

    pub fn get_card_by_id(&self, id: i64) -> Result<Option<Card>> {
        get_card_by_id_on(&self.conn, id)
    }

    pub fn get_current_card_id(&self) -> Result<Option<i64>> {
        self.get_setting("current_card_id")?
            .map(|value| {
                value.parse::<i64>().map_err(|error| {
                    Error::InvalidInput(format!("invalid current_card_id '{value}': {error}"))
                })
            })
            .transpose()
    }

    pub fn set_current_card_id(&self, card_id: Option<i64>) -> Result<()> {
        set_current_card_id_on(&self.conn, card_id)
    }

    pub fn get_current_card(&self) -> Result<Option<Card>> {
        self.get_current_card_id()?
            .map(|card_id| self.get_card_by_id(card_id))
            .transpose()
            .map(Option::flatten)
    }

    pub fn get_current_card_in_deck(&self, deck_id: i64) -> Result<Option<Card>> {
        Ok(self
            .get_current_card()?
            .filter(|card| card.deck_id == deck_id))
    }

    pub fn list_cards_with_state(&self) -> Result<Vec<CardWithState>> {
        self.list_cards_with_state_filtered(None)
    }

    pub fn list_cards_with_state_by_deck(&self, deck_id: i64) -> Result<Vec<CardWithState>> {
        self.list_cards_with_state_filtered(Some(deck_id))
    }

    fn list_cards_with_state_filtered(&self, deck_id: Option<i64>) -> Result<Vec<CardWithState>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.deck_id, c.word, c.language, c.meanings, c.pronunciations,
                    c.tags, c.source_name, c.source_license, c.created_at,
                    s.card_id, s.stability, s.difficulty, s.due, s.reps, s.lapses, s.state,
                    (
                        SELECT MAX(reviewed_at)
                        FROM review_log r
                        WHERE r.card_id = c.id
                    ) AS last_reviewed_at
             FROM cards c
             JOIN card_state s ON s.card_id = c.id
             WHERE (?1 IS NULL OR c.deck_id = ?1)
             ORDER BY c.id",
        )?;
        let rows = stmt
            .query_map(params![deck_id], |row| {
                let card = card_from_row(row)?;
                let state_str = row.get::<_, String>(16)?;
                let state = state_str.parse::<ReviewState>().unwrap_or(ReviewState::New);
                Ok(CardWithState {
                    card,
                    state: CardState {
                        card_id: row.get(10)?,
                        stability: row.get(11)?,
                        difficulty: row.get(12)?,
                        due: row.get(13)?,
                        reps: row.get(14)?,
                        lapses: row.get(15)?,
                        state,
                    },
                    last_reviewed_at: row.get(17)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn last_reviewed_at(&self, card_id: i64) -> Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT MAX(reviewed_at) FROM review_log WHERE card_id = ?1",
            params![card_id],
            |row| row.get::<_, Option<String>>(0),
        )?;
        Ok(result)
    }

    pub fn review_log_count(&self, card_id: i64) -> Result<i64> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM review_log WHERE card_id = ?1",
            params![card_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn progress_counts(&self) -> Result<ProgressCounts> {
        self.progress_counts_filtered(None)
    }

    pub fn progress_counts_by_deck(&self, deck_id: i64) -> Result<ProgressCounts> {
        self.progress_counts_filtered(Some(deck_id))
    }

    fn progress_counts_filtered(&self, deck_id: Option<i64>) -> Result<ProgressCounts> {
        let daily_new_limit = self.daily_new_limit()?;
        let due_count = self.conn.query_row(
            "SELECT COUNT(*)
             FROM card_state s
             JOIN cards c ON c.id = s.card_id
             WHERE (?1 IS NULL OR c.deck_id = ?1)
               AND s.reps > 0
               AND s.due <= datetime('now')",
            params![deck_id],
            |row| row.get(0),
        )?;
        let new_count = self.conn.query_row(
            "SELECT COUNT(*)
             FROM card_state s
             JOIN cards c ON c.id = s.card_id
             WHERE (?1 IS NULL OR c.deck_id = ?1)
               AND s.reps = 0",
            params![deck_id],
            |row| row.get::<_, i64>(0),
        )?;
        let reviewed_today = self.conn.query_row(
            "SELECT COUNT(*)
             FROM review_log r
             JOIN cards c ON c.id = r.card_id
             WHERE (?1 IS NULL OR c.deck_id = ?1)
               AND date(r.reviewed_at) = date('now')",
            params![deck_id],
            |row| row.get(0),
        )?;
        let new_reviewed_today = self.conn.query_row(
            "SELECT COUNT(*)
             FROM (
               SELECT r.card_id
               FROM review_log r
               JOIN cards c ON c.id = r.card_id
               WHERE (?1 IS NULL OR c.deck_id = ?1)
               GROUP BY r.card_id
               HAVING date(MIN(r.reviewed_at)) = date('now')
             )",
            params![deck_id],
            |row| row.get::<_, i64>(0),
        )?;
        // Clamp at 0: lowering the limit after new cards were already introduced
        // must not yield a negative quota, which would become a huge `usize`
        // downstream and silently disable the limit.
        let new_quota_remaining = (daily_new_limit - new_reviewed_today).max(0);
        Ok(ProgressCounts {
            due_count,
            new_remaining: new_quota_remaining.min(new_count),
            new_reviewed_today,
            reviewed_today,
        })
    }

    pub fn card_count_by_deck(&self, deck_id: i64) -> Result<i64> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM cards WHERE deck_id = ?1",
            params![deck_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn review_stats_by_deck_and_day_range(
        &self,
        deck_id: i64,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<DailyReviewBucket>> {
        let mut stmt = self.conn.prepare(
            "SELECT date(r.reviewed_at) AS day,
                    COUNT(*) AS reviews,
                    SUM(CASE WHEN r.rating = 1 THEN 1 ELSE 0 END) AS again,
                    SUM(CASE WHEN r.rating = 2 THEN 1 ELSE 0 END) AS hard,
                    SUM(CASE WHEN r.rating = 3 THEN 1 ELSE 0 END) AS good,
                    SUM(CASE WHEN r.rating = 4 THEN 1 ELSE 0 END) AS easy
             FROM review_log r
             JOIN cards c ON c.id = r.card_id
             WHERE c.deck_id = ?1
               AND date(r.reviewed_at) >= ?2
               AND date(r.reviewed_at) <= ?3
             GROUP BY day
             ORDER BY day",
        )?;
        let rows = stmt
            .query_map(params![deck_id, start_date, end_date], |row| {
                Ok(DailyReviewBucket {
                    date: row.get(0)?,
                    stats: DailyReviewStats {
                        reviews: row.get(1)?,
                        again: row.get(2)?,
                        hard: row.get(3)?,
                        good: row.get(4)?,
                        easy: row.get(5)?,
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Records a review and advances the current card pointer in one atomic transaction.
    /// Both the FSRS state update and the current-card pointer are committed together,
    /// so a crash between the two writes never leaves an inconsistent state.
    pub fn complete_review(&self, review: &ScheduledReview) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        record_review_on(&tx, review)?;
        set_current_card_id_on(&tx, Some(review.card_id))?;
        tx.commit()?;
        Ok(())
    }

    fn get_setting(&self, key: &str) -> Result<Option<String>> {
        get_setting_on(&self.conn, key)
    }

    /// Fetch the FSRS state for a card (returns `None` if card has no state row).
    pub fn get_card_state(&self, card_id: i64) -> Result<Option<CardState>> {
        let result = self.conn.query_row(
            "SELECT card_id, stability, difficulty, due, reps, lapses, state
             FROM card_state WHERE card_id = ?1",
            params![card_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i32>(4)?,
                    row.get::<_, i32>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        );

        match result {
            Ok((card_id, stability, difficulty, due, reps, lapses, state_str)) => {
                let state = state_str.parse::<ReviewState>().unwrap_or(ReviewState::New);
                Ok(Some(CardState {
                    card_id,
                    stability,
                    difficulty,
                    due,
                    reps,
                    lapses,
                    state,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Db(e)),
        }
    }
}

fn insert_deck_on(
    conn: &Connection,
    name: &str,
    description: Option<&str>,
    catalog_id: Option<&str>,
) -> Result<Deck> {
    match conn.execute(
        "INSERT INTO decks (name, description, catalog_id) VALUES (?1, ?2, ?3)",
        params![name, description, catalog_id],
    ) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            return Err(Error::AlreadyExists(format!("deck already exists: {name}")));
        }
        Err(e) => return Err(Error::Db(e)),
    }
    let id = conn.last_insert_rowid();
    get_deck_by_id_on(conn, id)?.ok_or_else(|| Error::NotFound(format!("deck id {id}")))
}

fn get_deck_by_id_on(conn: &Connection, id: i64) -> Result<Option<Deck>> {
    let result = conn.query_row(
        "SELECT id, name, description, created_at, catalog_id FROM decks WHERE id = ?1",
        params![id],
        |row| {
            Ok(Deck {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
                catalog_id: row.get(4)?,
            })
        },
    );
    match result {
        Ok(deck) => Ok(Some(deck)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(Error::Db(e)),
    }
}

fn get_setting_on(conn: &Connection, key: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    );
    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(Error::Db(e)),
    }
}

fn set_setting_on(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value)
         VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn delete_setting_on(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
    Ok(())
}

fn get_active_deck_id_on(conn: &Connection) -> Result<Option<i64>> {
    get_setting_on(conn, "active_deck_id")?
        .map(|value| {
            value.parse::<i64>().map_err(|error| {
                Error::InvalidInput(format!("invalid active_deck_id '{value}': {error}"))
            })
        })
        .transpose()
}

fn set_current_card_id_on(conn: &Connection, card_id: Option<i64>) -> Result<()> {
    match card_id {
        Some(card_id) => set_setting_on(conn, "current_card_id", &card_id.to_string()),
        None => delete_setting_on(conn, "current_card_id"),
    }
}

/// Sets (or clears) the active deck and always clears the current card in the same
/// transaction, so a crash never leaves "current card from the old deck" paired with
/// "new active deck" — see [`Storage::set_active_deck_id`].
fn set_active_deck_id_on(conn: &Connection, deck_id: Option<i64>) -> Result<()> {
    match deck_id {
        Some(deck_id) => set_setting_on(conn, "active_deck_id", &deck_id.to_string())?,
        None => delete_setting_on(conn, "active_deck_id")?,
    }
    set_current_card_id_on(conn, None)
}

/// Persists a review's FSRS state update and its review-log entry atomically: either
/// both writes land or neither does. See [`Storage::complete_review`].
fn record_review_on(conn: &Connection, review: &ScheduledReview) -> Result<()> {
    conn.execute(
        "UPDATE card_state
         SET stability = ?1,
             difficulty = ?2,
             due = ?3,
             reps = reps + 1,
             lapses = lapses + ?4,
             state = ?5
         WHERE card_id = ?6",
        params![
            review.stability,
            review.difficulty,
            review.due,
            i64::from(matches!(review.rating, crate::card::Rating::Again)),
            review.state.to_string(),
            review.card_id
        ],
    )?;
    conn.execute(
        "INSERT INTO review_log
         (card_id, rating, reviewed_at, elapsed_days, scheduled_days)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            review.card_id,
            review.rating.as_i64(),
            review.reviewed_at,
            review.elapsed_days,
            review.scheduled_days
        ],
    )?;
    Ok(())
}

fn import_cards_on(
    conn: &Connection,
    deck: &Deck,
    cards: &[ImportCard],
    duplicate_strategy: DuplicateStrategy,
) -> Result<ImportSummary> {
    let mut summary = ImportSummary {
        deck_id: deck.id,
        deck_name: deck.name.clone(),
        input_count: cards.len(),
        inserted: 0,
        updated: 0,
        skipped: 0,
        merged: 0,
    };

    for import_card in cards {
        let existing = find_first_card_by_word_on(conn, deck.id, &import_card.word)?;
        match (existing, duplicate_strategy) {
            (None, _) | (Some(_), DuplicateStrategy::Keep) => {
                insert_card_with_metadata_on(conn, deck.id, import_card)?;
                summary.inserted += 1;
            }
            (Some(_), DuplicateStrategy::Skip) => {
                summary.skipped += 1;
            }
            (Some(existing), DuplicateStrategy::Overwrite) => {
                update_card_from_import_on(conn, existing.id, import_card)?;
                summary.updated += 1;
            }
            (Some(existing), DuplicateStrategy::Merge) => {
                let merged = crate::importer::merge_import_card(&existing, import_card);
                update_card_from_import_on(conn, existing.id, &merged)?;
                summary.merged += 1;
            }
        }
    }

    Ok(summary)
}

fn insert_card_with_metadata_on(
    conn: &Connection,
    deck_id: i64,
    card: &ImportCard,
) -> Result<Card> {
    let meanings_json = serde_json::to_string(&card.meanings)?;
    let pronunciations_json = serde_json::to_string(&card.pronunciations)?;
    let tags_json = serde_json::to_string(&card.tags)?;
    conn.execute(
        "INSERT INTO cards
         (deck_id, word, language, meanings, pronunciations, tags, source_name, source_license)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            deck_id,
            card.word,
            card.language,
            meanings_json,
            pronunciations_json,
            tags_json,
            card.source.as_ref().map(|source| source.name.as_str()),
            card.source
                .as_ref()
                .and_then(|source| source.license.as_deref())
        ],
    )?;
    let id = conn.last_insert_rowid();

    conn.execute(
        "INSERT OR IGNORE INTO card_state (card_id) VALUES (?1)",
        params![id],
    )?;

    get_card_by_id_on(conn, id)?.ok_or_else(|| Error::NotFound(format!("card id {id}")))
}

fn get_card_by_id_on(conn: &Connection, id: i64) -> Result<Option<Card>> {
    let result = conn.query_row(
        "SELECT id, deck_id, word, language, meanings, pronunciations, tags,
                source_name, source_license, created_at
         FROM cards WHERE id = ?1",
        params![id],
        card_from_row,
    );
    match result {
        Ok(card) => Ok(Some(card)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(Error::Db(e)),
    }
}

fn find_first_card_by_word_on(conn: &Connection, deck_id: i64, word: &str) -> Result<Option<Card>> {
    let result = conn.query_row(
        "SELECT id, deck_id, word, language, meanings, pronunciations, tags,
                source_name, source_license, created_at
         FROM cards
         WHERE deck_id = ?1 AND word = ?2
         ORDER BY id
         LIMIT 1",
        params![deck_id, word],
        card_from_row,
    );
    match result {
        Ok(card) => Ok(Some(card)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(Error::Db(e)),
    }
}

fn update_card_from_import_on(conn: &Connection, id: i64, card: &ImportCard) -> Result<()> {
    let meanings_json = serde_json::to_string(&card.meanings)?;
    let pronunciations_json = serde_json::to_string(&card.pronunciations)?;
    let tags_json = serde_json::to_string(&card.tags)?;
    conn.execute(
        "UPDATE cards
         SET word = ?1,
             language = ?2,
             meanings = ?3,
             pronunciations = ?4,
             tags = ?5,
             source_name = ?6,
             source_license = ?7
         WHERE id = ?8",
        params![
            card.word,
            card.language,
            meanings_json,
            pronunciations_json,
            tags_json,
            card.source.as_ref().map(|source| source.name.as_str()),
            card.source
                .as_ref()
                .and_then(|source| source.license.as_deref()),
            id
        ],
    )?;
    Ok(())
}

fn card_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Card> {
    let meanings_json = row.get::<_, String>(4)?;
    let pronunciations_json = row.get::<_, String>(5)?;
    let tags_json = row.get::<_, String>(6)?;
    let source_name = row.get::<_, Option<String>>(7)?;
    Ok(Card {
        id: row.get(0)?,
        deck_id: row.get(1)?,
        word: row.get(2)?,
        language: row.get(3)?,
        meanings: serde_json::from_str(&meanings_json).map_err(json_to_sql_error)?,
        pronunciations: serde_json::from_str(&pronunciations_json).map_err(json_to_sql_error)?,
        tags: serde_json::from_str(&tags_json).map_err(json_to_sql_error)?,
        source: source_name.map(|name| Source {
            name,
            license: row.get::<_, Option<String>>(8).ok().flatten(),
        }),
        created_at: row.get(9)?,
    })
}

fn json_to_sql_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn ensure_card_metadata_columns(conn: &Connection) -> Result<()> {
    let columns = table_columns(conn, "cards")?;
    if columns.is_empty() {
        // Table doesn't exist yet — migrations will create it with all columns.
        return Ok(());
    }
    let missing = [
        (
            "language",
            "ALTER TABLE cards ADD COLUMN language TEXT NOT NULL DEFAULT 'en'",
        ),
        (
            "tags",
            "ALTER TABLE cards ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'",
        ),
        (
            "source_name",
            "ALTER TABLE cards ADD COLUMN source_name TEXT",
        ),
        (
            "source_license",
            "ALTER TABLE cards ADD COLUMN source_license TEXT",
        ),
    ];

    for (name, statement) in missing {
        if !columns.iter().any(|column| column == name) {
            conn.execute(statement, [])?;
        }
    }

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cards_deck_word ON cards(deck_id, word)",
        [],
    )?;
    Ok(())
}

fn ensure_deck_metadata_columns(conn: &Connection) -> Result<()> {
    let columns = table_columns(conn, "decks")?;
    if columns.is_empty() {
        return Ok(());
    }
    if !columns.iter().any(|column| column == "catalog_id") {
        conn.execute("ALTER TABLE decks ADD COLUMN catalog_id TEXT", [])?;
    }
    // SQLite treats multiple NULLs as distinct for UNIQUE constraints, so manually
    // created decks (catalog_id = NULL) never collide here; only two decks claiming
    // the same non-null catalog id would.
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_decks_catalog_id ON decks(catalog_id)",
        [],
    )?;
    Ok(())
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{card::Rating, scheduler::Scheduler};

    fn open_temp() -> Storage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("test.db");
        Storage::open(&path).unwrap()
    }

    #[test]
    fn test_migration_runs() {
        open_temp();
    }

    #[test]
    fn test_deck_insert_and_list() {
        let storage = open_temp();
        let deck = storage
            .insert_deck("cet4", Some("CET-4 vocabulary"))
            .unwrap();
        assert_eq!(deck.name, "cet4");
        assert_eq!(deck.description.as_deref(), Some("CET-4 vocabulary"));

        let decks = storage.list_decks().unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, "cet4");
        assert_eq!(decks[0].catalog_id, None);
    }

    #[test]
    fn test_catalog_id_round_trips_and_avoids_name_collision() {
        let storage = open_temp();

        // A manually created deck has no catalog_id.
        let manual = storage.insert_deck("CET-4", None).unwrap();
        assert_eq!(manual.catalog_id, None);
        assert!(storage
            .get_deck_by_catalog_id("kajweb:cet4")
            .unwrap()
            .is_none());

        // Creating a catalog-tagged deck with a *different* name persists catalog_id
        // and is then discoverable by it.
        let (catalog_deck, _) = storage
            .import_cards_into_new_deck_with_catalog_id(
                "CET-4 (catalog)",
                None,
                &[],
                DuplicateStrategy::Merge,
                Some("kajweb:cet4"),
            )
            .unwrap();
        assert_eq!(catalog_deck.catalog_id.as_deref(), Some("kajweb:cet4"));
        let found = storage
            .get_deck_by_catalog_id("kajweb:cet4")
            .unwrap()
            .unwrap();
        assert_eq!(found.id, catalog_deck.id);

        // The manually created deck with the colliding display name is untouched and
        // still not associated with any catalog id.
        let manual_again = storage.get_deck_by_id(manual.id).unwrap().unwrap();
        assert_eq!(manual_again.catalog_id, None);

        // A second catalog deck under a different catalog_id can coexist.
        let (other, _) = storage
            .import_cards_into_new_deck_with_catalog_id(
                "CET-6",
                None,
                &[],
                DuplicateStrategy::Merge,
                Some("kajweb:cet6"),
            )
            .unwrap();
        assert_eq!(other.catalog_id.as_deref(), Some("kajweb:cet6"));
        assert!(storage
            .get_deck_by_catalog_id("kajweb:toefl")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_card_insert_and_list() {
        let storage = open_temp();
        let deck = storage.insert_deck("cet4", None).unwrap();

        let meanings = vec![Meaning {
            part_of_speech: "v.".to_string(),
            definition: "取消，撤销".to_string(),
            example: Some("cancel a meeting".to_string()),
        }];
        let pronunciations = vec![Pronunciation {
            notation: "/ˈkænsl/".to_string(),
            audio_url: None,
        }];
        let card = storage
            .insert_card(deck.id, "cancel", &meanings, &pronunciations)
            .unwrap();
        assert_eq!(card.word, "cancel");
        assert_eq!(card.meanings.len(), 1);
        assert_eq!(card.meanings[0].definition, "取消，撤销");

        let cards = storage.list_cards_by_deck("cet4").unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].word, "cancel");
        assert_eq!(cards[0].pronunciations[0].notation, "/ˈkænsl/");
    }

    #[test]
    fn test_card_state_created_on_insert() {
        let storage = open_temp();
        let deck = storage.insert_deck("test", None).unwrap();
        let card = storage.insert_card(deck.id, "hello", &[], &[]).unwrap();
        let state = storage.get_card_state(card.id).unwrap();
        assert!(state.is_some());
        assert!(matches!(state.unwrap().state, ReviewState::New));
    }

    #[test]
    fn test_import_cards_rolls_back_batch_on_write_error() {
        let storage = open_temp();
        let deck = storage.insert_deck("test", None).unwrap();
        storage
            .conn
            .execute_batch(
                "
                CREATE TRIGGER reject_boom_import
                BEFORE INSERT ON cards
                WHEN NEW.word = 'boom'
                BEGIN
                    SELECT RAISE(ABORT, 'boom import rejected');
                END;
                ",
            )
            .unwrap();

        let cards = vec![
            ImportCard {
                word: "first".to_string(),
                language: "en".to_string(),
                meanings: Vec::new(),
                pronunciations: Vec::new(),
                tags: Vec::new(),
                source: None,
            },
            ImportCard {
                word: "boom".to_string(),
                language: "en".to_string(),
                meanings: Vec::new(),
                pronunciations: Vec::new(),
                tags: Vec::new(),
                source: None,
            },
        ];

        let result = storage.import_cards(deck.id, &cards, DuplicateStrategy::Merge);

        assert!(result.is_err());
        assert_eq!(storage.card_count_by_deck(deck.id).unwrap(), 0);
        let card_state_count: i64 = storage
            .conn
            .query_row("SELECT COUNT(*) FROM card_state", [], |row| row.get(0))
            .unwrap();
        assert_eq!(card_state_count, 0);
    }

    #[test]
    fn test_import_cards_into_new_deck_rolls_back_deck_on_write_error() {
        let storage = open_temp();
        storage
            .conn
            .execute_batch(
                "
                CREATE TRIGGER reject_boom_new_deck_import
                BEFORE INSERT ON cards
                WHEN NEW.word = 'boom'
                BEGIN
                    SELECT RAISE(ABORT, 'boom import rejected');
                END;
                ",
            )
            .unwrap();

        let cards = vec![ImportCard {
            word: "boom".to_string(),
            language: "en".to_string(),
            meanings: Vec::new(),
            pronunciations: Vec::new(),
            tags: Vec::new(),
            source: None,
        }];

        let result =
            storage.import_cards_into_new_deck("new-deck", None, &cards, DuplicateStrategy::Merge);

        assert!(result.is_err());
        assert!(storage.get_deck_by_name("new-deck").unwrap().is_none());
        let card_count: i64 = storage
            .conn
            .query_row("SELECT COUNT(*) FROM cards", [], |row| row.get(0))
            .unwrap();
        assert_eq!(card_count, 0);
    }

    #[test]
    fn test_complete_review_rolls_back_on_write_error() {
        use crate::card::Rating;
        use crate::scheduler::ScheduledReview;

        let storage = open_temp();
        let deck = storage.insert_deck("test", None).unwrap();
        let card = storage.insert_card(deck.id, "hello", &[], &[]).unwrap();

        // Force the review_log insert (the second write in record_review_on) to fail,
        // so we can verify the preceding card_state update is rolled back too, and
        // that set_current_card_id never runs.
        storage
            .conn
            .execute_batch(
                "
                CREATE TRIGGER reject_review_log_insert
                BEFORE INSERT ON review_log
                BEGIN
                    SELECT RAISE(ABORT, 'review_log insert rejected');
                END;
                ",
            )
            .unwrap();

        let before = storage.get_card_state(card.id).unwrap().unwrap();

        let review = ScheduledReview {
            card_id: card.id,
            rating: Rating::Good,
            reviewed_at: "2026-01-01 00:00:00".to_string(),
            due: "2026-01-05 00:00:00".to_string(),
            elapsed_days: 0,
            scheduled_days: 4,
            stability: 9.9,
            difficulty: 5.5,
            state: crate::card::ReviewState::Review,
        };

        let result = storage.complete_review(&review);
        assert!(result.is_err());

        // card_state must be unchanged — the UPDATE was rolled back along with the
        // failed INSERT, not partially applied.
        let after = storage.get_card_state(card.id).unwrap().unwrap();
        assert_eq!(after.stability, before.stability);
        assert_eq!(after.due, before.due);
        assert_eq!(after.reps, before.reps);

        // current_card_id must not have been set either.
        assert_eq!(storage.get_current_card_id().unwrap(), None);

        let log_count: i64 = storage
            .conn
            .query_row("SELECT COUNT(*) FROM review_log", [], |row| row.get(0))
            .unwrap();
        assert_eq!(log_count, 0);
    }

    #[test]
    fn test_delete_deck_and_active_pointer_update_are_atomic() {
        let storage = open_temp();
        let deck = storage.insert_deck("active-one", None).unwrap();
        storage.set_active_deck_id(Some(deck.id)).unwrap();
        assert_eq!(storage.get_active_deck_id().unwrap(), Some(deck.id));

        storage.delete_deck(deck.id).unwrap();

        // Deleting the active deck must clear the active pointer in the same
        // transaction as the delete — never leaving it pointing at a deleted id.
        assert_eq!(storage.get_active_deck_id().unwrap(), None);
        assert!(storage.get_deck_by_id(deck.id).unwrap().is_none());
    }

    #[test]
    fn test_list_cards_empty_deck() {
        let storage = open_temp();
        storage.insert_deck("empty", None).unwrap();
        let cards = storage.list_cards_by_deck("empty").unwrap();
        assert!(cards.is_empty());
    }

    #[test]
    fn test_list_cards_by_deck_paginated() {
        let storage = open_temp();
        let deck = storage.insert_deck("cet4", None).unwrap();
        storage.insert_card(deck.id, "first", &[], &[]).unwrap();
        storage.insert_card(deck.id, "second", &[], &[]).unwrap();
        storage.insert_card(deck.id, "third", &[], &[]).unwrap();

        let first_page = storage.list_cards_by_deck_paginated(deck.id, 2, 0).unwrap();
        let second_page = storage.list_cards_by_deck_paginated(deck.id, 2, 2).unwrap();

        assert_eq!(storage.card_count_by_deck(deck.id).unwrap(), 3);
        assert_eq!(
            first_page
                .iter()
                .map(|card| card.word.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert_eq!(
            second_page
                .iter()
                .map(|card| card.word.as_str())
                .collect::<Vec<_>>(),
            vec!["third"]
        );
    }

    #[test]
    fn test_active_deck_setting_clears_current_card() {
        let storage = open_temp();
        let deck = storage.insert_deck("cet4", None).unwrap();
        let card = storage.insert_card(deck.id, "cancel", &[], &[]).unwrap();

        storage.set_current_card_id(Some(card.id)).unwrap();
        storage.set_active_deck_id(Some(deck.id)).unwrap();

        assert_eq!(storage.get_active_deck_id().unwrap(), Some(deck.id));
        assert_eq!(storage.get_active_deck().unwrap().unwrap().name, "cet4");
        assert_eq!(storage.get_current_card_id().unwrap(), None);
    }

    #[test]
    fn test_progress_counts_are_scoped_by_deck() {
        let storage = open_temp();
        let first_deck = storage.insert_deck("first", None).unwrap();
        let second_deck = storage.insert_deck("second", None).unwrap();
        storage
            .insert_card(first_deck.id, "same", &[], &[])
            .unwrap();
        storage
            .insert_card(second_deck.id, "same", &[], &[])
            .unwrap();

        let first_progress = storage.progress_counts_by_deck(first_deck.id).unwrap();
        let second_progress = storage.progress_counts_by_deck(second_deck.id).unwrap();

        assert_eq!(first_progress.new_remaining, 1);
        assert_eq!(second_progress.new_remaining, 1);
    }

    #[test]
    fn test_progress_counts_subtract_new_cards_reviewed_today() {
        let storage = open_temp();
        storage.set_daily_new_limit(2).unwrap();
        let deck = storage.insert_deck("test", None).unwrap();
        let first = storage.insert_card(deck.id, "first", &[], &[]).unwrap();
        storage.insert_card(deck.id, "second", &[], &[]).unwrap();
        storage.insert_card(deck.id, "third", &[], &[]).unwrap();

        let before = storage.progress_counts_by_deck(deck.id).unwrap();
        assert_eq!(before.new_remaining, 2);

        Scheduler::review(&storage, first.id, Rating::Easy).unwrap();

        let after = storage.progress_counts_by_deck(deck.id).unwrap();
        assert_eq!(after.new_remaining, 1);
        assert_eq!(after.reviewed_today, 1);
    }

    #[test]
    fn test_daily_new_limit_defaults_when_unset() {
        let storage = open_temp();
        assert_eq!(storage.daily_new_limit().unwrap(), DEFAULT_DAILY_NEW_LIMIT);
    }

    #[test]
    fn test_daily_new_limit_round_trips_through_settings() {
        let storage = open_temp();
        storage.set_daily_new_limit(80).unwrap();
        assert_eq!(storage.daily_new_limit().unwrap(), 80);
    }

    #[test]
    fn test_daily_new_limit_rejects_non_positive_values() {
        let storage = open_temp();
        assert!(storage.set_daily_new_limit(0).is_err());
        assert!(storage.set_daily_new_limit(-5).is_err());
        assert_eq!(storage.daily_new_limit().unwrap(), DEFAULT_DAILY_NEW_LIMIT);
    }

    #[test]
    fn test_daily_new_limit_rejects_corrupted_setting() {
        let storage = open_temp();
        set_setting_on(&storage.conn, DAILY_NEW_LIMIT_KEY, "many").unwrap();
        assert!(storage.daily_new_limit().is_err());
    }

    #[test]
    fn test_reviewing_more_than_the_limit_does_not_produce_a_negative_quota() {
        let storage = open_temp();
        storage.set_daily_new_limit(2).unwrap();
        let deck = storage.insert_deck("test", None).unwrap();
        for word in ["first", "second", "third"] {
            let card = storage.insert_card(deck.id, word, &[], &[]).unwrap();
            Scheduler::review(&storage, card.id, Rating::Easy).unwrap();
        }

        // Lowering the limit below today's already-introduced count must clamp at
        // zero instead of wrapping into a huge unsigned value downstream.
        let progress = storage.progress_counts_by_deck(deck.id).unwrap();
        assert_eq!(progress.new_remaining, 0);
        assert_eq!(progress.new_reviewed_today, 3);
    }

    #[test]
    fn test_default_path_is_some() {
        // Just ensure it doesn't error on this platform.
        let path = Storage::default_path().unwrap();
        assert!(path.to_str().unwrap().contains("fishword"));
    }

    #[test]
    fn test_open_upgrades_m1_cards_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("old.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE decks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE cards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                deck_id INTEGER NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
                word TEXT NOT NULL,
                meanings TEXT NOT NULL DEFAULT '[]',
                pronunciations TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            ",
        )
        .unwrap();
        drop(conn);

        let storage = Storage::open(&path).unwrap();
        let deck = storage.insert_deck("legacy", None).unwrap();
        let card = storage.insert_card(deck.id, "hello", &[], &[]).unwrap();
        assert_eq!(card.language, "en");
        assert!(card.tags.is_empty());
    }

    #[test]
    fn test_open_adds_catalog_id_column_to_existing_decks_table() {
        // Simulate an existing user database created before `catalog_id` existed:
        // a `decks` table with the original (pre-catalog) column set, already
        // containing a deck row.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("pre-catalog.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE decks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO decks (name, description) VALUES ('old-deck', 'created before catalog_id existed');
            ",
        )
        .unwrap();
        drop(conn);

        // Opening with the current schema must add the column without losing the
        // existing row, and the pre-existing deck must come back with catalog_id = None
        // rather than failing to open or erroring on the missing column.
        let storage = Storage::open(&path).unwrap();
        let decks = storage.list_decks().unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, "old-deck");
        assert_eq!(decks[0].catalog_id, None);

        // The deck should still be usable for catalog fetches afterwards (no
        // accidental match against an empty/None catalog_id).
        assert!(storage.get_deck_by_catalog_id("cet4").unwrap().is_none());
    }

    #[test]
    fn test_open_removes_empty_card_tags() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("tags.db");
        // Simulate a pre-migration database: run only M1 without setting user_version,
        // so Storage::open() will trigger migration 0002 (clean_empty_tags).
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../../../../migrations/0001_init.sql"))
            .unwrap();
        conn.execute(
            "INSERT INTO decks (name, description) VALUES (?1, ?2)",
            params!["legacy", Option::<String>::None],
        )
        .unwrap();
        let deck_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO cards
             (deck_id, word, language, meanings, pronunciations, tags, source_name, source_license)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                deck_id,
                "cancel",
                "en",
                "[]",
                "[]",
                r#"["", "anki", ""]"#,
                Option::<String>::None,
                Option::<String>::None
            ],
        )
        .unwrap();
        drop(conn);

        let storage = Storage::open(&path).unwrap();
        let cards = storage.list_cards_by_deck("legacy").unwrap();

        assert_eq!(cards[0].tags, vec!["anki"]);
    }

    #[test]
    fn test_open_preserves_non_empty_legacy_tags_while_cleaning_empty_tags() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("legacy-tags.db");
        // Simulate a pre-migration database: run only M1 without setting user_version,
        // so Storage::open() will trigger migration 0002 (clean_empty_tags).
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../../../../migrations/0001_init.sql"))
            .unwrap();
        conn.execute(
            "INSERT INTO decks (name, description) VALUES (?1, ?2)",
            params!["CET-4", Option::<String>::None],
        )
        .unwrap();
        let deck_id = conn.last_insert_rowid();
        for (word, tags) in [
            ("cancel", r#"["", "CET-4", "review", ""]"#),
            ("abandon", r#"["hard", "anki"]"#),
        ] {
            conn.execute(
                "INSERT INTO cards
                 (deck_id, word, language, meanings, pronunciations, tags, source_name, source_license)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    deck_id,
                    word,
                    "en",
                    "[]",
                    "[]",
                    tags,
                    Option::<String>::None,
                    Option::<String>::None
                ],
            )
            .unwrap();
        }
        drop(conn);

        drop(Storage::open(&path).unwrap());
        let storage = Storage::open(&path).unwrap();
        let cards = storage.list_cards_by_deck("CET-4").unwrap();

        assert_eq!(cards[0].tags, vec!["CET-4", "review"]);
        assert_eq!(cards[1].tags, vec!["hard", "anki"]);
    }
}
