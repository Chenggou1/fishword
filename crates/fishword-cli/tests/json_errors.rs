use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_home(test_name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "fishword-{test_name}-{}-{suffix}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_jsonl(home: &Path) -> PathBuf {
    let path = home.join("words.jsonl");
    fs::write(
        &path,
        r#"{"term":"cancel","meanings":[{"lang":"zh-CN","text":"取消"}]}"#,
    )
    .unwrap();
    path
}

fn fishword(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fishword"))
        .env("HOME", home)
        .args(args)
        .output()
        .unwrap()
}

fn assert_json_error(output: Output, code: &str, message: &str) {
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "JSON errors should be emitted on stdout only\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "fishword.protocol.error.v1");
    assert_eq!(value["error"]["code"], code);
    assert_eq!(value["error"]["message"], message);
}

fn assert_json_error_code(output: Output, code: &str) {
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "JSON errors should be emitted on stdout only\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "fishword.protocol.error.v1");
    assert_eq!(value["error"]["code"], code);
    assert!(
        value["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.trim().is_empty()),
        "error message should be present"
    );
}

#[test]
fn invalid_rating_with_json_returns_protocol_error() {
    let home = temp_home("json-invalid-rating");

    assert_json_error(
        fishword(&home, &["rate", "maybe", "--json"]),
        "invalid_rating",
        "invalid rating 'maybe', expected again/hard/good/easy",
    );
}

#[test]
fn invalid_duplicate_strategy_with_json_returns_protocol_error() {
    let home = temp_home("json-invalid-duplicates");
    let jsonl = write_jsonl(&home);

    assert_json_error(
        fishword(
            &home,
            &[
                "import",
                "jsonl",
                jsonl.to_str().unwrap(),
                "--create-deck",
                "Imported",
                "--duplicates",
                "nonsense",
                "--json",
            ],
        ),
        "invalid_duplicate_strategy",
        "invalid --duplicates value 'nonsense'",
    );
}

#[test]
fn empty_import_file_with_json_returns_protocol_error() {
    let home = temp_home("json-empty-import");
    let jsonl = home.join("empty.jsonl");
    fs::write(&jsonl, "\n").unwrap();

    assert_json_error(
        fishword(
            &home,
            &[
                "import",
                "jsonl",
                jsonl.to_str().unwrap(),
                "--create-deck",
                "Empty",
                "--json",
            ],
        ),
        "empty_import_file",
        "No importable cards found in the JSONL file",
    );
}

#[test]
fn deck_not_found_with_json_returns_protocol_error() {
    let home = temp_home("json-deck-not-found");

    assert_json_error(
        fishword(&home, &["deck", "use", "999", "--json"]),
        "deck_not_found",
        "Deck not found: 999",
    );
}

#[test]
fn missing_required_import_target_with_json_returns_protocol_error() {
    let home = temp_home("json-missing-import-target");
    let jsonl = write_jsonl(&home);

    assert_json_error_code(
        fishword(
            &home,
            &["import", "jsonl", jsonl.to_str().unwrap(), "--json"],
        ),
        "missing_required_argument",
    );
}

#[test]
fn unknown_argument_with_json_returns_protocol_error() {
    let home = temp_home("json-unknown-argument");

    assert_json_error_code(fishword(&home, &["--json"]), "unknown_argument");
}
