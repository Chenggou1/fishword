use std::{path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fishword_core::{
    card::Rating,
    deck::Deck,
    default_decks::{find_default_dict_dir, seed_default_decks},
    importer::{
        import_anki_tsv_file, import_csv_file, import_jsonl_file, import_qwerty_file,
        DuplicateStrategy, ImportDeck,
    },
    protocol::{render_card, CardResponse, ErrorResponse, RateResponse, TextFormat},
    scheduler::Scheduler,
    selector::{SelectedCard, Selector},
    storage::Storage,
};

#[derive(Parser)]
#[command(name = "fishword", about = "Vocabulary flashcard CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Initialize the database at the platform-default path.
    Init(InitArgs),
    /// Manage decks.
    Deck {
        #[command(subcommand)]
        sub: DeckCmd,
    },
    /// Manage cards.
    Card {
        #[command(subcommand)]
        sub: CardCmd,
    },
    /// Import vocabulary decks.
    Import {
        #[command(subcommand)]
        sub: ImportCmd,
    },
    /// Show the current selected card.
    Current(CardOutputArgs),
    /// Select the next card without writing review history.
    Next(CardOutputArgs),
    /// Rate the current card: again, hard, good, easy.
    Rate(RateArgs),
}

#[derive(Parser)]
struct InitArgs {
    /// Create the database without importing bundled default decks.
    #[arg(long)]
    skip_default_decks: bool,
}

#[derive(Subcommand)]
enum DeckCmd {
    /// List all decks.
    List,
    /// Set the active deck used by current/next/rate.
    Use {
        /// Deck name (e.g. cet4)
        deck: String,
    },
    /// Show the active deck.
    Current,
}

#[derive(Subcommand)]
enum CardCmd {
    /// List cards in a deck.
    List {
        /// Deck name (e.g. cet4)
        #[arg(long)]
        deck: String,
    },
}

#[derive(Subcommand)]
enum ImportCmd {
    /// Import Qwerty Learner JSON.
    Qwerty(ImportArgs),
    /// Import minimal CSV.
    Csv(ImportArgs),
    /// Import fishword.deck.v1 JSONL.
    Jsonl(ImportArgs),
    /// Import Anki exported TSV.
    AnkiTsv(ImportArgs),
}

#[derive(Parser)]
struct ImportArgs {
    /// Input file path.
    path: PathBuf,
    /// Deck id/name used by the local database.
    #[arg(long)]
    deck: String,
    /// Human-readable deck name/description.
    #[arg(long)]
    name: Option<String>,
    /// Duplicate strategy: merge, skip, overwrite, keep.
    #[arg(long, default_value = "merge")]
    duplicates: String,
}

#[derive(Parser)]
struct CardOutputArgs {
    /// Emit stable JSON protocol output.
    #[arg(long)]
    json: bool,
    /// Deck name used as this command's learning scope.
    #[arg(long)]
    deck: Option<String>,
    /// Human-readable output format: plain, compact, status.
    #[arg(long, default_value = "plain")]
    format: String,
}

#[derive(Parser)]
struct RateArgs {
    /// Review rating: again, hard, good, easy.
    rating: String,
    /// Deck name used as this command's learning scope.
    #[arg(long)]
    deck: Option<String>,
    /// Emit stable JSON protocol output.
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Init(args) => cmd_init(&args),
        Cmd::Deck { sub } => cmd_deck(sub),
        Cmd::Card {
            sub: CardCmd::List { deck },
        } => cmd_card_list(&deck),
        Cmd::Import { sub } => cmd_import(sub),
        Cmd::Current(args) => cmd_current(&args),
        Cmd::Next(args) => cmd_next(&args),
        Cmd::Rate(args) => cmd_rate(&args),
    }
}

fn open_storage() -> Result<Storage> {
    let path = Storage::default_path().context("cannot determine data directory")?;
    Storage::open(&path).with_context(|| format!("cannot open database at {}", path.display()))
}

