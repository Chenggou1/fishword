use std::path::{Path, PathBuf};

use crate::{
    error::{Error, Result},
    importer::{import_qwerty_file, DuplicateStrategy, ImportSummary},
    storage::Storage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultDeck {
    pub id: &'static str,
    pub name: &'static str,
    pub file_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultDeckSeedSummary {
    pub deck: DefaultDeck,
    pub import: ImportSummary,
}

pub const DEFAULT_DECKS: &[DefaultDeck] = &[
    DefaultDeck {
        id: "cet4",
        name: "CET-4",
        file_name: "CET4_T.json",
    },
    DefaultDeck {
        id: "cet6",
        name: "CET-6",
        file_name: "CET6_T.json",
    },
    DefaultDeck {
        id: "toefl",
        name: "TOEFL",
        file_name: "TOEFL_3_T.json",
    },
];

pub fn seed_default_decks(
    storage: &Storage,
    dict_dir: &Path,
) -> Result<Vec<DefaultDeckSeedSummary>> {
    let mut summaries = Vec::new();

    for deck in DEFAULT_DECKS {
        let path = dict_dir.join(deck.file_name);
        reject_lfs_pointer(&path)?;
        let import_deck = import_qwerty_file(&path, deck.id, Some(deck.name))?;
        let summary = storage.import_cards(
            &import_deck.deck_id,
            import_deck.deck_name.as_deref(),
            &import_deck.cards,
            DuplicateStrategy::Skip,
        )?;
        summaries.push(DefaultDeckSeedSummary {
            deck: *deck,
            import: summary,
        });
    }

    if storage.get_active_deck_id()?.is_none() {
        if let Some(deck) = storage.get_deck_by_name(DEFAULT_DECKS[0].id)? {
            storage.set_active_deck_id(Some(deck.id))?;
        }
    }

    Ok(summaries)
}

pub fn default_dict_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os("FISHWORD_DEFAULT_DICT_DIR") {
        candidates.push(PathBuf::from(path));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            candidates.push(bin_dir.join("assets/dicts/qwerty-learner/dicts"));
            candidates.push(bin_dir.join("../assets/dicts/qwerty-learner/dicts"));
        }
    }

    candidates.push(PathBuf::from("assets/dicts/qwerty-learner/dicts"));
    candidates
}

pub fn find_default_dict_dir() -> Option<PathBuf> {
    default_dict_dir_candidates()
        .into_iter()
        .find(|path| path.join(DEFAULT_DECKS[0].file_name).is_file())
}

fn reject_lfs_pointer(path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    if text.starts_with("version https://git-lfs.github.com/spec/v1") {
        return Err(Error::InvalidInput(format!(
            "{} is a Git LFS pointer; fetch LFS contents before building or packaging defaults",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn seeds_default_decks_from_qwerty_files() {
        let dir = tempdir().unwrap();
        let dict_dir = dir.path().join("dicts");
        std::fs::create_dir(&dict_dir).unwrap();
        for deck in DEFAULT_DECKS {
            std::fs::write(
                dict_dir.join(deck.file_name),
                r#"[{"name":"cancel","trans":["取消，撤销"],"usphone":"'kænsl","ukphone":"'kænsl"}]"#,
            )
            .unwrap();
        }

        let storage = Storage::open(&dir.path().join("fishword.db")).unwrap();
        let summaries = seed_default_decks(&storage, &dict_dir).unwrap();

        assert_eq!(summaries.len(), DEFAULT_DECKS.len());
        assert!(summaries.iter().all(|summary| summary.import.inserted == 1));
        assert_eq!(storage.list_decks().unwrap().len(), DEFAULT_DECKS.len());
        assert_eq!(
            storage.get_active_deck().unwrap().unwrap().name,
            DEFAULT_DECKS[0].id
        );

        let summaries = seed_default_decks(&storage, &dict_dir).unwrap();
        assert!(summaries.iter().all(|summary| summary.import.skipped == 1));
    }
}
