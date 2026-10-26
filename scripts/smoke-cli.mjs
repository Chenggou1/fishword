import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { fishwordPath } from "../packages/cli/index.js";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const home = mkdtempSync(join(tmpdir(), "fishword-smoke-"));
const env = { ...process.env, HOME: home };

function run(args, options = {}) {
  const result = spawnSync(fishwordPath, args, {
    cwd: repoRoot,
    env,
    encoding: "utf8"
  });

  if (result.status !== 0) {
    throw new Error(
      [
        `fishword ${args.join(" ")} failed with exit code ${result.status}`,
        result.stdout,
        result.stderr
      ]
        .filter(Boolean)
        .join("\n")
    );
  }

  if (options.json) {
    return JSON.parse(result.stdout);
  }

  return result.stdout;
}

try {
  run(["init"]);

  run([
    "import",
    "jsonl",
    "crates/fishword-core/fixtures/deck_v1_sample.jsonl",
    "--create-deck",
    "smoke",
  ]);
  const decks = run(["deck", "list", "--json"], { json: true });
  const importedDeck = decks.decks?.find((deck) => deck.name === "smoke");
  if (!importedDeck?.id) {
    throw new Error(`import --create-deck did not create the smoke deck: ${JSON.stringify(decks)}`);
  }
  const deckId = importedDeck.id;

  const activeDeck = run(["deck", "current"]);
  if (!activeDeck.includes("smoke")) {
    throw new Error("deck current did not show the imported smoke deck");
  }

  // deck use now takes id
  run(["deck", "use", String(deckId)]);

  const defaultLimit = run(["deck", "daily-limit", "--json"], { json: true });
  if (defaultLimit.daily_new_limit !== 20) {
    throw new Error(`unexpected default daily limit: ${JSON.stringify(defaultLimit)}`);
  }
  const raisedLimit = run(["deck", "daily-limit", "50", "--json"], { json: true });
  if (raisedLimit.daily_new_limit !== 50) {
    throw new Error(`deck daily-limit did not persist: ${JSON.stringify(raisedLimit)}`);
  }

  const current = run(["current", "--json"], { json: true });
  if (!current.card?.term) {
    throw new Error("current --json did not return a card term");
  }

  const rated = run(["rate", "good", "--deck", String(deckId), "--json"], { json: true });
  if (rated.review?.rating !== "good") {
    throw new Error("rate good --json did not return a good review");
  }
  if (!("next" in rated)) {
    throw new Error("rate --json did not include a next field");
  }

  // deck rename
  const renamed = run(["deck", "rename", String(deckId), "smoke-renamed", "--json"], { json: true });
  if (renamed.deck?.name !== "smoke-renamed") {
    throw new Error(`deck rename failed: ${JSON.stringify(renamed)}`);
  }
  if (renamed.deck?.id !== deckId) {
    throw new Error("deck rename changed the id");
  }

  // deck delete
  const deleted = run(["deck", "delete", String(deckId), "--json"], { json: true });
  if (deleted.deleted?.id !== deckId) {
    throw new Error(`deck delete returned unexpected id: ${JSON.stringify(deleted)}`);
  }

  // verify deck is gone
  const listAfter = run(["deck", "list", "--json"], { json: true });
  if (listAfter.decks.some((d) => d.id === deckId)) {
    throw new Error("deleted deck still appears in deck list");
  }

  console.log(`smoke:rust ok (${current.card.term})`);
} finally {
  rmSync(home, { recursive: true, force: true });
}
