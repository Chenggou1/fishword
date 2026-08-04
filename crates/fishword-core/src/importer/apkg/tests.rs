//! apkg 解析器的单元测试。fixture 全部程序化构造（建 anki2 schema → 写临时文件 →
//! 读字节 → 打 zip），不依赖磁盘上的真实 .apkg 文件。
//!
//! 覆盖：基本解析、IPA 误标字段、媒体忽略、--map 覆盖（索引/字段名/部分）、
//! inspect、HTML 清洗、错误路径、anki21b zstd、schema 15+、多 deck tag、端到端持久化。

use std::collections::HashMap;
use std::io::Write;

use rusqlite::Connection;
use zip::{write::SimpleFileOptions, ZipWriter};

use super::*;
use crate::importer::DuplicateStrategy;
use crate::storage::Storage;

const SEP: char = '\u{1f}';

// ---- fixture 构造 ----

/// 一条 note 的测试数据：字段值 + 标签 + 所属 deck id。
struct NoteSpec {
    fields: Vec<String>,
    tags: Vec<String>,
    deck_id: Option<i64>,
}

/// 构造一个 schema 11 的 anki2 数据库字节。`models_json` 是 col.models 的 JSON。
fn build_anki2_v11(models_json: &str, decks_json: &str, notes: &[NoteSpec]) -> Vec<u8> {
    let schema = "CREATE TABLE col (models TEXT, decks TEXT);
                  CREATE TABLE notes (id INTEGER, mid INTEGER, flds TEXT, tags TEXT);
                  CREATE TABLE cards (id INTEGER, nid INTEGER, did INTEGER, ord INTEGER);";
    build_db(|conn| {
        conn.execute_batch(schema).unwrap();
        conn.execute(
            "INSERT INTO col (models, decks) VALUES (?1, ?2)",
            rusqlite::params![models_json, decks_json],
        )
        .unwrap();
        for (index, note) in notes.iter().enumerate() {
            let note_id = (index + 1) as i64;
            let mid = 100_i64;
            let flds = note.fields.join(&SEP.to_string());
            let tags = note.tags.join(" ");
            conn.execute(
                "INSERT INTO notes (id, mid, flds, tags) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![note_id, mid, flds, tags],
            )
            .unwrap();
            if let Some(deck_id) = note.deck_id {
                conn.execute(
                    "INSERT INTO cards (id, nid, did, ord) VALUES (?1, ?2, ?3, 0)",
                    rusqlite::params![note_id, note_id, deck_id],
                )
                .unwrap();
            }
        }
    })
}

/// 构造 schema 15+ 的 anki2 数据库字节（notetype/field/deck 走独立表）。
fn build_anki2_v15(
    notetype_name: &str,
    fields: &[(&str, usize)],
    decks: &[(i64, &str)],
    notes: &[NoteSpec],
) -> Vec<u8> {
    let schema = "CREATE TABLE notetypes (id INTEGER, name TEXT);
                  CREATE TABLE fields (ntid INTEGER, ord INTEGER, name TEXT);
                  CREATE TABLE decks (id INTEGER, name TEXT);
                  CREATE TABLE notes (id INTEGER, mid INTEGER, flds TEXT, tags TEXT);
                  CREATE TABLE cards (id INTEGER, nid INTEGER, did INTEGER, ord INTEGER);";
    build_db(|conn| {
        conn.execute_batch(schema).unwrap();
        conn.execute(
            "INSERT INTO notetypes (id, name) VALUES (100, ?1)",
            rusqlite::params![notetype_name],
        )
        .unwrap();
        for (name, ord) in fields {
            conn.execute(
                "INSERT INTO fields (ntid, ord, name) VALUES (100, ?1, ?2)",
                rusqlite::params![*ord as i64, name],
            )
            .unwrap();
        }
        for (id, name) in decks {
            conn.execute(
                "INSERT INTO decks (id, name) VALUES (?1, ?2)",
                rusqlite::params![id, name],
            )
            .unwrap();
        }
        for (index, note) in notes.iter().enumerate() {
            let note_id = (index + 1) as i64;
            let flds = note.fields.join(&SEP.to_string());
            let tags = note.tags.join(" ");
            conn.execute(
                "INSERT INTO notes (id, mid, flds, tags) VALUES (?1, 100, ?2, ?3)",
                rusqlite::params![note_id, flds, tags],
            )
            .unwrap();
            if let Some(deck_id) = note.deck_id {
                conn.execute(
                    "INSERT INTO cards (id, nid, did, ord) VALUES (?1, ?2, ?3, 0)",
                    rusqlite::params![note_id, note_id, deck_id],
                )
                .unwrap();
            }
        }
    })
}

