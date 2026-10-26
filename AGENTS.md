# Fishword — Agent Guide

For domain vocabulary and system overview, read `CONTEXT.md` first.
For architectural decisions, see `docs/adr/`.

## CLI Usage

The CLI is the public boundary of the system. Frontends use `--json`; human-readable output is for manual testing only (see ADR-0002).

Key details:
- `import` takes a numeric local deck id via `--deck-id`; create or list decks first.
- `import jsonl <path> --create-deck <name>` creates a new local deck and imports into it.
- `catalog fetch` takes a catalog id such as `kajweb:cet4`, not a local deck id.
- No standalone `next` command. Use `current` to show the current card; `rate` advances to the next.
- `deck daily-limit` reads or writes the per-deck daily new-card limit (default 20). The value lives in `settings` so it can change without a rebuild.
- Set `FISHWORD_CATALOG_URL` to override the catalog endpoint for offline testing.

## Directory Layout

```text
.
├── assets/dicts/
│   ├── kajweb/          ← converted deck.v1 JSONL files
│   └── qwerty-learner/  ← source dictionaries (Git LFS)
├── crates/
│   ├── fishword-core/
│   └── fishword-cli/
├── docs/
│   ├── adr/
│   └── agents/
├── migrations/
├── packages/
│   ├── cli/             ← npm wrapper
│   ├── cli-darwin-arm64/
│   ├── cli-darwin-x64/
│   ├── cli-linux-arm64/
│   ├── cli-linux-x64/
│   ├── cli-win32-x64/
│   └── pi-extension/
├── schemas/
├── site/                ← GitHub Pages product site
│   ├── index.html
│   ├── decks/
│   ├── images/
│   └── videos/
└── scripts/
```

## npm Packages and Pi Extension

The JavaScript workspace is managed by pnpm under `packages/`.

- `@fishword/cli` — JavaScript wrapper, exposes the `fishword` npm binary.
- `@fishword/cli-*` — platform-specific Rust binaries.
- `@fishword/pi-extension` — Pi extension package.

Release order: Rust binaries → platform CLI packages → `@fishword/cli` → Pi extension build → `@fishword/pi-extension`.

When adding a Pi overlay, track its `OverlayHandle` in `packages/pi-extension/src/index.ts` and include it in the Boss-key hide/summon state checks. A modal overlay (e.g. deck management) must prevent the review card overlay from reappearing on Boss-key restore.

## Dictionaries

### kajweb/dict

Deck.v1 JSONL files live under `assets/dicts/kajweb/`. To regenerate:

```bash
uv run scripts/kajweb_to_jsonl.py --book CET4 -o assets/dicts/kajweb/cet4.jsonl
```

### Qwerty Learner

Source files under `assets/dicts/qwerty-learner/dicts/` (Git LFS). Keep `SOURCE.md` and `upstream/LICENSE` in place.

### Online Catalog

Built by `scripts/convert-qwerty-decks.mjs`, output to `dist/catalog/` (git-ignored). Deployed to `gh-pages` by `.github/workflows/publish-catalog.yml`.

To regenerate locally:

```bash
node scripts/convert-qwerty-decks.mjs
```

### Git LFS

```
assets/dicts/qwerty-learner/dicts/*.json
assets/dicts/kajweb/*.jsonl
```

After changing LFS patterns for already-tracked files: `git add --renormalize <path>`

## Data Storage

Default DB path on macOS: `~/Library/Application Support/fishword/fishword.db`

For isolated manual tests: `HOME=/private/tmp/fishword-test ./target/debug/fishword init`

## Development Rules

- Keep `fishword-cli` thin; delegate domain work to `fishword-core`.
- Do not make Rust `init` aware of npm package paths; asset lookup belongs in the extension.
- `current` and `status` must not write review logs (see ADR-0003).
- Keep dictionary data tracked by Git LFS.

## Agent skills

### Issue tracker

Issues live in GitHub Issues, managed via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the default five-label vocabulary (needs-triage / needs-info / ready-for-agent / ready-for-human / wontfix). See `docs/agents/triage-labels.md`.

### Domain docs

Single-core + multi-extension layout: root `CONTEXT.md` covers the CLI protocol and core domain; each extension may have its own `CONTEXT.md` under `packages/<name>/`. See `docs/agents/domain.md`.

## Commit Message Format

Use `type: message` format. Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `ci`.

```text
refactor: refine catalog identifiers
fix: handle empty import files
docs: update catalog examples
```

Do not use `type!:` syntax unless explicitly requested.

## Verification Commands

```bash
pnpm check
pnpm format:check
pnpm lint
pnpm test
pnpm check:pi
pnpm smoke:rust
```
