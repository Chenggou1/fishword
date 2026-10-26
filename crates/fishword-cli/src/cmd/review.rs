use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use fishword_core::selector;

use crate::protocol::{StatsResponse, StatusResponse};

use crate::{
    args::{CardOutputArgs, StatsArgs, StatusArgs},
    util::{cmd_error, open_storage, print_human, print_json, print_selected_card},
};

use super::scope::resolve_deck;

pub fn cmd_current(args: &CardOutputArgs) -> Result<()> {
    let storage = open_storage()?;
    let Some(deck) = resolve_deck(&storage, args.deck, args.json)? else {
        if args.json {
            return Err(cmd_error(
                true,
                "no_active_deck",
                "No active deck. Run `fishword deck use <deck>` first.",
            ));
        }
        print_human("No active deck. Run `fishword deck use <deck>` first.");
        return Ok(());
    };
    match selector::select_current(&storage, deck.id).context("failed to select current card")? {
        Some(selected) => print_selected_card(&storage, &selected, args, true)?,
        None => {
            if args.json {
                return Err(cmd_error(
                    true,
                    "no_cards",
                    "No cards found. Import a deck first.",
                ));
            }
            print_human("No cards found. Import a deck first.");
        }
    }
    Ok(())
}

pub fn cmd_status(args: &StatusArgs) -> Result<()> {
    let storage = open_storage()?;
    let Some(deck) = resolve_deck(&storage, args.deck, args.json)? else {
        if args.json {
            return Err(cmd_error(
                true,
                "no_active_deck",
                "No active deck. Run `fishword deck use <deck>` first.",
            ));
        }
        print_human("No active deck. Run `fishword deck use <deck>` first.");
        return Ok(());
    };
    let progress = storage
        .progress_counts_by_deck(deck.id)
        .context("failed to read progress")?;
    let card_count = storage
        .card_count_by_deck(deck.id)
        .context("failed to count cards")?;
    let response = StatusResponse::new(&deck, progress, card_count);
    if args.json {
        print_json(&response)?;
    } else {
        match args.format.as_str() {
            "plain" => print_human(response.display.plain),
            "compact" => print_human(response.display.compact),
            "statusline" => print_human(response.display.statusline),
            other => anyhow::bail!(
                "invalid --format '{}', expected plain/compact/statusline",
                other
            ),
        }
    }
    Ok(())
}

pub fn cmd_stats(args: &StatsArgs) -> Result<()> {
    if args.range != "7d" {
        return Err(cmd_error(
            args.json,
            "invalid_range",
            &format!("Invalid range: {}", args.range),
        ));
    }
    let storage = open_storage()?;
    let Some(deck) = resolve_deck(&storage, args.deck, args.json)? else {
        if args.json {
            return Err(cmd_error(
                true,
                "no_active_deck",
                "No active deck. Run `fishword deck use <deck>` first.",
            ));
        }
        print_human("No active deck. Run `fishword deck use <deck>` first.");
        return Ok(());
    };
    let today = Utc::now().date_naive();
    let start = today - Duration::days(6);
    let buckets = storage
        .review_stats_by_deck_and_day_range(deck.id, &start.to_string(), &today.to_string())
        .context("failed to read review stats")?;
    let response = StatsResponse::new(&deck, 7, buckets);
    if args.json {
        print_json(&response)?;
    } else {
        print_human(format!(
            "Today: {} reviews",
            response.summary.reviewed_today
        ));
        match response.summary.good_or_easy_rate {
            Some(rate) => print_human(format!(
                "7 days: {} reviews, {}% good/easy",
                response.summary.reviews,
                (rate * 100.0).round() as i64
            )),
            None => print_human(format!(
                "7 days: {} reviews, no ratings yet",
                response.summary.reviews
            )),
        }
        print_human(format!(
            "{:<12} {:>7} {:>7} {:>7} {:>7} {:>7}",
            "DATE", "REVIEWS", "AGAIN", "HARD", "GOOD", "EASY"
        ));
        for day in response.series {
            print_human(format!(
                "{:<12} {:>7} {:>7} {:>7} {:>7} {:>7}",
                day.date, day.reviews, day.again, day.hard, day.good, day.easy
            ));
        }
    }
    Ok(())
}