/// 在临时文件里建库，关连接后读字节（避免依赖 rusqlite backup feature）。
fn build_db<F: FnOnce(&Connection)>(populate: F) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.anki2");
    {
        let conn = Connection::open(&path).unwrap();
        populate(&conn);
    }
    std::fs::read(&path).unwrap()
}

/// 把 collection 字节打成 anki2/anki21 格式的 apkg。
fn build_apkg(db_bytes: &[u8]) -> Vec<u8> {
    build_apkg_named("collection.anki2", db_bytes, &[])
}

/// anki21b 格式：zstd 压缩 db + 一个占位 meta 文件（reader 只检测 meta 是否存在）。
fn build_apkg_anki21b(db_bytes: &[u8]) -> Vec<u8> {
    let compressed = zstd::stream::encode_all(db_bytes, 3).unwrap();
    build_apkg_named(
        "collection.anki21b",
        &compressed,
        &[("meta", b"\x00\x00\x00\x00")],
    )
}

fn build_apkg_named(collection_name: &str, db_bytes: &[u8], extras: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let options = SimpleFileOptions::default();
        zip.start_file(collection_name, options).unwrap();
        zip.write_all(db_bytes).unwrap();
        for (name, data) in extras {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn models_json(fields: &[(&str, usize)]) -> String {
    let flds: Vec<String> = fields
        .iter()
        .map(|(name, ord)| format!(r#"{{"ord":{ord},"name":"{name}"}}"#))
        .collect();
    format!(
        r#"{{"100":{{"id":100,"name":"Basic","flds":[{}]}}}}"#,
        flds.join(",")
    )
}

fn default_options() -> ApkgImportOptions {
    ApkgImportOptions::default()
}

fn options_with_map(specs: &[&str]) -> ApkgImportOptions {
    ApkgImportOptions {
        language: "en".to_string(),
        field_map: FieldMap::parse(&specs.iter().map(|s| s.to_string()).collect::<Vec<_>>())
            .unwrap(),
    }
}

// ---- 解析测试 ----

#[test]
fn parses_basic_schema11_note() {
    let models = models_json(&[("Front", 0), ("Back", 1)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec!["cancel".to_string(), "取消".to_string()],
            tags: vec![],
            deck_id: Some(1),
        }],
    );
    let apkg = build_apkg(&db);
    let cards = import_apkg_bytes(&apkg, &default_options()).unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].word, "cancel");
    assert_eq!(cards[0].meanings[0].definition, "取消");
    assert_eq!(cards[0].tags, vec!["Default"]);
}

#[test]
fn ipa_in_mislabeled_field_becomes_phonetic() {
    // 字段名是 pos，内容是 IPA——内容特征必须赢过字段名。
    let models = models_json(&[("word", 0), ("pos", 1), ("definition", 2)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec![
                "essence".to_string(),
                "[ˈesns]".to_string(),
                "n. 本质".to_string(),
            ],
            tags: vec![],
            deck_id: None,
        }],
    );
    let cards = import_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();
    assert_eq!(cards[0].word, "essence");
    assert_eq!(cards[0].pronunciations.len(), 1);
    assert_eq!(cards[0].pronunciations[0].notation, "[ˈesns]");
    assert_eq!(cards[0].meanings[0].definition, "n. 本质");
}

