use anyhow::{Context, Result};
use fishword_core::error::Error as CoreError;

use fishword_core::storage::DEFAULT_DAILY_NEW_LIMIT;

use crate::protocol::{
    DailyLimitResponse, DeckCreateResponse, DeckDeleteResponse, DeckListResponse,
    DeckRenameResponse, DeckUseResponse, DAILY_LIMIT_SCHEMA,
};

use crate::{
    args::{DailyLimitArgs, DeckCmd},
    util::{cmd_error, open_storage, print_human, print_json},
};

pub fn cmd_deck(sub: DeckCmd) -> Result<()> {
    match sub {
        DeckCmd::List { json } => list(json),
        DeckCmd::Create {
            name,
            description,
            json,
        } => create(&name, description.as_deref(), json),
        DeckCmd::Use { deck, json } => use_deck(deck, json),
        DeckCmd::Delete { id, json } => delete(id, json),
        DeckCmd::Rename { id, new_name, json } => rename(id, &new_name, json),
        DeckCmd::Current => current(),
        DeckCmd::DailyLimit(args) => daily_limit(&args),
    }
}

fn list(json: bool) -> Result<()> {
    let storage = open_storage()?;
    let decks = storage.list_decks().context("failed to list decks")?;
    let active_deck_id = storage
        .get_active_deck_id()
        .context("failed to read active deck")?;
    if json {
        return print_json(DeckListResponse::new(decks, active_deck_id));
    }
    if decks.is_empty() {
        print_human("No decks found.");
        return Ok(());
    }
    print_human(format!(
        "{:<6}  {:<6}  {:<20}  {:<14}  DESCRIPTION",
        "ACTIVE", "ID", "NAME", "CATALOG"
    ));
    print_human("-".repeat(72));
    for d in decks {
        print_human(format!(
            "{:<6}  {:<6}  {:<20}  {:<14}  {}",
            if Some(d.id) == active_deck_id {
                "*"
            } else {
                ""
            },
            d.id,
            d.name,
            d.catalog_id.as_deref().unwrap_or("-"),
            d.description.as_deref().unwrap_or("")
        ));
    }
    Ok(())
}

fn create(name: &str, description: Option<&str>, json: bool) -> Result<()> {
    let storage = open_storage()?;
    match storage.insert_deck(name, description) {
        Ok(deck) => {
            if json {
                print_json(DeckCreateResponse::new(&deck))?;
            } else {
                print_human(format!("Created deck: {} (id={})", deck.name, deck.id));
            }
        }
        Err(CoreError::AlreadyExists(_)) => {
            return Err(cmd_error(
                json,
                "deck_already_exists",
                &format!("Deck already exists: {name}"),
            ));
        }
        Err(e) => return Err(anyhow::anyhow!(e)),
    }
    Ok(())
}

fn use_deck(deck_id: i64, json: bool) -> Result<()> {
    let storage = open_storage()?;
    let deck = match storage
        .get_deck_by_id(deck_id)
        .with_context(|| format!("failed to read deck {deck_id}"))?
    {
        Some(d) => d,
        None => {
            return Err(cmd_error(
                json,
                "deck_not_found",
                &format!("Deck not found: {deck_id}"),
            ))
        }
    };
    storage
        .set_active_deck_id(Some(deck.id))
        .with_context(|| format!("failed to set active deck {deck_id}"))?;
    storage
        .set_current_card_id(None)
        .with_context(|| "failed to clear current card on deck switch")?;
    if json {
        print_json(DeckUseResponse::new(&deck))?;
    } else {
        print_human(format!("Active deck: {} ({})", deck.name, deck.id));
    }
    Ok(())
}

fn delete(id: i64, json: bool) -> Result<()> {
    let storage = open_storage()?;
    match storage.delete_deck(id) {
        Ok(deck) => {
            if json {
                print_json(DeckDeleteResponse::new(&deck))?;
            } else {
                print_human(format!("Deleted deck: {} (id={})", deck.name, deck.id));
            }
        }
        Err(CoreError::NotFound(_)) => {
            return Err(cmd_error(
                json,
                "deck_not_found",
                &format!("Deck not found: {id}"),
            ));
        }
        Err(e) => return Err(anyhow::anyhow!(e)),
    }
    Ok(())
}

fn rename(id: i64, new_name: &str, json: bool) -> Result<()> {
    let storage = open_storage()?;
    match storage.update_deck_name(id, new_name) {
        Ok(deck) => {
            if json {
                print_json(DeckRenameResponse::new(&deck))?;
            } else {
                print_human(format!("Renamed deck {} to: {}", id, deck.name));
            }
        }
        Err(CoreError::NotFound(_)) => {
            return Err(cmd_error(
                json,
                "deck_not_found",
                &format!("Deck not found: {id}"),
            ));
        }
        Err(CoreError::AlreadyExists(_)) => {
            return Err(cmd_error(
                json,
                "deck_already_exists",
                &format!("Deck already exists: {new_name}"),
            ));
        }
        Err(e) => return Err(anyhow::anyhow!(e)),
    }
    Ok(())
}

fn current() -> Result<()> {
    let storage = open_storage()?;
    match storage
        .get_active_deck()
        .context("failed to read active deck")?
    {
        Some(deck) => print_human(format!(
            "Active deck: {} ({}) {}",
            deck.name,
            deck.id,
            deck.description.as_deref().unwrap_or("")
        )),
        None => print_human("No decks found."),
    }
    Ok(())
}

fn daily_limit(args: &DailyLimitArgs) -> Result<()> {
    let storage = open_storage()?;
    if let Some(value) = args.value {
        if value <= 0 {
            return Err(cmd_error(
                args.json,
                "invalid_daily_limit",
                &format!("Daily new limit must be positive, got {value}"),
            ));
        }
        storage
            .set_daily_new_limit(value)
            .context("failed to save daily new limit")?;
    }

    let limit = storage
        .daily_new_limit()
        .context("failed to read daily new limit")?;
    if args.json {
        return print_json(DailyLimitResponse {
            schema: DAILY_LIMIT_SCHEMA,
            daily_new_limit: limit,
            default_limit: DEFAULT_DAILY_NEW_LIMIT,
        });
    }
    match args.value {
        Some(value) => print_human(format!("Daily new limit set to {value}.")),
        None => print_human(format!(
            "Daily new limit: {limit} (default {DEFAULT_DAILY_NEW_LIMIT})"
        )),
    }
    Ok(())
}
