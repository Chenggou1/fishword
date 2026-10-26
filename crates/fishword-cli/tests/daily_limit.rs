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

fn fishword(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fishword"))
        .env("HOME", home)
        .args(args)
        .output()
        .unwrap()
}

fn fishword_ok(home: &Path, args: &[&str]) -> serde_json::Value {
    let output = fishword(home, args);
    assert!(
        output.status.success(),
        "fishword {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn write_deck(home: &Path, words: usize) -> PathBuf {
    let path = home.join("words.jsonl");
    let mut body = String::new();
    for index in 0..words {
        body.push_str(&format!(
            "{{\"term\":\"word{index}\",\"meanings\":[{{\"lang\":\"zh-CN\",\"text\":\"释义{index}\"}}]}}\n"
        ));
    }
    fs::write(&path, body).unwrap();
    path
}

/// Rates the current card, selecting one first when none is set.
fn rate_ok(home: &Path, deck: &str) {
    let output = fishword(home, &["rate", "good", "--deck", deck, "--json"]);
    if output.status.success() {
        return;
    }
    let selected = fishword(home, &["current", "--deck", deck, "--json"]);
    assert!(
        selected.status.success(),
        "current --deck {deck} failed: {}",
        String::from_utf8_lossy(&selected.stderr)
    );
    let output = fishword(home, &["rate", "good", "--deck", deck, "--json"]);
    assert!(
        output.status.success(),
        "rate good --deck {deck} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Bootstraps a deck with `words` new cards and returns its numeric deck id.
fn seed_deck(home: &Path, words: usize) -> i64 {
    fishword(home, &["init"]);
    let jsonl = write_deck(home, words);
    let imported = fishword_ok(
        home,
        &[
            "import",
            "jsonl",
            jsonl.to_str().unwrap(),
            "--create-deck",
            "Limit",
            "--json",
        ],
    );
    imported["deck_id"].as_i64().unwrap()
}

#[test]
fn daily_limit_defaults_to_twenty() {
    let home = temp_home("daily-limit-default");

    let value = fishword_ok(&home, &["deck", "daily-limit", "--json"]);

    assert_eq!(value["schema"], "fishword.protocol.daily_limit.v1");
    assert_eq!(value["daily_new_limit"], 20);
    assert_eq!(value["default_limit"], 20);
}

#[test]
fn daily_limit_persists_across_invocations() {
    let home = temp_home("daily-limit-persist");

    let set = fishword_ok(&home, &["deck", "daily-limit", "50", "--json"]);
    assert_eq!(set["daily_new_limit"], 50);

    let read = fishword_ok(&home, &["deck", "daily-limit", "--json"]);
    assert_eq!(read["daily_new_limit"], 50);
    assert_eq!(read["default_limit"], 20);
}

#[test]
fn status_reports_the_configured_limit() {
    let home = temp_home("daily-limit-status");
    seed_deck(&home, 30);
    fishword(&home, &["deck", "daily-limit", "25"]);

    let status = fishword_ok(&home, &["status", "--json"]);

    // 30 new cards exist but only 25 fit in today's quota.
    assert_eq!(status["today"]["new_remaining"], 25);
}

#[test]
fn raising_the_limit_unlocks_more_new_cards_the_same_day() {
    let home = temp_home("daily-limit-raise");
    let deck_id = seed_deck(&home, 40);
    let deck = deck_id.to_string();

    fishword(&home, &["deck", "daily-limit", "20"]);
    for _ in 0..20 {
        rate_ok(&home, &deck);
    }
    // Quota for the day is used up under the default limit.
    let exhausted = fishword(&home, &["current", "--deck", &deck, "--json"]);
    assert!(!exhausted.status.success());

    fishword(&home, &["deck", "daily-limit", "40"]);

    let status = fishword_ok(&home, &["status", "--json"]);
    assert_eq!(status["today"]["reviewed"], 20);
    assert_eq!(status["today"]["new_remaining"], 20);

    let current = fishword(&home, &["current", "--deck", &deck, "--json"]);
    assert!(
        current.status.success(),
        "raising the limit should introduce new cards again: {}",
        String::from_utf8_lossy(&current.stderr)
    );
}

#[test]
fn lowering_the_limit_below_todays_count_reports_zero_remaining() {
    let home = temp_home("daily-limit-lower");
    let deck_id = seed_deck(&home, 10);
    let deck = deck_id.to_string();

    for _ in 0..5 {
        rate_ok(&home, &deck);
    }

    fishword(&home, &["deck", "daily-limit", "2"]);

    let status = fishword_ok(&home, &["status", "--json"]);
    assert_eq!(status["today"]["reviewed"], 5);
    assert_eq!(
        status["today"]["new_remaining"], 0,
        "a limit below today's introduced count must clamp at zero, not wrap"
    );
}

#[test]
fn non_positive_limit_returns_protocol_error_and_keeps_previous_value() {
    let home = temp_home("daily-limit-invalid");
    fishword(&home, &["deck", "daily-limit", "35"]);

    for invalid in ["0", "-3"] {
        let output = fishword(&home, &["deck", "daily-limit", invalid, "--json"]);
        assert!(!output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema"], "fishword.protocol.error.v1");
        assert_eq!(value["error"]["code"], "invalid_daily_limit");
    }

    let read = fishword_ok(&home, &["deck", "daily-limit", "--json"]);
    assert_eq!(read["daily_new_limit"], 35);
}