#[test]
fn audio_and_image_only_fields_are_ignored() {
    let models = models_json(&[("word", 0), ("audio", 1), ("image", 2), ("definition", 3)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec![
                "test".to_string(),
                "[sound:test.mp3]".to_string(),
                "<img src=\"test.jpg\">".to_string(),
                "测试".to_string(),
            ],
            tags: vec![],
            deck_id: None,
        }],
    );
    let cards = import_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();
    assert_eq!(cards[0].word, "test");
    assert!(cards[0].pronunciations.is_empty());
    assert_eq!(cards[0].meanings.len(), 1);
    assert_eq!(cards[0].meanings[0].definition, "测试");
}

#[test]
fn html_is_cleaned_and_audio_stripped() {
    let models = models_json(&[("word", 0), ("definition", 1), ("example", 2)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec![
                "absorb".to_string(),
                "v.吸收 <br />n.吸收体&nbsp;[sound:x.mp3]".to_string(),
                "<div>Plants <b>absorb</b> nutrients.</div>".to_string(),
            ],
            tags: vec![],
            deck_id: None,
        }],
    );
    let cards = import_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();
    assert_eq!(cards[0].meanings[0].definition, "v.吸收; n.吸收体");
    assert_eq!(
        cards[0].meanings[0].example.as_deref(),
        Some("Plants absorb nutrients.")
    );
}

// ---- --map 覆盖测试 ----

#[test]
fn map_by_index_overrides_auto_detection() {
    let models = models_json(&[("definition", 0), ("word", 1)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec!["你好释义".to_string(), "你好".to_string()],
            tags: vec![],
            deck_id: None,
        }],
    );
    // 强制把字段 1 当 term（覆盖 ord 0 → term 的默认）；ord 0 降为 definition。
    let cards = import_apkg_bytes(&build_apkg(&db), &options_with_map(&["term=1"])).unwrap();
    assert_eq!(cards[0].word, "你好");
    assert_eq!(cards[0].meanings[0].definition, "你好释义");
}

#[test]
fn map_by_field_name_overrides_auto_detection() {
    let models = models_json(&[("Front", 0), ("Notes", 1)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec!["go".to_string(), "走，去".to_string()],
            tags: vec![],
            deck_id: None,
        }],
    );
    let cards =
        import_apkg_bytes(&build_apkg(&db), &options_with_map(&["definition=Notes"])).unwrap();
    assert_eq!(cards[0].word, "go");
    assert_eq!(cards[0].meanings[0].definition, "走，去");
}

#[test]
fn map_partial_override_leaves_rest_auto() {
    let models = models_json(&[("word", 0), ("phon", 1), ("meaning", 2)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec!["run".to_string(), "[rʌn]".to_string(), "v.跑".to_string()],
            tags: vec![],
            deck_id: None,
        }],
    );
    // 只显式 ignore phon，其余自动。
    let cards = import_apkg_bytes(&build_apkg(&db), &options_with_map(&["ignore=phon"])).unwrap();
    assert_eq!(cards[0].word, "run");
    assert!(cards[0].pronunciations.is_empty());
    assert_eq!(cards[0].meanings[0].definition, "v.跑");
}

#[test]
fn map_unknown_field_name_errors() {
    let models = models_json(&[("word", 0), ("back", 1)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec!["x".to_string(), "X".to_string()],
            tags: vec![],
            deck_id: None,
        }],
    );
    let result = import_apkg_bytes(&build_apkg(&db), &options_with_map(&["term=Nonexistent"]));
    assert!(matches!(result, Err(ApkgError::FieldNotFound(_))));
}

#[test]
fn map_invalid_syntax_errors() {
    assert!(matches!(
        FieldMap::parse(&["bogus".to_string()]),
        Err(ApkgError::InvalidMap(_))
    ));
    assert!(matches!(
        FieldMap::parse(&["unknown_role=0".to_string()]),
        Err(ApkgError::InvalidMap(_))
    ));
}

// ---- inspect 测试 ----