fn cmd_init(args: &InitArgs) -> Result<()> {
    let path = Storage::default_path().context("cannot determine data directory")?;
    let storage = Storage::open(&path)
        .with_context(|| format!("cannot initialize database at {}", path.display()))?;
    println!("Initialized: {}", path.display());
    if !args.skip_default_decks {
        match find_default_dict_dir() {
            Some(dict_dir) => {
                let summaries = seed_default_decks(&storage, &dict_dir).with_context(|| {
                    format!("failed to import default decks from {}", dict_dir.display())
                })?;
                for summary in summaries {
                    println!(
                        "Default deck={} input={} inserted={} skipped={}",
                        summary.deck.id,
                        summary.import.input_count,
                        summary.import.inserted,
                        summary.import.skipped
                    );
                }
            }
            None => {
                eprintln!(
                    "Warning: bundled default decks were not found; run with FISHWORD_DEFAULT_DICT_DIR or reinstall the npm package with assets."
                );
            }
        }
    }
    Ok(())
}

fn cmd_deck(sub: DeckCmd) -> Result<()> {
    match sub {
        DeckCmd::List => cmd_deck_list(),
        DeckCmd::Use { deck } => cmd_deck_use(&deck),
        DeckCmd::Current => cmd_deck_current(),
    }
}

