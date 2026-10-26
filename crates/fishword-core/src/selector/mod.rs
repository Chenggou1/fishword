use chrono::Utc;
use fsrs::{current_retrievability, MemoryState, FSRS6_DEFAULT_DECAY};

use crate::{
    card::{Card, CardWithState},
    error::Result,
    scheduler::parse_utc,
    storage::Storage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionReason {
    Due,
    New,
}

#[derive(Debug, Clone)]
pub struct SelectedCard {
    pub card: Card,
    pub reason: SelectionReason,
}

/// Returns the current card for the given deck, or selects the next one if none is set.
pub fn select_current(storage: &Storage, deck_id: i64) -> Result<Option<SelectedCard>> {
    if let Some(card) = storage.get_current_card_in_deck(deck_id)? {
        let reason = storage
            .get_card_state(card.id)?
            .map(|state| reason_for_reps(state.reps))
            .unwrap_or(SelectionReason::New);
        return Ok(Some(SelectedCard { card, reason }));
    }
    select_next(storage, deck_id)
}

/// Selects the next card for the given deck and updates the current-card pointer.
pub fn select_next(storage: &Storage, deck_id: i64) -> Result<Option<SelectedCard>> {
    let progress = storage.progress_counts_by_deck(deck_id)?;
    let daily_new_limit = progress.new_remaining as usize;
    let current_card_id = storage
        .get_current_card_in_deck(deck_id)?
        .map(|card| card.id);
    let candidates = storage.list_cards_with_state_by_deck(deck_id)?;
    let selected = pick_card(&candidates, daily_new_limit, current_card_id);
    storage.set_current_card_id(selected.as_ref().map(|s| s.card.id))?;
    Ok(selected)
}

// Applies the in-memory Selection policy after storage has loaded deck-scoped
// candidates: avoid repeating the current card when possible, prefer due
// reviewed cards by lowest retrievability, then introduce new cards only within
// the remaining daily new-card limit.
fn pick_card(
    candidates: &[CardWithState],
    daily_new_limit: usize,
    skip_card_id: Option<i64>,
) -> Option<SelectedCard> {
    let all_candidates = candidates.iter().collect::<Vec<_>>();
    let filtered_candidates = candidates
        .iter()
        .filter(|candidate| Some(candidate.card.id) != skip_card_id)
        .collect::<Vec<_>>();
    let candidates = if filtered_candidates.is_empty() {
        // If there is only one selectable card, keep it selected rather than
        // returning nothing for a valid single-card deck.
        all_candidates
    } else {
        filtered_candidates
    };

    let now = Utc::now();
    let mut due = candidates
        .iter()
        .filter(|candidate| candidate.state.reps > 0)
        .filter_map(|candidate| {
            let due_at = parse_utc(&candidate.state.due).ok()?;
            (due_at <= now).then_some(candidate)
        })
        .collect::<Vec<_>>();
    due.sort_by(|left, right| {
        retrievability(left)
            .partial_cmp(&retrievability(right))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.state.due.cmp(&right.state.due))
            .then_with(|| left.card.id.cmp(&right.card.id))
    });
    if let Some(candidate) = due.first() {
        return Some(make_selected(candidate, SelectionReason::Due));
    }

    let new_cards = candidates
        .iter()
        .filter(|candidate| candidate.state.reps == 0)
        .take(daily_new_limit)
        .collect::<Vec<_>>();
    if let Some(candidate) = new_cards.first() {
        return Some(make_selected(candidate, SelectionReason::New));
    }

    None
}

fn make_selected(candidate: &CardWithState, reason: SelectionReason) -> SelectedCard {
    SelectedCard {
        card: candidate.card.clone(),
        reason,
    }
}

fn reason_for_reps(reps: i32) -> SelectionReason {
    if reps == 0 {
        SelectionReason::New
    } else {
        SelectionReason::Due
    }
}