#[test]
fn inspect_reports_role_and_confidence() {
    let models = models_json(&[("word", 0), ("phonetic", 1), ("definition", 2)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[
            NoteSpec {
                fields: vec!["a".to_string(), "[ə]".to_string(), "甲".to_string()],
                tags: vec![],
                deck_id: None,
            },
            NoteSpec {
                fields: vec!["b".to_string(), "[i:]".to_string(), "乙".to_string()],
                tags: vec![],
                deck_id: None,
            },
        ],
    );
    let report = inspect_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();
    assert_eq!(report.note_count, 2);
    let nt = &report.notetypes[0];
    let by_ord: HashMap<usize, _> = nt.fields.iter().map(|f| (f.ord, f.role)).collect();
    assert_eq!(by_ord[&0], FieldRole::Term);
    assert_eq!(by_ord[&1], FieldRole::Phonetic);
    assert_eq!(by_ord[&2], FieldRole::Definition);
    // 置信度应接近 1.0。
    assert!((nt.fields[1].confidence - 1.0).abs() < 1e-9);
}

// ---- sort_field 在 ord 0 的边界 ----

#[test]
fn sort_field_at_ord0_does_not_become_term() {
    // 实测 MuJing basic_word.apkg：ord 0 是 sort_field（值 "07-02-01"），真单词在 english(ord1)。
    let models = models_json(&[
        ("sort_field", 0),
        ("english", 1),
        ("pronunciation", 2),
        ("part_of_speech", 3),
        ("chinese", 4),
        ("example", 5),
    ]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec![
                "07-02-01".to_string(),
                "Africa".to_string(),
                "/ˈæfrɪkə/".to_string(),
                "n.".to_string(),
                "非洲".to_string(),
                "Lions live in Africa.".to_string(),
            ],
            tags: vec![],
            deck_id: None,
        }],
    );
    let cards = import_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(
        cards[0].word, "Africa",
        "term 应来自 english 字段，不是 sort_field"
    );
    assert_eq!(cards[0].pronunciations[0].notation, "/ˈæfrɪkə/");
    assert_eq!(cards[0].meanings[0].definition, "非洲");
    assert_eq!(cards[0].meanings[0].part_of_speech, "n.");
    assert_eq!(
        cards[0].meanings[0].example.as_deref(),
        Some("Lions live in Africa.")
    );
}

// ---- 错误路径测试 ----

#[test]
fn missing_collection_errors() {
    // zip 里没有 collection.anki*。
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        zip.start_file("media", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"{}").unwrap();
        zip.finish().unwrap();
    }
    let result = import_apkg_bytes(&buf.into_inner(), &default_options());
    assert!(matches!(result, Err(ApkgError::MissingCollection)));
}

#[test]
fn empty_notes_errors() {
    let models = models_json(&[("Front", 0), ("Back", 1)]);
    let db = build_anki2_v11(&models, r#"{"1":{"name":"Default"}}"#, &[]);
    let result = import_apkg_bytes(&build_apkg(&db), &default_options());
    assert!(matches!(result, Err(ApkgError::EmptyCollection)));
}

#[test]
fn empty_word_field_errors_no_term() {
    // word 字段为空 → 识别不出单词 → NoTerm（明确报错，引导用户 --inspect/--map）。
    let models = models_json(&[("Front", 0)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec![String::new()],
            tags: vec![],
            deck_id: None,
        }],
    );
    let result = import_apkg_bytes(&build_apkg(&db), &default_options());
    assert!(matches!(result, Err(ApkgError::NoTerm)));
}

#[test]
fn no_term_field_anywhere_errors_no_term() {
    // ord 0 是 sort 键、其余字段名都不像 term → 全部 note 识别不出单词 → NoTerm。
    let models = models_json(&[("sort_field", 0), ("glossary", 1)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec!["001".to_string(), "a word meaning".to_string()],
            tags: vec![],
            deck_id: None,
        }],
    );
    let result = import_apkg_bytes(&build_apkg(&db), &default_options());
    assert!(matches!(result, Err(ApkgError::NoTerm)));
}

#[test]
fn pos_prefixed_content_becomes_definition() {
    // 内容 `n. 本质` 带词性前缀 → 不靠字段名也能判成 definition。
    let models = models_json(&[("word", 0), ("gloss", 1)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec!["cancel".to_string(), "vt.取消 n.撤销".to_string()],
            tags: vec![],
            deck_id: None,
        }],
    );
    let cards = import_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();
    assert_eq!(cards[0].meanings[0].definition, "vt.取消 n.撤销");
}

