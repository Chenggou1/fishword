use anyhow::{Context, Result};
use clap::error::ErrorKind;
use fishword_core::{selector::SelectedCard, storage::Storage};

use crate::protocol::{render_card, CardResponse, ErrorResponse, TextFormat};

use crate::args::CardOutputArgs;

pub fn open_storage() -> Result<Storage> {
    let path = Storage::default_path().context("cannot determine data directory")?;
    Storage::open(&path).with_context(|| format!("cannot open database at {}", path.display()))
}

pub fn print_selected_card(
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
        .progress_counts_by_deck(deck.id)
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
        print_human(render_card(&response, format));
    }
    Ok(())
}

pub fn print_human(value: impl std::fmt::Display) {
    println!("{value}");
}

pub fn print_json<T: serde::Serialize>(value: T) -> Result<()> {
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

#[derive(Debug)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}

impl ProtocolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProtocolError {}

/// Returns an error with a stable protocol code. The top-level CLI entry point
/// decides whether to render it as JSON or as human-readable text.
pub fn cmd_error(_json: bool, code: &str, message: &str) -> anyhow::Error {
    anyhow::Error::new(ProtocolError::new(code, message))
}

pub fn render_cli_error(json: bool, err: anyhow::Error) -> ! {
    if json {
        let response = err
            .chain()
            .find_map(|cause| cause.downcast_ref::<ProtocolError>())
            .map(|err| ErrorResponse::new(&err.code, &err.message))
            .unwrap_or_else(|| ErrorResponse::new("internal_error", err.to_string()));
        println!(
            "{}",
            serde_json::to_string(&response).expect("serializing protocol error should not fail")
        );
    } else {
        eprintln!("Error: {err:?}");
    }
    std::process::exit(1);
}

pub fn render_clap_error_json(err: clap::Error) -> ! {
    let code = clap_error_code(err.kind());
    let message = err.render().to_string();
    println!(
        "{}",
        serde_json::to_string(&ErrorResponse::new(code, message.trim()))
            .expect("serializing protocol error should not fail")
    );
    std::process::exit(err.exit_code());
}

fn clap_error_code(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::InvalidValue | ErrorKind::ValueValidation | ErrorKind::InvalidUtf8 => {
            "invalid_argument"
        }
        ErrorKind::UnknownArgument => "unknown_argument",
        ErrorKind::InvalidSubcommand => "invalid_subcommand",
        ErrorKind::ArgumentConflict => "argument_conflict",
        ErrorKind::MissingRequiredArgument
        | ErrorKind::MissingSubcommand
        | ErrorKind::TooFewValues => "missing_required_argument",
        ErrorKind::TooManyValues | ErrorKind::WrongNumberOfValues => "wrong_number_of_values",
        ErrorKind::NoEquals => "invalid_argument",
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        | ErrorKind::DisplayVersion => "display_requested",
        ErrorKind::Io | ErrorKind::Format => "cli_parse_error",
        _ => "cli_parse_error",
    }
}