// Estimate the card's current recall probability with the FSRS forgetting
// curve. Lower retrievability means the memory is weaker, so due cards with
// lower values are reviewed first.
fn retrievability(candidate: &CardWithState) -> f64 {
    if candidate.state.stability <= 0.0 || candidate.state.difficulty <= 0.0 {
        return 0.0;
    }
    let Some(last_reviewed_at) = candidate.last_reviewed_at.as_deref() else {
        return 0.0;
    };
    let Ok(last_reviewed_at) = parse_utc(last_reviewed_at) else {
        return 0.0;
    };
    let elapsed_days = (Utc::now() - last_reviewed_at).num_days().max(0) as f32;
    current_retrievability(
        MemoryState {
            stability: candidate.state.stability as f32,
            difficulty: candidate.state.difficulty as f32,
        },
        elapsed_days,
        FSRS6_DEFAULT_DECAY,
    ) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{card::Rating, scheduler::Scheduler, storage::Storage};

    fn open_temp() -> Storage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("selector.db");
        Storage::open(&path).unwrap()
    }

    fn insert_card(storage: &Storage, word: &str) -> (i64, i64) {
        let deck = storage.ensure_deck("test", None).unwrap();
        let card = storage.insert_card(deck.id, word, &[], &[]).unwrap();
        (deck.id, card.id)
    }

    fn insert_card_in_deck(storage: &Storage, deck_name: &str, word: &str) -> (i64, i64) {
        let deck = storage.ensure_deck(deck_name, None).unwrap();
        let card = storage.insert_card(deck.id, word, &[], &[]).unwrap();
        (deck.id, card.id)
    }

    #[test]
    fn next_selects_new_card_without_review_log() {
        let storage = open_temp();
        let (deck_id, card_id) = insert_card(&storage, "cancel");
        let selected = select_next(&storage, deck_id).unwrap().unwrap();
        assert_eq!(selected.card.id, card_id);
        assert_eq!(selected.reason, SelectionReason::New);
        assert_eq!(storage.review_log_count(card_id).unwrap(), 0);
        assert_eq!(storage.get_current_card_id().unwrap(), Some(card_id));
    }

    #[test]
    fn current_keeps_new_reason_for_unreviewed_card() {
        let storage = open_temp();
        let (deck_id, card_id) = insert_card(&storage, "new");
        storage.set_current_card_id(Some(card_id)).unwrap();

        let selected = select_current(&storage, deck_id).unwrap().unwrap();

        assert_eq!(selected.card.id, card_id);
        assert_eq!(selected.reason, SelectionReason::New);
    }

    #[test]
    fn current_uses_due_reason_for_reviewed_card() {
        let storage = open_temp();
        let (deck_id, card_id) = insert_card(&storage, "reviewed");
        Scheduler::review(&storage, card_id, Rating::Again).unwrap();
        storage.set_current_card_id(Some(card_id)).unwrap();

        let selected = select_current(&storage, deck_id).unwrap().unwrap();

        assert_eq!(selected.card.id, card_id);
        assert_eq!(selected.reason, SelectionReason::Due);
    }

    #[test]
    fn next_skips_current_card_when_possible() {
        let storage = open_temp();
        let (deck_id, first_id) = insert_card(&storage, "first");
        let deck = storage.ensure_deck("test", None).unwrap();
        let second_id = storage.insert_card(deck.id, "second", &[], &[]).unwrap().id;

        storage.set_current_card_id(Some(first_id)).unwrap();
        let selected = select_next(&storage, deck_id).unwrap().unwrap();

        assert_eq!(selected.card.id, second_id);
        assert_eq!(storage.review_log_count(first_id).unwrap(), 0);
        assert_eq!(storage.review_log_count(second_id).unwrap(), 0);
    }

    #[test]
    fn due_review_card_beats_new_card() {
        let storage = open_temp();
        let (deck_id, due_card) = insert_card(&storage, "due");
        let deck = storage.ensure_deck("test", None).unwrap();
        storage.insert_card(deck.id, "new", &[], &[]).unwrap();

        Scheduler::review(&storage, due_card, Rating::Again).unwrap();
        storage.set_current_card_id(None).unwrap();
        let selected = select_next(&storage, deck_id).unwrap().unwrap();

        assert_eq!(selected.card.id, due_card);
        assert_eq!(selected.reason, SelectionReason::Due);
        assert_eq!(storage.review_log_count(due_card).unwrap(), 1);
    }

    #[test]
    fn next_stops_when_no_due_or_new_quota_remains() {
        let storage = open_temp();
        let (deck_id, card_id) = insert_card(&storage, "known");
        Scheduler::review(&storage, card_id, Rating::Easy).unwrap();

        let selected = select_next(&storage, deck_id).unwrap();
        assert!(selected.is_none());
        assert_eq!(storage.get_current_card_id().unwrap(), None);
    }

    #[test]
    fn next_stops_after_daily_new_quota_is_used() {
        let storage = open_temp();
        let deck = storage.ensure_deck("test", None).unwrap();
        for index in 0..20 {
            let card_id = storage
                .insert_card(deck.id, &format!("word-{index}"), &[], &[])
                .unwrap()
                .id;
            Scheduler::review(&storage, card_id, Rating::Easy).unwrap();
        }
        storage.insert_card(deck.id, "extra-new", &[], &[]).unwrap();

        let selected = select_next(&storage, deck.id).unwrap();
        assert!(selected.is_none());
        assert_eq!(storage.get_current_card_id().unwrap(), None);
    }

    #[test]
    fn next_only_selects_candidates_from_given_deck() {
        let storage = open_temp();
        let (_, first_id) = insert_card_in_deck(&storage, "first", "same");
        let (second_deck_id, second_id) = insert_card_in_deck(&storage, "second", "same");

        let selected = select_next(&storage, second_deck_id).unwrap().unwrap();

        assert_eq!(selected.card.id, second_id);
        assert_ne!(selected.card.id, first_id);
    }

    #[test]
    fn current_ignores_card_from_other_deck() {
        let storage = open_temp();
        let (_, first_id) = insert_card_in_deck(&storage, "first", "first");
        let (second_deck_id, second_id) = insert_card_in_deck(&storage, "second", "second");

        storage.set_current_card_id(Some(first_id)).unwrap();
        let selected = select_current(&storage, second_deck_id).unwrap().unwrap();

        assert_eq!(selected.card.id, second_id);
    }
}
