use std::{path::Path, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{
    card::{Card, Meaning, Pronunciation, Source},
    error::{Error, Result},
};

pub mod apkg;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DuplicateStrategy {
    #[default]
    Merge,
    Skip,
    Overwrite,
    Keep,
}

impl FromStr for DuplicateStrategy {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "merge" => Ok(Self::Merge),
            "skip" => Ok(Self::Skip),
            "overwrite" => Ok(Self::Overwrite),
            "keep" => Ok(Self::Keep),
            other => Err(Error::InvalidInput(format!(
                "unknown duplicate strategy '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportCard {
    pub word: String,
    pub language: String,
    pub meanings: Vec<Meaning>,
    pub pronunciations: Vec<Pronunciation>,
    pub tags: Vec<String>,
    pub source: Option<Source>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub deck_id: i64,
    pub deck_name: String,
    pub input_count: usize,
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    pub merged: usize,
}

#[derive(Debug, Deserialize)]
struct DeckCardV1 {
    #[serde(alias = "term")]
    word: String,
    #[serde(default = "default_language")]
    language: String,
    #[serde(default)]
    meanings: Vec<DeckMeaningV1>,
    #[serde(default)]
    pronunciation: Option<DeckPronunciationV1>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    source: Option<Source>,
}

#[derive(Debug, Deserialize)]
struct DeckMeaningV1 {
    #[serde(default = "default_meaning_lang")]
    lang: String,
    text: String,
    #[serde(default)]
    example: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeckPronunciationV1 {
    #[serde(default)]
    us: Option<String>,
    #[serde(default)]
    uk: Option<String>,
}

pub fn import_jsonl_file(path: &Path) -> Result<Vec<ImportCard>> {
    let text = std::fs::read_to_string(path)?;
    import_jsonl_str(&text)
}

pub fn import_jsonl_str(text: &str) -> Result<Vec<ImportCard>> {
    let mut cards = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let card: DeckCardV1 = serde_json::from_str(line)
            .map_err(|error| Error::InvalidInput(format!("JSONL line {}: {error}", index + 1)))?;
        cards.push(deck_v1_to_import_card(card)?);
    }
    Ok(cards)
}

fn deck_v1_to_import_card(card: DeckCardV1) -> Result<ImportCard> {
    if card.word.trim().is_empty() {
        return Err(Error::InvalidInput(
            "deck.v1 card word is empty".to_string(),
        ));
    }
    let meanings = card
        .meanings
        .into_iter()
        .map(|meaning| Meaning {
            part_of_speech: meaning.lang,
            definition: meaning.text,
            example: meaning.example,
        })
        .collect::<Vec<_>>();
    let mut pronunciations = Vec::new();
    if let Some(pronunciation) = card.pronunciation {
        if let Some(us) = pronunciation.us.filter(|value| !value.trim().is_empty()) {
            pronunciations.push(Pronunciation {
                notation: format!("US {us}"),
                audio_url: None,
            });
        }
        if let Some(uk) = pronunciation.uk.filter(|value| !value.trim().is_empty()) {
            pronunciations.push(Pronunciation {
                notation: format!("UK {uk}"),
                audio_url: None,
            });
        }
    }
    Ok(ImportCard {
        word: card.word,
        language: card.language,
        meanings,
        pronunciations,
        tags: card.tags,
        source: card.source,
    })
}

/// Merges an incoming import card into an existing card, combining meanings,
/// pronunciations, and tags without duplicates. This is the implementation of
/// `DuplicateStrategy::Merge` — kept here because merge policy belongs to the
/// import domain, not to storage.
pub(crate) fn merge_import_card(existing: &Card, incoming: &ImportCard) -> ImportCard {
    let mut meanings = existing.meanings.clone();
    for meaning in &incoming.meanings {
        if !meanings.iter().any(|item| {
            item.part_of_speech == meaning.part_of_speech && item.definition == meaning.definition
        }) {
            meanings.push(meaning.clone());
        }
    }

    let mut pronunciations = existing.pronunciations.clone();
    for pronunciation in &incoming.pronunciations {
        if !pronunciations
            .iter()
            .any(|item| item.notation == pronunciation.notation)
        {
            pronunciations.push(pronunciation.clone());
        }
    }

    let mut tags = existing.tags.clone();
    for tag in &incoming.tags {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }

    ImportCard {
        word: existing.word.clone(),
        language: if incoming.language.is_empty() {
            existing.language.clone()
        } else {
            incoming.language.clone()
        },
        meanings,
        pronunciations,
        tags,
        source: incoming.source.clone().or_else(|| existing.source.clone()),
    }
}

fn default_language() -> String {
    "en".to_string()
}

fn default_meaning_lang() -> String {
    "zh-CN".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name);
        std::fs::read_to_string(path).unwrap()
    }

    fn open_temp() -> Storage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("importer.db");
        Storage::open(&path).unwrap()
    }

    #[test]
    fn imports_jsonl() {
        let cards = import_jsonl_str(&fixture("deck_v1_sample.jsonl")).unwrap();
        assert_eq!(cards[0].word, "cancel");
        assert_eq!(cards[0].pronunciations.len(), 2);
    }

    #[test]
    fn importer_preserves_only_explicit_tags() {
        let cards =
            import_jsonl_str(r#"{"term":"cancel","meanings":[{"lang":"zh-CN","text":"取消"}]}"#)
                .unwrap();
        assert!(cards[0].tags.is_empty());

        let tagged = import_jsonl_str(
            r#"{"term":"cancel","meanings":[{"lang":"zh-CN","text":"取消"}],"tags":["review","hard"]}"#,
        )
        .unwrap();
        assert_eq!(tagged[0].tags, vec!["review", "hard"]);
    }

    #[test]
    fn duplicate_skip_and_merge_work_in_storage() {
        let storage = open_temp();
        let deck = storage.insert_deck("cet4", Some("CET-4")).unwrap();
        let initial =
            import_jsonl_str(r#"{"term":"cancel","meanings":[{"lang":"zh-CN","text":"取消"}]}"#)
                .unwrap();
        let summary = storage
            .import_cards(deck.id, &initial, DuplicateStrategy::Merge)
            .unwrap();
        assert_eq!(summary.inserted, 1);

        let duplicate = import_jsonl_str(
            r#"{"term":"cancel","meanings":[{"lang":"zh-CN","text":"撤销"}],"tags":["review"]}"#,
        )
        .unwrap();
        let skipped = storage
            .import_cards(deck.id, &duplicate, DuplicateStrategy::Skip)
            .unwrap();
        assert_eq!(skipped.skipped, 1);
        assert_eq!(
            storage
                .list_cards_by_deck_paginated(deck.id, 100, 0)
                .unwrap()
                .len(),
            1
        );

        let merged = storage
            .import_cards(deck.id, &duplicate, DuplicateStrategy::Merge)
            .unwrap();
        assert_eq!(merged.merged, 1);
        let cards = storage
            .list_cards_by_deck_paginated(deck.id, 100, 0)
            .unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].meanings.len(), 2);
        assert!(cards[0].tags.iter().any(|tag| tag == "review"));
    }

    #[test]
    fn duplicate_overwrite_and_keep_work_in_storage() {
        let storage = open_temp();
        let deck = storage.insert_deck("cet4", Some("CET-4")).unwrap();
        let initial =
            import_jsonl_str(r#"{"term":"cancel","meanings":[{"lang":"zh-CN","text":"取消"}]}"#)
                .unwrap();
        storage
            .import_cards(deck.id, &initial, DuplicateStrategy::Merge)
            .unwrap();

        let replacement =
            import_jsonl_str(r#"{"term":"cancel","meanings":[{"lang":"zh-CN","text":"撤销"}]}"#)
                .unwrap();
        let overwritten = storage
            .import_cards(deck.id, &replacement, DuplicateStrategy::Overwrite)
            .unwrap();
        assert_eq!(overwritten.updated, 1);
        assert_eq!(
            storage
                .list_cards_by_deck_paginated(deck.id, 100, 0)
                .unwrap()[0]
                .meanings[0]
                .definition,
            "撤销"
        );

        let kept = storage
            .import_cards(deck.id, &replacement, DuplicateStrategy::Keep)
            .unwrap();
        assert_eq!(kept.inserted, 1);
        assert_eq!(
            storage
                .list_cards_by_deck_paginated(deck.id, 100, 0)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn jsonl_example_field_is_parsed() {
        let jsonl = r#"{"term":"absorb","meanings":[{"lang":"v","text":"吸收","example":"Plants absorb nutrients from the soil."}]}"#;
        let cards = import_jsonl_str(jsonl).unwrap();
        assert_eq!(cards.len(), 1);
        let meaning = &cards[0].meanings[0];
        assert_eq!(meaning.definition, "吸收");
        assert_eq!(
            meaning.example.as_deref(),
            Some("Plants absorb nutrients from the soil.")
        );
    }

    #[test]
    fn jsonl_missing_example_is_none() {
        let jsonl = r#"{"term":"cancel","meanings":[{"lang":"zh-CN","text":"取消"}]}"#;
        let cards = import_jsonl_str(jsonl).unwrap();
        assert!(cards[0].meanings[0].example.is_none());
    }

    #[test]
    fn jsonl_multiple_meanings_example_per_meaning() {
        let jsonl = r#"{"term":"access","meanings":[{"lang":"n","text":"入口","example":"The only access is across the bridge."},{"lang":"vt","text":"访问"}]}"#;
        let cards = import_jsonl_str(jsonl).unwrap();
        let meanings = &cards[0].meanings;
        assert_eq!(
            meanings[0].example.as_deref(),
            Some("The only access is across the bridge.")
        );
        assert!(meanings[1].example.is_none());
    }

    #[test]
    fn example_persisted_and_retrieved_from_storage() {
        let storage = open_temp();
        let db_deck = storage.insert_deck("test", None).unwrap();
        let jsonl = r#"{"term":"absorb","meanings":[{"lang":"v","text":"吸收","example":"Plants absorb nutrients from the soil."}]}"#;
        let parsed = import_jsonl_str(jsonl).unwrap();
        storage
            .import_cards(db_deck.id, &parsed, DuplicateStrategy::Merge)
            .unwrap();

        let cards = storage
            .list_cards_by_deck_paginated(db_deck.id, 100, 0)
            .unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(
            cards[0].meanings[0].example.as_deref(),
            Some("Plants absorb nutrients from the soil.")
        );
    }
}
