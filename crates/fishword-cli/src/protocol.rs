use chrono::{Duration, NaiveDate, Utc};
use serde::Serialize;

use fishword_core::{
    card::Card,
    deck::Deck,
    scheduler::ScheduledReview,
    selector::{SelectedCard, SelectionReason},
    storage::{DailyReviewBucket, DailyReviewStats, ProgressCounts},
};

pub const CURRENT_SCHEMA: &str = "fishword.protocol.current.v1";
pub const NEXT_SCHEMA: &str = "fishword.protocol.next.v1";
pub const RATE_SCHEMA: &str = "fishword.protocol.rate.v1";
pub const ERROR_SCHEMA: &str = "fishword.protocol.error.v1";
pub const DECKS_SCHEMA: &str = "fishword.protocol.decks.v1";
pub const DECK_USE_SCHEMA: &str = "fishword.protocol.deck_use.v1";
pub const DECK_CREATE_SCHEMA: &str = "fishword.protocol.deck_create.v1";
pub const DECK_DELETE_SCHEMA: &str = "fishword.protocol.deck_delete.v1";
pub const DECK_RENAME_SCHEMA: &str = "fishword.protocol.deck_rename.v1";
pub const STATUS_SCHEMA: &str = "fishword.protocol.status.v1";
pub const STATS_SCHEMA: &str = "fishword.protocol.stats.v1";
pub const CARD_LIST_SCHEMA: &str = "fishword.protocol.card_list.v1";
pub const IMPORT_SCHEMA: &str = "fishword.protocol.import.v1";
pub const CATALOG_LIST_SCHEMA: &str = "fishword.protocol.catalog_list.v1";
pub const CATALOG_FETCH_SCHEMA: &str = "fishword.protocol.catalog_fetch.v1";
pub const DAILY_LIMIT_SCHEMA: &str = "fishword.protocol.daily_limit.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    Plain,
    Compact,
    Status,
}

