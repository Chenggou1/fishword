use std::str::FromStr;

use anyhow::{Context, Result};
use fishword_core::{
    deck::Deck,
    error::Error as CoreError,
    importer::{import_jsonl_file, DuplicateStrategy},
};

use crate::protocol::{ImportResponse, IMPORT_SCHEMA};

use crate::{
    args::{ImportArgs, ImportCmd},
    util::{cmd_error, open_storage, print_human, print_json},
};

enum ImportTarget {
    ExistingDeck(i64),
    CreateDeck(String),
}

pub fn cmd_import(command: ImportCmd) -> Result<()> {
    let ImportCmd::Jsonl(args) = command;
    let target = ImportTarget::from_args(&args)?;
    let cards = import_jsonl_file(&args.path)
        .with_context(|| format!("failed to parse {}", args.path.display()))?;
    if cards.is_empty() {
        return Err(cmd_error(
            args.json,
            "empty_import_file",
            "No importable cards found in the JSONL file",
        ));
    }
    persist_import(target, cards, &args.duplicates, args.json)
}

impl ImportTarget {
    fn from_args(args: &ImportArgs) -> Result<Self> {
        match (args.deck_id, args.create_deck.as_deref()) {
            (Some(deck_id), None) => Ok(Self::ExistingDeck(deck_id)),
            (None, Some(name)) => Ok(Self::CreateDeck(name.to_string())),
            _ => anyhow::bail!("pass exactly one of --deck-id or --create-deck"),
        }
    }
}

fn persist_import(
    target: ImportTarget,
    cards: Vec<fishword_core::importer::ImportCard>,
    duplicates: &str,
    json: bool,
) -> Result<()> {
    let duplicate_strategy = DuplicateStrategy::from_str(duplicates).map_err(|_| {
        cmd_error(
            json,
            "invalid_duplicate_strategy",
            &format!("invalid --duplicates value '{duplicates}'"),
        )
    })?;
    let storage = open_storage()?;
    let (db_deck, summary) = match target {
        ImportTarget::ExistingDeck(deck_id) => {
            let db_deck = storage
                .get_deck_by_id(deck_id)
                .with_context(|| format!("failed to read deck {}", deck_id))?
                .ok_or_else(|| {
                    cmd_error(
                        json,
                        "deck_not_found",
                        &format!(
                            "deck not found: {}. Run `fishword deck create <name>` first.",
                            deck_id
                        ),
                    )
                })?;
            let summary = storage
                .import_cards(deck_id, &cards, duplicate_strategy)
                .context("failed to write imported cards")?;
            (db_deck, summary)
        }
        ImportTarget::CreateDeck(name) => {
            import_into_new_deck(&storage, &name, &cards, duplicate_strategy, json)?
        }
    };
    if storage
        .get_active_deck_id()
        .context("failed to read active deck")?
        .is_none()
    {
        storage
            .set_active_deck_id(Some(db_deck.id))
            .context("failed to set active deck")?;
    }
    if json {
        return print_json(&ImportResponse {
            schema: IMPORT_SCHEMA,
            deck_id: db_deck.id,
            deck: db_deck.name,
            input: summary.input_count,
            inserted: summary.inserted,
            updated: summary.updated,
            merged: summary.merged,
            skipped: summary.skipped,
        });
    }
    print_human(format!(
        "Imported deck={} input={} inserted={} updated={} merged={} skipped={}",
        db_deck.name,
        summary.input_count,
        summary.inserted,
        summary.updated,
        summary.merged,
        summary.skipped
    ));
    Ok(())
}

fn import_into_new_deck(
    storage: &fishword_core::storage::Storage,
    name: &str,
    cards: &[fishword_core::importer::ImportCard],
    duplicate_strategy: DuplicateStrategy,
    json: bool,
) -> Result<(Deck, fishword_core::importer::ImportSummary)> {
    match storage.import_cards_into_new_deck(name, None, cards, duplicate_strategy) {
        Ok(result) => Ok(result),
        Err(CoreError::AlreadyExists(_)) => Err(cmd_error(
            json,
            "deck_already_exists",
            &format!(
                "Deck already exists: {name}. Use `fishword deck list` to find its id, then import with `--deck-id <id>`."
            ),
        )),
        Err(e) => Err(anyhow::anyhow!(e)).context("failed to write imported cards"),
    }
}
