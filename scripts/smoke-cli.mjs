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
  run(["init", "--skip-default-decks"]);
  run([
    "import",
    "qwerty",
    "crates/fishword-core/fixtures/qwerty_cet4_sample.json",
    "--deck",
    "smoke",
    "--name",
    "Smoke"
  ]);

  const activeDeck = run(["deck", "current"]);
  if (!activeDeck.includes("smoke")) {
    throw new Error("deck current did not show the imported smoke deck");
  }

  run(["deck", "use", "smoke"]);

  const current = run(["current", "--json"], { json: true });
  if (!current.card?.term) {
    throw new Error("current --json did not return a card term");
  }

  const next = run(["next", "--deck", "smoke", "--json"], { json: true });
  if (!next.card?.term) {
    throw new Error("next --json did not return a card term");
  }

  const rated = run(["rate", "good", "--deck", "smoke", "--json"], { json: true });
  if (rated.review?.rating !== "good") {
    throw new Error("rate good --json did not return a good review");
  }

  console.log(`smoke:cli ok (${current.card.term})`);
} finally {
  rmSync(home, { recursive: true, force: true });
}
