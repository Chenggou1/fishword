use std::str::FromStr;

use anyhow::{Context, Result};
use fishword_core::{
    deck::Deck,
    error::Error as CoreError,
    importer::{
        apkg::{
            import_apkg_file, inspect_apkg_file, ApkgError, ApkgImportOptions, ApkgInspectReport,
            FieldMap, NotetypeInspection,
        },
        import_jsonl_file, DuplicateStrategy,
    },
};

use crate::protocol::{ImportResponse, IMPORT_SCHEMA};

use crate::{
    args::{ApkgImportArgs, ImportArgs, ImportCmd},
    util::{cmd_error, open_storage, print_human, print_json},
};

enum ImportTarget {
    ExistingDeck(i64),
    CreateDeck(String),
}

impl ImportTarget {
    /// 从 `--deck-id` / `--create-deck` 解析导入目标。两者必须传且仅传一个。
    fn resolve(deck_id: Option<i64>, create_deck: Option<&str>) -> Result<Self> {
        match (deck_id, create_deck) {
            (Some(deck_id), None) => Ok(Self::ExistingDeck(deck_id)),
            (None, Some(name)) => Ok(Self::CreateDeck(name.to_string())),
            _ => anyhow::bail!("pass exactly one of --deck-id or --create-deck"),
        }
    }
}

pub fn cmd_import(command: ImportCmd) -> Result<()> {
    match command {
        ImportCmd::Jsonl(args) => cmd_import_jsonl(args),
        ImportCmd::Apkg(args) => cmd_import_apkg(args),
    }
}

fn cmd_import_jsonl(args: ImportArgs) -> Result<()> {
    let target = ImportTarget::resolve(args.deck_id, args.create_deck.as_deref())?;
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

fn cmd_import_apkg(args: ApkgImportArgs) -> Result<()> {
    let field_map = FieldMap::parse(&args.map).map_err(|e| apkg_error_to_cmd(args.json, e))?;
    let options = ApkgImportOptions {
        language: args.language.clone(),
        field_map,
    };

    if args.inspect {
        // inspect 是纯文本诊断工具，全程忽略 --json（与成功路径一致）。
        let report =
            inspect_apkg_file(&args.path, &options).map_err(|e| apkg_error_to_cmd(false, e))?;
        print_human(render_inspect_report(&args.path, &report));
        return Ok(());
    }

    let target = ImportTarget::resolve(args.deck_id, args.create_deck.as_deref())?;
    let cards =
        import_apkg_file(&args.path, &options).map_err(|e| apkg_error_to_cmd(args.json, e))?;
    // import_apkg_file 在无可导入卡时返回 NoCards，不会返回空 Vec。
    persist_import(target, cards, &args.duplicates, args.json)
}

/// 把 [`ApkgError`] 映射成稳定的协议错误码 + 人类可读消息。
fn apkg_error_to_cmd(json: bool, error: ApkgError) -> anyhow::Error {
    let code = match &error {
        ApkgError::InvalidZip(_) => "apkg_invalid_zip",
        ApkgError::MissingCollection => "apkg_missing_collection",
        ApkgError::ZstdDecode(_) => "apkg_zstd_decode",
        ApkgError::InvalidDatabase(_) => "apkg_invalid_database",
        ApkgError::EmptyCollection => "apkg_empty_collection",
        ApkgError::NoCards => "apkg_no_cards",
        ApkgError::NoTerm => "apkg_no_term",
        ApkgError::InvalidMap(_) => "apkg_invalid_map",
        ApkgError::FieldNotFound(_) => "apkg_field_not_found",
    };
    cmd_error(json, code, &error.to_string())
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

/// 渲染 apkg 字段映射检查报告为人类可读文本（经 `print_human` 输出，遵守输出契约）。
/// 每个 notetype 一节，列出字段→角色+置信度+样本。
fn render_inspect_report(path: &std::path::Path, report: &ApkgInspectReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Anki package: {} ({} notes, {} notetype{})",
        path.display(),
        report.note_count,
        report.notetypes.len(),
        if report.notetypes.len() == 1 { "" } else { "s" }
    );
    for notetype in &report.notetypes {
        out.push('\n');
        render_notetype(&mut out, notetype);
    }
    out.push('\n');
    out.push_str("Run without --inspect to import, or pass --map role=selector to override.");
    out
}

fn render_notetype(out: &mut String, notetype: &NotetypeInspection) {
    use std::fmt::Write;
    let _ = writeln!(
        out,
        "Notetype \"{}\" ({} notes):",
        notetype.name, notetype.note_count
    );
    let _ = writeln!(
        out,
        "  {:>4}  {:<16} {:<12} {:>6}  samples",
        "ord", "field", "role", "conf"
    );
    for field in &notetype.fields {
        let confidence = format!("{:.0}%", field.confidence * 100.0);
        let samples = if field.samples.is_empty() {
            "(empty)".to_string()
        } else {
            field.samples.join(" · ")
        };
        let _ = writeln!(
            out,
            "  {:>4}  {:<16} {:<12} {:>6}  {}",
            field.ord,
            field.name,
            field.role.as_str(),
            confidence,
            samples
        );
    }
}
