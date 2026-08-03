# Fishword Domain Context

## Core principle

Each deck runs FSRS independently. Review scheduling, card selection, and progress tracking are always scoped to a single deck. There is no cross-deck scheduling or selection.

## What the system is

Fishword is a local vocabulary learning tool centered on a Rust CLI. The CLI is the stable integration boundary for the Pi extension, npm packages, and future terminal/editor integrations.

Supported capabilities:
- SQLite-backed decks, cards, card state, settings, and review logs
- Importing `fishword.deck.v1` JSONL and Anki `.apkg` packages — both parse to the shared `ImportCard` IR before storage
- FSRS-based review scheduling per deck
- Deck-scoped card selection via `current`; `rate` records a review and returns the next card
- Stable JSON protocol output for frontend integrations
- Online catalog of pre-built decks hosted on GitHub Pages

## Glossary

**Deck** — an independent vocabulary set with its own FSRS state. A user may have multiple decks (e.g. CET-4, CET-6, TOEFL); their scheduling never interacts.

**Card** — a vocabulary item belonging to exactly one deck. Has its own FSRS state (`card_state` row).

**Review** — a single rating event (`again | hard | good | easy`) that advances a card's FSRS state and updates the current-card pointer within its deck. Only `rate` triggers a review; `current` and `status` are read-only.

**Current card** — the card a user is actively studying, scoped to a deck. Stored as `current_card_id` in the `settings` table.

**Selection** — the process of choosing which card to show next within a deck, based on FSRS due dates, retrievability, and the daily new-card limit.

**Daily new limit** — the maximum number of new (never-reviewed) cards introduced per deck per day. Default: 20.

**Catalog** — a remote index of pre-built decks downloadable via `fishword catalog fetch`. Fetched decks become independent local decks after import.

**JSON protocol** — the stable machine-readable CLI contract consumed by frontend integrations. A protocol response must not be mixed with human-readable output.

**deck.v1 JSONL** — the canonical runtime import format. Other sources (e.g. kajweb, Qwerty Learner) are converted to this format offline via scripts before import.

**`.apkg`** — Anki package, a second runtime import format. The parser unzips the package, opens the embedded anki2/anki21/anki21b SQLite, and maps each field to a role (term/phonetic/definition/example/pos/ignore) from content features + field position — **field names are unreliable** (a TOEFL deck's `pos` field actually stores IPA). Multiple Anki decks merge into one local deck (Anki deck name → tag). SM-2 scheduling is not migrated; imported cards start fresh (per ADR-0003). See `docs/apkg-import.md`.

## Core crates

### `crates/fishword-core`

Contains all domain logic. Keep it free of CLI concerns.

- `card` — card, meaning, pronunciation, review state, rating, and source models
- `deck` — deck model
- `storage` — SQLite persistence, migrations, settings, current-card state, review logs
- `importer` — `deck.v1` JSONL importer + Anki `.apkg` importer (`apkg/` submodule); both emit `ImportCard` IR consumed by storage
- `scheduler` — FSRS review scheduling
- `selector` — deck-scoped card selection policy
- `error` — shared core error and result types

### `crates/fishword-cli`

Contains the command-line interface. Keep it thin and delegate domain work to `fishword-core`.

- `args` — Clap command shape and CLI argument parsing
- `cmd` — command handlers grouped by CLI domain (`deck`, `card`, `catalog`, `import`, `review`, `rate`, `init`)
- `protocol` — stable JSON DTOs for frontend consumers
- `util` — shared CLI plumbing for storage opening, JSON/human output, and protocol errors

CLI command modules expose `pub fn cmd_*` handlers only at the `main.rs` dispatch seam. Inside a command module, private helpers should use local action names (e.g. `list`, `create`, `rename`) and rely on module locality rather than repeating the module name or `cmd_` prefix.

## Pi Extension domain behavior

The Pi extension seeds three default decks on session start: `CET-4`, `CET-6`, `TOEFL`. Seeding is driven from the extension (`packages/pi-extension/src/defaultDecks.ts`), not from Rust `init`, because the extension knows where its npm package assets are while the Rust CLI only receives local file paths.

The extension build copies the three default kajweb JSONL files from `assets/dicts/kajweb/` into `packages/pi-extension/assets/dicts/kajweb/` (git-ignored, included in the npm tarball).