#[test]
fn bare_pos_marker_not_treated_as_definition() {
    // 光秃秃 `n.`（词性字段）不应被当释义——必须有正文。
    let models = models_json(&[("word", 0), ("pos", 1), ("meaning", 2)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec!["go".to_string(), "v.".to_string(), "去".to_string()],
            tags: vec![],
            deck_id: None,
        }],
    );
    let cards = import_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();
    // pos 字段 `v.` → PartOfSpeech（不是 definition）；meaning 字段 → definition。
    assert_eq!(cards[0].meanings[0].part_of_speech, "v.");
    assert_eq!(cards[0].meanings[0].definition, "去");
}

// ---- anki21b zstd 路径 ----
#[test]
fn anki21b_zstd_path_decodes_and_parses() {
    let models = models_json(&[("word", 0), ("definition", 1)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec!["test".to_string(), "测试".to_string()],
            tags: vec![],
            deck_id: None,
        }],
    );
    let apkg = build_apkg_anki21b(&db);
    let cards = import_apkg_bytes(&apkg, &default_options()).unwrap();
    assert_eq!(cards[0].word, "test");
    assert_eq!(cards[0].meanings[0].definition, "测试");
}

// ---- schema 15+ 路径 ----

#[test]
fn schema15_notetype_tables_path_parses() {
    let db = build_anki2_v15(
        "Basic",
        &[("word", 0), ("phon", 1), ("definition", 2)],
        &[(1, "Default")],
        &[NoteSpec {
            fields: vec!["run".to_string(), "[rʌn]".to_string(), "v.跑".to_string()],
            tags: vec![],
            deck_id: Some(1),
        }],
    );
    let cards = import_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();
    assert_eq!(cards[0].word, "run");
    assert_eq!(cards[0].pronunciations[0].notation, "[rʌn]");
    assert_eq!(cards[0].meanings[0].definition, "v.跑");
    assert_eq!(cards[0].tags, vec!["Default"]);
}

// ---- 多 deck → tag ----

#[test]
fn multi_deck_names_become_tags() {
    let models = models_json(&[("word", 0), ("definition", 1)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Animals"},"2":{"name":"Food"}}"#,
        &[
            NoteSpec {
                fields: vec!["cat".to_string(), "猫".to_string()],
                tags: vec![],
                deck_id: Some(1),
            },
            NoteSpec {
                fields: vec!["bread".to_string(), "面包".to_string()],
                tags: vec![],
                deck_id: Some(2),
            },
        ],
    );
    let cards = import_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();
    let by_word: HashMap<&str, &[String]> = cards
        .iter()
        .map(|c| (c.word.as_str(), c.tags.as_slice()))
        .collect();
    assert!(by_word["cat"].contains(&"Animals".to_string()));
    assert!(by_word["bread"].contains(&"Food".to_string()));
}

// ---- 端到端：parse → storage 持久化 → 读回 ----

#[test]
fn end_to_end_parse_persist_readback() {
    let models = models_json(&[("word", 0), ("phonetic", 1), ("definition", 2)]);
    let db = build_anki2_v11(
        &models,
        r#"{"1":{"name":"Default"}}"#,
        &[NoteSpec {
            fields: vec![
                "absorb".to_string(),
                "[əbˈsɔːb]".to_string(),
                "v.吸收".to_string(),
            ],
            tags: vec!["science".to_string()],
            deck_id: Some(1),
        }],
    );
    let cards = import_apkg_bytes(&build_apkg(&db), &default_options()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(&dir.path().join("e2e.db")).unwrap();
    let deck = storage.insert_deck("test", None).unwrap();
    let summary = storage
        .import_cards(deck.id, &cards, DuplicateStrategy::Merge)
        .unwrap();
    assert_eq!(summary.inserted, 1);

    let stored = storage
        .list_cards_by_deck_paginated(deck.id, 10, 0)
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].word, "absorb");
    assert_eq!(stored[0].pronunciations[0].notation, "[əbˈsɔːb]");
    assert_eq!(stored[0].meanings[0].definition, "v.吸收");
    assert!(stored[0].tags.iter().any(|t| t == "science"));
}