fn cmd_deck_list() -> Result<()> {
    let storage = open_storage()?;
    let decks = storage.list_decks().context("failed to list decks")?;
    let active_deck_id = storage
        .get_active_deck_id()
        .context("failed to read active deck")?;
    if decks.is_empty() {
        println!("No decks found.");
        return Ok(());
    }
    println!("{:<6}  {:<6}  {:<20}  DESCRIPTION", "ACTIVE", "ID", "NAME");
    println!("{}", "-".repeat(60));
    for d in decks {
        println!(
            "{:<6}  {:<6}  {:<20}  {}",
            if Some(d.id) == active_deck_id {
                "*"
            } else {
                ""
            },
            d.id,
            d.name,
            d.description.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

fn cmd_deck_use(deck_name: &str) -> Result<()> {
    let storage = open_storage()?;
    let deck = storage
        .get_deck_by_name(deck_name)
        .with_context(|| format!("failed to read deck '{deck_name}'"))?
        .with_context(|| format!("deck not found: {deck_name}"))?;
    storage
        .set_active_deck_id(Some(deck.id))
        .with_context(|| format!("failed to set active deck '{deck_name}'"))?;
    println!("Active deck: {} ({})", deck.name, deck.id);
    Ok(())
}

fn cmd_deck_current() -> Result<()> {
    let storage = open_storage()?;
    match resolve_deck_scope(&storage, None, false)? {
        Some(deck) => println!(
            "Active deck: {} ({}) {}",
            deck.name,
            deck.id,
            deck.description.as_deref().unwrap_or("")
        ),
        None => println!("No decks found."),
    }
    Ok(())
}

fn cmd_card_list(deck: &str) -> Result<()> {
    let storage = open_storage()?;
    let cards = storage
        .list_cards_by_deck(deck)
        .with_context(|| format!("failed to list cards for deck '{deck}'"))?;
    if cards.is_empty() {
        println!("No cards in deck '{deck}'.");
        return Ok(());
    }
    println!("{:<6}  {:<20}  MEANINGS", "ID", "WORD");
    println!("{}", "-".repeat(60));
    for c in cards {
        let meanings_summary = c
            .meanings
            .iter()
            .map(|m| format!("[{}] {}", m.part_of_speech, m.definition))
            .collect::<Vec<_>>()
            .join("; ");
        println!("{:<6}  {:<20}  {}", c.id, c.word, meanings_summary);
    }
    Ok(())
}

fn cmd_current(args: &CardOutputArgs) -> Result<()> {
    let storage = open_storage()?;
    let Some(deck) = resolve_deck_scope(&storage, args.deck.as_deref(), args.json)? else {
        if args.json {
            exit_json_error("no_cards", "No cards found. Import a deck first.");
        }
        println!("No cards found. Import a deck first.");
        return Ok(());
    };
    match Selector::select_current_in_deck(&storage, deck.id)
        .context("failed to select current card")?
    {
        Some(selected) => print_selected_card(&storage, &selected, args, true)?,
        None if args.json => exit_json_error("no_cards", "No cards found. Import a deck first."),
        None => println!("No cards found. Import a deck first."),
    }
    Ok(())
}

fn cmd_next(args: &CardOutputArgs) -> Result<()> {
    let storage = open_storage()?;
    let Some(deck) = resolve_deck_scope(&storage, args.deck.as_deref(), args.json)? else {
        if args.json {
            exit_json_error("no_cards", "No cards found. Import a deck first.");
        }
        println!("No cards found. Import a deck first.");
        return Ok(());
    };
    match Selector::select_next_in_deck(&storage, deck.id).context("failed to select next card")? {
        Some(selected) => print_selected_card(&storage, &selected, args, false)?,
        None if args.json => exit_json_error("no_cards", "No cards found. Import a deck first."),
        None => println!("No cards found. Import a deck first."),
    }
    Ok(())
}

fn cmd_rate(args: &RateArgs) -> Result<()> {
    let rating = args
        .rating
        .parse::<Rating>()
        .map_err(anyhow::Error::msg)
        .with_context(|| {
            format!(
                "invalid rating '{}', expected again/hard/good/easy",
                args.rating
            )
        })?;
    let storage = open_storage()?;
    let Some(scope_deck) = resolve_deck_scope(&storage, args.deck.as_deref(), args.json)? else {
        if args.json {
            exit_json_error("no_cards", "No cards found. Import a deck first.");
        }
        anyhow::bail!("No cards found. Import a deck first.");
    };
    let Some(card_id) = storage
        .get_current_card_id()
        .context("failed to read current card")?
    else {
        if args.json {
            exit_json_error(
                "no_current_card",
                "No current card. Run `fishword next` first.",
            );
        }
        anyhow::bail!("No current card. Run `fishword next` first.");
    };
    let card = storage
        .get_card_by_id(card_id)
        .context("failed to read current card")?
        .context("Current card disappeared")?;
    if card.deck_id != scope_deck.id {
        if args.json {
            exit_json_error(
                "no_current_card",
                "No current card in this deck. Run `fishword next` first.",
            );
        }
        anyhow::bail!("No current card in this deck. Run `fishword next` first.");
    }
    let review = Scheduler::review(&storage, card_id, rating).context("failed to rate card")?;
    if args.json {
        let progress = storage
            .progress_counts_by_deck(scope_deck.id, 20)
            .context("failed to read progress")?;
        print_json(&RateResponse::new(&card, &scope_deck, &review, progress))?;
    } else {
        println!(
            "Rated {} as {}. due={} scheduled_days={}",
            card.word, review.rating, review.due, review.scheduled_days
        );
    }
    Ok(())
}

fn print_selected_card(
    storage: &Storage,
    selected: &SelectedCard,
    args: &CardOutputArgs,
    current: bool,
) -> Result<()> {
    let deck = storage
        .get_deck_by_id(selected.card.deck_id)
        .context("failed to read deck")?
        .context("Selected card deck disappeared")?;
    let progress = storage
        .progress_counts_by_deck(deck.id, 20)
        .context("failed to read progress")?;
    let response = if current {
        CardResponse::current(selected, &deck, progress)
    } else {
        CardResponse::next(selected, &deck, progress)
    };
    if args.json {
        print_json(&response)?;
    } else {
        let format = args
            .format
            .parse::<TextFormat>()
            .map_err(anyhow::Error::msg)
            .with_context(|| {
                format!(
                    "invalid --format '{}', expected plain/compact/status",
                    args.format
                )
            })?;
        println!("{}", render_card(&response, format));
    }
    Ok(())
}

fn resolve_deck_scope(
    storage: &Storage,
    deck_name: Option<&str>,
    json_errors: bool,
) -> Result<Option<Deck>> {
    if let Some(deck_name) = deck_name {
        return match storage
            .get_deck_by_name(deck_name)
            .with_context(|| format!("failed to read deck '{deck_name}'"))?
        {
            Some(deck) => Ok(Some(deck)),
            None if json_errors => {
                exit_json_error("deck_not_found", &format!("Deck not found: {deck_name}"));
            }
            None => anyhow::bail!("Deck not found: {deck_name}"),
        };
    }

    if let Some(deck) = storage
        .get_active_deck()
        .context("failed to read active deck")?
    {
        return Ok(Some(deck));
    }

    let decks = storage.list_decks().context("failed to list decks")?;
    match decks.as_slice() {
        [] => Ok(None),
        [deck] => {
            storage
                .set_active_deck_id(Some(deck.id))
                .context("failed to set active deck")?;
            Ok(Some(deck.clone()))
        }
        _ if json_errors => {
            exit_json_error(
                "no_active_deck",
                "Multiple decks found. Run `fishword deck use <deck>` or pass `--deck <deck>`.",
            );
        }
        _ => anyhow::bail!(
            "Multiple decks found. Run `fishword deck use <deck>` or pass `--deck <deck>`."
        ),
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}

fn exit_json_error(code: &str, message: &str) -> ! {
    println!(
        "{}",
        serde_json::to_string(&ErrorResponse::new(code, message))
            .expect("serializing protocol error should not fail")
    );
    std::process::exit(2);
}

fn cmd_import(command: ImportCmd) -> Result<()> {
    let (args, deck) = match command {
        ImportCmd::Qwerty(args) => {
            let deck = import_qwerty_file(&args.path, &args.deck, args.name.as_deref())
                .with_context(|| format!("failed to parse {}", args.path.display()))?;
            (args, deck)
        }
        ImportCmd::Csv(args) => {
            let deck = import_csv_file(&args.path, &args.deck, args.name.as_deref())
                .with_context(|| format!("failed to parse {}", args.path.display()))?;
            (args, deck)
        }
        ImportCmd::Jsonl(args) => {
            let deck = import_jsonl_file(&args.path, &args.deck, args.name.as_deref())
                .with_context(|| format!("failed to parse {}", args.path.display()))?;
            (args, deck)
        }
        ImportCmd::AnkiTsv(args) => {
            let deck = import_anki_tsv_file(&args.path, &args.deck, args.name.as_deref())
                .with_context(|| format!("failed to parse {}", args.path.display()))?;
            (args, deck)
        }
    };
    persist_import(args, deck)
}

fn persist_import(args: ImportArgs, deck: ImportDeck) -> Result<()> {
    let duplicate_strategy = DuplicateStrategy::from_str(&args.duplicates)
        .with_context(|| format!("invalid --duplicates value '{}'", args.duplicates))?;
    let storage = open_storage()?;
    let summary = storage
        .import_cards(
            &deck.deck_id,
            deck.deck_name.as_deref(),
            &deck.cards,
            duplicate_strategy,
        )
        .context("failed to write imported cards")?;
    if storage
        .get_active_deck_id()
        .context("failed to read active deck")?
        .is_none()
    {
        storage
            .set_active_deck_id(Some(summary.deck_id))
            .context("failed to set active deck")?;
    }

    println!(
        "Imported deck={} input={} inserted={} updated={} merged={} skipped={}",
        summary.deck_name,
        summary.input_count,
        summary.inserted,
        summary.updated,
        summary.merged,
        summary.skipped
    );
    Ok(())
}
