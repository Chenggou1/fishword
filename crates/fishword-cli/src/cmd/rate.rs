use anyhow::{Context, Result};
use fishword_core::{card::Rating, scheduler::Scheduler, selector};

use crate::{
    args::RateArgs,
    protocol::RateResponse,
    util::{cmd_error, open_storage, print_human, print_json},
};

use super::scope::resolve_deck;

pub fn cmd_rate(args: &RateArgs) -> Result<()> {
    let rating = args.rating.parse::<Rating>().map_err(|_| {
        cmd_error(
            args.json,
            "invalid_rating",
            &format!(
                "invalid rating '{}', expected again/hard/good/easy",
                args.rating
            ),
        )
    })?;
    let storage = open_storage()?;
    let Some(scope_deck) = resolve_deck(&storage, args.deck, args.json)? else {
        return Err(cmd_error(
            args.json,
            "no_active_deck",
            "No active deck. Run `fishword deck use <deck>` first.",
        ));
    };
    let Some(card_id) = storage
        .get_current_card_id()
        .context("failed to read current card")?
    else {
        return Err(cmd_error(
            args.json,
            "no_current_card",
            "No current card. Run `fishword current` first.",
        ));
    };
    let card = storage
        .get_card_by_id(card_id)
        .context("failed to read current card")?
        .context("Current card disappeared")?;
    if card.deck_id != scope_deck.id {
        return Err(cmd_error(
            args.json,
            "no_current_card",
            "No current card in this deck. Run `fishword current` first.",
        ));
    }
    let review = Scheduler::review(&storage, card_id, rating).context("failed to rate card")?;
    let next = selector::select_next(&storage, scope_deck.id)
        .context("failed to select next card after rating")?;
    let next_deck = if let Some(ref selected) = next {
        storage
            .get_deck_by_id(selected.card.deck_id)
            .context("failed to read next card deck")?
    } else {
        None
    };
    if args.json {
        let progress = storage
            .progress_counts_by_deck(scope_deck.id)
            .context("failed to read progress")?;
        let next_ref = next.as_ref().zip(next_deck.as_ref());
        print_json(RateResponse::new(
            &card,
            &scope_deck,
            &review,
            progress,
            next_ref,
        ))?;
    } else {
        print_human(format!(
            "Rated {} as {}. due={} scheduled_days={}",
            card.word, review.rating, review.due, review.scheduled_days
        ));
        if let Some(ref selected) = next {
            print_human(format!("Next: {}", selected.card.word));
        }
    }
    Ok(())
}