// ---- 多 notetype 牌组：--map 跳过不存在字段 ----

/// 构造 schema 15+ 的多 notetype 数据库（每个 notetype 独立 id）。
fn build_multi_notetype_db() -> Vec<u8> {
    let schema = "CREATE TABLE notetypes (id INTEGER, name TEXT);
                  CREATE TABLE fields (ntid INTEGER, ord INTEGER, name TEXT);
                  CREATE TABLE decks (id INTEGER, name TEXT);
                  CREATE TABLE notes (id INTEGER, mid INTEGER, flds TEXT, tags TEXT);
                  CREATE TABLE cards (id INTEGER, nid INTEGER, did INTEGER, ord INTEGER);";
    build_db(|conn| {
        conn.execute_batch(schema).unwrap();
        // Notetype A：有 Front 字段
        conn.execute("INSERT INTO notetypes (id, name) VALUES (100, 'TypeA')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fields (ntid, ord, name) VALUES (100, 0, 'Front')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fields (ntid, ord, name) VALUES (100, 1, 'Back')",
            [],
        )
        .unwrap();
        // Notetype B：没有 Front 字段（只有 Word/Definition）
        conn.execute("INSERT INTO notetypes (id, name) VALUES (200, 'TypeB')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fields (ntid, ord, name) VALUES (200, 0, 'Word')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fields (ntid, ord, name) VALUES (200, 1, 'Definition')",
            [],
        )
        .unwrap();
        // Deck
        conn.execute("INSERT INTO decks (id, name) VALUES (1, 'Default')", [])
            .unwrap();
        // Note A：属于 TypeA，有 Front
        conn.execute(
            "INSERT INTO notes (id, mid, flds, tags) VALUES (1, 100, ?, '')",
            [["hello".to_string(), "你好".to_string()].join(&SEP.to_string())],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards (id, nid, did, ord) VALUES (1, 1, 1, 0)",
            [],
        )
        .unwrap();
        // Note B：属于 TypeB，没有 Front
        conn.execute(
            "INSERT INTO notes (id, mid, flds, tags) VALUES (2, 200, ?, '')",
            [["world".to_string(), "世界".to_string()].join(&SEP.to_string())],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards (id, nid, did, ord) VALUES (2, 2, 1, 0)",
            [],
        )
        .unwrap();
    })
}

#[test]
fn multi_notetype_skips_type_without_mapped_field() {
    // 两个 notetype：TypeA 有 Front，TypeB 没有。
    // --map term=Front 时，TypeA 应正常导入，TypeB 被跳过（不中断整次导入）。
    let db = build_multi_notetype_db();
    let apkg = build_apkg(&db);
    let options = options_with_map(&["term=Front"]);

    let result = import_apkg_bytes(&apkg, &options);
    assert!(
        result.is_ok(),
        "应成功导入，不应因 TypeB 缺少 Front 字段而失败"
    );

    let cards = result.unwrap();
    assert_eq!(cards.len(), 1, "应只产出 TypeA 的卡片，TypeB 被跳过");
    assert_eq!(cards[0].word, "hello", "TypeA 的 Front 字段应被映射为 term");
}

#[test]
fn inspect_multi_notetype_skips_type_without_mapped_field() {
    // inspect 同理：解析失败的 notetype 不纳入报告，不中断 inspect。
    let db = build_multi_notetype_db();
    let apkg = build_apkg(&db);
    let options = options_with_map(&["term=Front"]);

    let result = inspect_apkg_bytes(&apkg, &options);
    assert!(
        result.is_ok(),
        "inspect 应成功，不应因 TypeB 缺少 Front 字段而失败"
    );

    let report = result.unwrap();
    assert_eq!(report.notetypes.len(), 1, "应只报告 TypeA，TypeB 被跳过");
    assert_eq!(report.notetypes[0].name, "TypeA");
}