impl std::str::FromStr for TextFormat {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "plain" => Ok(Self::Plain),
            "compact" => Ok(Self::Compact),
            "status" => Ok(Self::Status),
            other => Err(format!("unknown format: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CardResponse {
    pub schema: &'static str,
    pub card: ProtocolCard,
    pub display: DisplayFields,
    pub progress: ProgressFields,
    pub selection: SelectionFields,
}

#[derive(Debug, Clone, Serialize)]
pub struct RateResponse {
    pub schema: &'static str,
    pub card: ProtocolCard,
    pub display: DisplayFields,
    pub progress: ProgressFields,
    pub review: ReviewFields,
    /// 评分后自动推进的下一张卡片，今日无更多卡片时为 null。
    pub next: Option<CardResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckListResponse {
    pub schema: &'static str,
    pub decks: Vec<DeckItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckItem {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckUseResponse {
    pub schema: &'static str,
    pub name: String,
    pub description: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckMutationFields {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckCreateResponse {
    pub schema: &'static str,
    pub deck: DeckMutationFields,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckDeleteResponse {
    pub schema: &'static str,
    pub deleted: DeckDeletedFields,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckDeletedFields {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckRenameResponse {
    pub schema: &'static str,
    pub deck: DeckMutationFields,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyLimitResponse {
    pub schema: &'static str,
    /// Effective limit the selector will use for every deck.
    pub daily_new_limit: i64,
    /// Value used when the user never set the limit.
    pub default_limit: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogDeckEntry {
    pub id: String,
    pub slug: String,
    pub source_id: String,
    pub name: String,
    pub description: Option<String>,
    pub language: String,
    pub word_count: u64,
    pub tags: Vec<String>,
    pub source: Option<fishword_core::card::Source>,
    pub url: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogListResponse {
    pub schema: &'static str,
    pub decks: Vec<CatalogDeckEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogFetchResponse {
    pub schema: &'static str,
    pub catalog_id: String,
    pub slug: String,
    pub source_id: String,
    pub name: String,
    pub import: ImportResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResponse {
    pub schema: &'static str,
    pub deck_id: i64,
    pub deck: String,
    pub input: usize,
    pub inserted: usize,
    pub updated: usize,
    pub merged: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardListResponse {
    pub schema: &'static str,
    pub deck: DeckFields,
    pub cards: Vec<ProtocolCard>,
    pub pagination: PaginationFields,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaginationFields {
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
    pub page_count: i64,
    pub has_previous: bool,
    pub has_next: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusResponse {
    pub schema: &'static str,
    pub deck: DeckFields,
    pub mode: String,
    pub today: TodayStatusFields,
    pub display: StatusDisplayFields,
    pub next_action: NextActionFields,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodayStatusFields {
    pub due: i64,
    pub new_remaining: i64,
    pub new_today: i64,
    pub reviewed: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusDisplayFields {
    pub plain: String,
    pub compact: String,
    pub statusline: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NextActionFields {
    pub kind: String,
    pub label: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsResponse {
    pub schema: &'static str,
    pub deck: DeckFields,
    pub range: StatsRangeFields,
    pub summary: StatsSummaryFields,
    pub series: Vec<DailyStatsFields>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsRangeFields {
    pub kind: String,
    pub days: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsSummaryFields {
    pub reviews: i64,
    pub reviewed_today: i64,
    pub good_or_easy_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyStatsFields {
    pub date: String,
    pub reviews: i64,
    pub again: i64,
    pub hard: i64,
    pub good: i64,
    pub easy: i64,
    pub good_or_easy_rate: Option<f64>,
}

impl DeckUseResponse {
    pub fn new(deck: &Deck) -> Self {
        let display = deck
            .description
            .clone()
            .unwrap_or_else(|| deck.name.clone());
        Self {
            schema: DECK_USE_SCHEMA,
            name: deck.name.clone(),
            description: deck.description.clone(),
            message: format!("Active deck: {display}"),
        }
    }
}

impl DeckCreateResponse {
    pub fn new(deck: &Deck) -> Self {
        Self {
            schema: DECK_CREATE_SCHEMA,
            deck: DeckMutationFields {
                id: deck.id,
                name: deck.name.clone(),
                description: deck.description.clone(),
                created_at: deck.created_at.clone(),
            },
        }
    }
}

impl DeckDeleteResponse {
    pub fn new(deck: &Deck) -> Self {
        Self {
            schema: DECK_DELETE_SCHEMA,
            deleted: DeckDeletedFields {
                id: deck.id,
                name: deck.name.clone(),
            },
        }
    }
}

impl DeckRenameResponse {
    pub fn new(deck: &Deck) -> Self {
        Self {
            schema: DECK_RENAME_SCHEMA,
            deck: DeckMutationFields {
                id: deck.id,
                name: deck.name.clone(),
                description: deck.description.clone(),
                created_at: deck.created_at.clone(),
            },
        }
    }
}

impl CardListResponse {
    pub fn new(deck: &Deck, cards: Vec<Card>, page: i64, page_size: i64, total: i64) -> Self {
        let page_count = if total == 0 {
            0
        } else {
            (total + page_size - 1) / page_size
        };
        Self {
            schema: CARD_LIST_SCHEMA,
            deck: deck_fields(deck),
            cards: cards
                .into_iter()
                .map(|card| protocol_card(&card, deck))
                .collect(),
            pagination: PaginationFields {
                page,
                page_size,
                total,
                page_count,
                has_previous: page > 1 && page_count > 0,
                has_next: page < page_count,
            },
        }
    }
}

impl StatusResponse {
    pub fn new(deck: &Deck, progress: ProgressCounts, card_count: i64) -> Self {
        let deck_fields = deck_fields(deck);
        let mode = if card_count == 0 {
            "empty"
        } else if progress.due_count > 0 || progress.new_remaining > 0 {
            "review"
        } else {
            "complete"
        };
        let deck_name = deck_fields.name.as_str();
        let compact = if mode == "complete" {
            format!("all caught up · {} done", progress.reviewed_today)
        } else if mode == "empty" {
            "no cards".to_string()
        } else {
            format!(
                "{} due · {} new · {} done",
                progress.due_count, progress.new_remaining, progress.reviewed_today
            )
        };
        let plain = if mode == "complete" {
            format!(
                "{deck_name}: all caught up, {} reviewed today",
                progress.reviewed_today
            )
        } else if mode == "empty" {
            format!("{deck_name}: no cards")
        } else {
            format!(
                "{deck_name}: {} due, {} new, {} reviewed today",
                progress.due_count, progress.new_remaining, progress.reviewed_today
            )
        };
        let statusline = format!("📚 {deck_name} · {compact}");
        let command = format!("fishword current --deck {} --json", deck.id);
        Self {
            schema: STATUS_SCHEMA,
            deck: deck_fields,
            mode: mode.to_string(),
            today: TodayStatusFields {
                due: progress.due_count,
                new_remaining: progress.new_remaining,
                new_today: progress.new_reviewed_today,
                reviewed: progress.reviewed_today,
            },
            display: StatusDisplayFields {
                plain,
                compact,
                statusline,
            },
            next_action: NextActionFields {
                kind: if mode == "review" { "review" } else { "none" }.to_string(),
                label: if mode == "review" { "Continue" } else { "Rest" }.to_string(),
                command,
            },
        }
    }
}

impl StatsResponse {
    pub fn new(deck: &Deck, days: i64, buckets: Vec<DailyReviewBucket>) -> Self {
        let today = Utc::now().date_naive();
        let first_day = today - Duration::days(days.saturating_sub(1));
        Self::new_for_range(deck, days, first_day, buckets)
    }

    fn new_for_range(
        deck: &Deck,
        days: i64,
        first_day: NaiveDate,
        buckets: Vec<DailyReviewBucket>,
    ) -> Self {
        let mut series = Vec::new();
        for offset in 0..days {
            let date = first_day + Duration::days(offset);
            let date_string = date.to_string();
            let stats = buckets
                .iter()
                .find(|bucket| bucket.date == date_string)
                .map(|bucket| bucket.stats)
                .unwrap_or(DailyReviewStats {
                    reviews: 0,
                    again: 0,
                    hard: 0,
                    good: 0,
                    easy: 0,
                });
            series.push(DailyStatsFields {
                date: date_string,
                reviews: stats.reviews,
                again: stats.again,
                hard: stats.hard,
                good: stats.good,
                easy: stats.easy,
                good_or_easy_rate: good_or_easy_rate(stats.good, stats.easy, stats.reviews),
            });
        }
        let reviews = series.iter().map(|day| day.reviews).sum::<i64>();
        let good = series.iter().map(|day| day.good).sum::<i64>();
        let easy = series.iter().map(|day| day.easy).sum::<i64>();
        let reviewed_today = series.last().map(|day| day.reviews).unwrap_or(0);
        let rate = good_or_easy_rate(good, easy, reviews);
        Self {
            schema: STATS_SCHEMA,
            deck: deck_fields(deck),
            range: StatsRangeFields {
                kind: "days".to_string(),
                days,
            },
            summary: StatsSummaryFields {
                reviews,
                reviewed_today,
                good_or_easy_rate: rate,
            },
            series,
        }
    }
}

impl DeckListResponse {
    pub fn new(decks: Vec<Deck>, active_id: Option<i64>) -> Self {
        Self {
            schema: DECKS_SCHEMA,
            decks: decks
                .into_iter()
                .map(|d| DeckItem {
                    active: Some(d.id) == active_id,
                    id: d.id,
                    name: d.name,
                    description: d.description,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub schema: &'static str,
    pub error: ErrorFields,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorFields {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtocolCard {
    pub id: String,
    pub db_id: i64,
    pub term: String,
    pub language: String,
    pub phonetic: PhoneticFields,
    pub meanings: Vec<MeaningFields>,
    pub deck: DeckFields,
    pub tags: Vec<String>,
    pub source: Option<SourceFields>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeaningFields {
    pub part_of_speech: String,
    pub definition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhoneticFields {
    pub us: Option<String>,
    pub uk: Option<String>,
    pub other: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckFields {
    pub id: String,
    pub name: String,
    pub db_id: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceFields {
    pub name: String,
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DisplayFields {
    pub plain: String,
    pub compact: String,
    pub status: String,
    pub masked: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressFields {
    pub due_count: i64,
    pub new_today_remaining: i64,
    pub reviewed_today: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectionFields {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewFields {
    pub rating: String,
    pub due: String,
    pub elapsed_days: i64,
    pub scheduled_days: i64,
    pub stability: f64,
    pub difficulty: f64,
    pub state: String,
}

impl CardResponse {
    pub fn current(selected: &SelectedCard, deck: &Deck, progress: ProgressCounts) -> Self {
        Self::from_selected(CURRENT_SCHEMA, selected, deck, progress)
    }

    pub fn next(selected: &SelectedCard, deck: &Deck, progress: ProgressCounts) -> Self {
        Self::from_selected(NEXT_SCHEMA, selected, deck, progress)
    }

    fn from_selected(
        schema: &'static str,
        selected: &SelectedCard,
        deck: &Deck,
        progress: ProgressCounts,
    ) -> Self {
        let card = protocol_card(&selected.card, deck);
        let display = display_fields(&selected.card, deck);
        Self {
            schema,
            card,
            display,
            progress: progress_fields(progress),
            selection: SelectionFields {
                reason: selection_reason(selected.reason).to_string(),
            },
        }
    }
}

impl RateResponse {
    pub fn new(
        card: &Card,
        deck: &Deck,
        review: &ScheduledReview,
        progress: ProgressCounts,
        next: Option<(&SelectedCard, &Deck)>,
    ) -> Self {
        Self {
            schema: RATE_SCHEMA,
            card: protocol_card(card, deck),
            display: display_fields(card, deck),
            progress: progress_fields(progress),
            review: ReviewFields {
                rating: review.rating.to_string(),
                due: review.due.clone(),
                elapsed_days: review.elapsed_days,
                scheduled_days: review.scheduled_days,
                stability: review.stability,
                difficulty: review.difficulty,
                state: review.state.to_string(),
            },
            next: next
                .map(|(selected, next_deck)| CardResponse::next(selected, next_deck, progress)),
        }
    }
}

impl ErrorResponse {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            schema: ERROR_SCHEMA,
            error: ErrorFields {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}

pub fn render_card(response: &CardResponse, format: TextFormat) -> &str {
    match format {
        TextFormat::Plain => &response.display.plain,
        TextFormat::Compact => &response.display.compact,
        TextFormat::Status => &response.display.status,
    }
}

fn protocol_card(card: &Card, deck: &Deck) -> ProtocolCard {
    let phonetic = phonetic_fields(card);
    ProtocolCard {
        id: format!("{}:{}", deck.name, card.word),
        db_id: card.id,
        term: card.word.clone(),
        language: card.language.clone(),
        meanings: card
            .meanings
            .iter()
            .map(|meaning| MeaningFields {
                part_of_speech: meaning.part_of_speech.clone(),
                definition: meaning.definition.clone(),
                example: meaning.example.clone(),
            })
            .collect(),
        phonetic,
        deck: deck_fields(deck),
        tags: card.tags.clone(),
        source: card.source.as_ref().map(|source| SourceFields {
            name: source.name.clone(),
            license: source.license.clone(),
        }),
    }
}

fn deck_fields(deck: &Deck) -> DeckFields {
    DeckFields {
        id: deck.name.clone(),
        name: deck
            .description
            .clone()
            .unwrap_or_else(|| deck.name.clone()),
        db_id: deck.id,
    }
}

fn phonetic_fields(card: &Card) -> PhoneticFields {
    let mut us = None;
    let mut uk = None;
    let mut other = Vec::new();

    for pronunciation in &card.pronunciations {
        if let Some(value) = pronunciation.notation.strip_prefix("US ") {
            us = Some(value.to_string());
        } else if let Some(value) = pronunciation.notation.strip_prefix("UK ") {
            uk = Some(value.to_string());
        } else {
            other.push(pronunciation.notation.clone());
        }
    }

    PhoneticFields { us, uk, other }
}

fn display_fields(card: &Card, deck: &Deck) -> DisplayFields {
    let meanings = card
        .meanings
        .iter()
        .map(|meaning| meaning.definition.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let phonetic = display_phonetic(card);
    let plain = if phonetic.is_empty() {
        format!("{} - {}", card.word, meanings)
    } else {
        format!("{} {} - {}", card.word, phonetic, meanings)
    };
    let compact = if phonetic.is_empty() {
        format!("📚 {} {}", card.word, meanings)
    } else {
        format!("📚 {} /{}/ {}", card.word, phonetic, meanings)
    };
    let deck_name = deck.description.as_deref().unwrap_or(&deck.name);
    let status = if phonetic.is_empty() {
        format!("📚 {} | {} | {}", card.word, deck_name, meanings)
    } else {
        format!(
            "📚 {} /{}/ | {} | {}",
            card.word, phonetic, deck_name, meanings
        )
    };
    let masked = if phonetic.is_empty() {
        format!("📚 {}", mask_term(&card.word))
    } else {
        format!("📚 {} /{}/", mask_term(&card.word), phonetic)
    };
    DisplayFields {
        plain,
        compact,
        status,
        masked,
    }
}

fn display_phonetic(card: &Card) -> String {
    let fields = phonetic_fields(card);
    fields
        .us
        .or(fields.uk)
        .or_else(|| fields.other.first().cloned())
        .unwrap_or_default()
}

fn mask_term(term: &str) -> String {
    let chars = term.chars().collect::<Vec<_>>();
    match chars.as_slice() {
        [] => String::new(),
        [only] => only.to_string(),
        [first, rest @ ..] => {
            let mut masked = first.to_string();
            masked.push_str(&"_".repeat(rest.len()));
            masked
        }
    }
}

fn progress_fields(progress: ProgressCounts) -> ProgressFields {
    ProgressFields {
        due_count: progress.due_count,
        new_today_remaining: progress.new_remaining,
        reviewed_today: progress.reviewed_today,
    }
}

fn selection_reason(reason: SelectionReason) -> &'static str {
    match reason {
        SelectionReason::Due => "due",
        SelectionReason::New => "new",
    }
}

fn good_or_easy_rate(good: i64, easy: i64, reviews: i64) -> Option<f64> {
    (reviews > 0).then_some((good + easy) as f64 / reviews as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fishword_core::{
        card::{Meaning, Pronunciation, Rating, ReviewState, Source},
        scheduler::ScheduledReview,
        selector::SelectedCard,
        storage::ProgressCounts,
    };

    fn sample_card() -> Card {
        Card {
            id: 1,
            deck_id: 1,
            word: "cancel".to_string(),
            language: "en".to_string(),
            meanings: vec![Meaning {
                part_of_speech: "".to_string(),
                definition: "取消，撤销；删去".to_string(),
                example: None,
            }],
            pronunciations: vec![
                Pronunciation {
                    notation: "US 'kænsl".to_string(),
                    audio_url: None,
                },
                Pronunciation {
                    notation: "UK 'kænsl".to_string(),
                    audio_url: None,
                },
            ],
            tags: vec!["cet4".to_string()],
            source: Some(Source {
                name: "qwerty-learner".to_string(),
                license: Some("GPL-3.0".to_string()),
            }),
            created_at: "2026-06-10 00:00:00".to_string(),
        }
    }

    fn sample_deck() -> Deck {
        Deck {
            id: 1,
            name: "cet4".to_string(),
            description: Some("CET-4".to_string()),
            created_at: "2026-06-10 00:00:00".to_string(),
            catalog_id: None,
        }
    }

    #[test]
    fn current_response_serializes_stable_fields() {
        let selected = SelectedCard {
            card: sample_card(),
            reason: SelectionReason::New,
        };
        let response = CardResponse::current(
            &selected,
            &sample_deck(),
            ProgressCounts {
                due_count: 1,
                new_remaining: 8,
                new_reviewed_today: 2,
                reviewed_today: 5,
            },
        );
        let value = serde_json::to_value(response).unwrap();
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../fixtures/protocol_current_sample.json"
        ))
        .unwrap();
        assert_eq!(value, fixture);
    }

    #[test]
    fn current_response_serializes_due_reason() {
        let selected = SelectedCard {
            card: sample_card(),
            reason: SelectionReason::Due,
        };
        let response = CardResponse::current(
            &selected,
            &sample_deck(),
            ProgressCounts {
                due_count: 1,
                new_remaining: 8,
                new_reviewed_today: 2,
                reviewed_today: 5,
            },
        );
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["selection"]["reason"], "due");
    }

    #[test]
    fn error_response_serializes() {
        let value = serde_json::to_value(ErrorResponse::new(
            "no_cards",
            "No cards found. Import a deck first.",
        ))
        .unwrap();
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../fixtures/protocol_error_sample.json"
        ))
        .unwrap();
        assert_eq!(value, fixture);
    }

    #[test]
    fn deck_list_response_serializes_stable_fields() {
        let response = DeckListResponse::new(vec![sample_deck()], Some(1));
        let value = serde_json::to_value(response).unwrap();
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../fixtures/protocol_decks_sample.json"
        ))
        .unwrap();
        assert_eq!(value, fixture);
    }

    #[test]
    fn deck_use_response_serializes_stable_fields() {
        let response = DeckUseResponse::new(&sample_deck());
        let value = serde_json::to_value(response).unwrap();
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../fixtures/protocol_deck_use_sample.json"
        ))
        .unwrap();
        assert_eq!(value, fixture);
    }

    #[test]
    fn card_list_response_serializes_stable_fields() {
        let response = CardListResponse::new(&sample_deck(), vec![sample_card()], 1, 50, 1);
        let value = serde_json::to_value(response).unwrap();
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../fixtures/protocol_card_list_sample.json"
        ))
        .unwrap();
        assert_eq!(value, fixture);
    }

    #[test]
    fn rate_response_serializes_stable_fields() {
        let review = ScheduledReview {
            card_id: 1,
            rating: Rating::Good,
            reviewed_at: "2026-06-10 00:00:00".to_string(),
            due: "2026-06-14 00:00:00".to_string(),
            elapsed_days: 0,
            scheduled_days: 4,
            stability: 4.0,
            difficulty: 5.0,
            state: ReviewState::Learning,
        };
        let response = RateResponse::new(
            &sample_card(),
            &sample_deck(),
            &review,
            ProgressCounts {
                due_count: 0,
                new_remaining: 19,
                new_reviewed_today: 1,
                reviewed_today: 1,
            },
            None,
        );
        let value = serde_json::to_value(response).unwrap();
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../fixtures/protocol_rate_sample.json"
        ))
        .unwrap();
        assert_eq!(value, fixture);
    }

    #[test]
    fn status_response_serializes_stable_fields() {
        let response = StatusResponse::new(
            &sample_deck(),
            ProgressCounts {
                due_count: 12,
                new_remaining: 8,
                new_reviewed_today: 5,
                reviewed_today: 3,
            },
            120,
        );
        let value = serde_json::to_value(response).unwrap();
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../fixtures/protocol_status_sample.json"
        ))
        .unwrap();
        assert_eq!(value, fixture);
    }

    #[test]
    fn stats_response_serializes_stable_fields() {
        let response = StatsResponse::new_for_range(
            &sample_deck(),
            7,
            NaiveDate::from_ymd_opt(2026, 6, 4).unwrap(),
            vec![
                DailyReviewBucket {
                    date: "2026-06-05".to_string(),
                    stats: DailyReviewStats {
                        reviews: 1,
                        again: 1,
                        hard: 0,
                        good: 0,
                        easy: 0,
                    },
                },
                DailyReviewBucket {
                    date: "2026-06-06".to_string(),
                    stats: DailyReviewStats {
                        reviews: 3,
                        again: 0,
                        hard: 1,
                        good: 2,
                        easy: 0,
                    },
                },
                DailyReviewBucket {
                    date: "2026-06-08".to_string(),
                    stats: DailyReviewStats {
                        reviews: 5,
                        again: 0,
                        hard: 0,
                        good: 3,
                        easy: 2,
                    },
                },
                DailyReviewBucket {
                    date: "2026-06-09".to_string(),
                    stats: DailyReviewStats {
                        reviews: 4,
                        again: 1,
                        hard: 1,
                        good: 1,
                        easy: 1,
                    },
                },
                DailyReviewBucket {
                    date: "2026-06-10".to_string(),
                    stats: DailyReviewStats {
                        reviews: 7,
                        again: 1,
                        hard: 1,
                        good: 3,
                        easy: 2,
                    },
                },
            ],
        );
        let value = serde_json::to_value(response).unwrap();
        let fixture = serde_json::from_str::<serde_json::Value>(include_str!(
            "../fixtures/protocol_stats_sample.json"
        ))
        .unwrap();
        assert_eq!(value, fixture);
    }

    #[test]
    fn catalog_list_response_serializes_catalog_identity_fields() {
        let value = serde_json::to_value(CatalogListResponse {
            schema: CATALOG_LIST_SCHEMA,
            decks: vec![CatalogDeckEntry {
                id: "kajweb:cet4".to_string(),
                slug: "cet4".to_string(),
                source_id: "kajweb".to_string(),
                name: "CET-4".to_string(),
                description: Some("大学英语四级".to_string()),
                language: "en".to_string(),
                word_count: 4544,
                tags: vec!["cet4".to_string(), "exam".to_string(), "zh".to_string()],
                source: Some(fishword_core::card::Source {
                    name: "kajweb/dict".to_string(),
                    license: None,
                }),
                url: "https://example.invalid/catalog/kajweb-cet4.jsonl".to_string(),
                size_bytes: 123,
            }],
        })
        .unwrap();

        assert_eq!(value["decks"][0]["id"], "kajweb:cet4");
        assert_eq!(value["decks"][0]["slug"], "cet4");
        assert_eq!(value["decks"][0]["source_id"], "kajweb");
    }

    #[test]
    fn catalog_fetch_response_distinguishes_catalog_id_from_local_deck_id() {
        let value = serde_json::to_value(CatalogFetchResponse {
            schema: CATALOG_FETCH_SCHEMA,
            catalog_id: "kajweb:cet4".to_string(),
            slug: "cet4".to_string(),
            source_id: "kajweb".to_string(),
            name: "CET-4".to_string(),
            import: ImportResponse {
                schema: IMPORT_SCHEMA,
                deck_id: 1,
                deck: "CET-4".to_string(),
                input: 4544,
                inserted: 4544,
                updated: 0,
                merged: 0,
                skipped: 0,
            },
        })
        .unwrap();

        assert_eq!(value["catalog_id"], "kajweb:cet4");
        assert!(value.get("deck_id").is_none());
        assert_eq!(value["import"]["deck_id"], 1);
    }

    #[test]
    fn protocol_schema_is_valid_json() {
        serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../schemas/protocol.v1.schema.json"
        ))
        .unwrap();
    }
}
