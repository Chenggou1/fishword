import { cpSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const executableName = process.platform === "win32" ? "fishword.exe" : "fishword";
const fishwordPath = resolve(repoRoot, "target", "debug", executableName);
const fixture = fileURLToPath(
  new URL("../crates/fishword-core/fixtures/qwerty_cet4_sample.json", import.meta.url)
);
const defaultFiles = ["CET4_T.json", "CET6_T.json", "TOEFL_3_T.json"];

const tempRoot = mkdtempSync(join(tmpdir(), "fishword-release-smoke-"));
const packageRoot = join(tempRoot, "node_modules", "@fishword", "cli");
const home = join(tempRoot, "home");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: tempRoot,
    env: {
      ...process.env,
      FISHWORD_CLI_PATH: fishwordPath,
      HOME: home,
      USERPROFILE: home
    },
    encoding: "utf8"
  });

  if (result.status !== 0) {
    throw new Error(
      [
        `${command} ${args.join(" ")} failed with exit code ${result.status}`,
        result.stdout,
        result.stderr
      ]
        .filter(Boolean)
        .join("\n")
    );
  }

  if (options.includes && !result.stdout.includes(options.includes)) {
    throw new Error(
      `${command} ${args.join(" ")} did not include expected output: ${options.includes}\n${result.stdout}`
    );
  }

  return result.stdout;
}

try {
  mkdirSync(join(packageRoot, "bin"), { recursive: true });
  mkdirSync(join(packageRoot, "assets", "dicts", "qwerty-learner", "dicts"), {
    recursive: true
  });
  cpSync(join(repoRoot, "packages", "cli", "index.js"), join(packageRoot, "index.js"));
  cpSync(join(repoRoot, "packages", "cli", "bin", "fishword.js"), join(packageRoot, "bin", "fishword.js"));
  writeFileSync(
    join(packageRoot, "package.json"),
    JSON.stringify({ name: "@fishword/cli", type: "module" }, null, 2)
  );

  for (const fileName of defaultFiles) {
    cpSync(fixture, join(packageRoot, "assets", "dicts", "qwerty-learner", "dicts", fileName));
  }

  const bin = join(packageRoot, "bin", "fishword.js");
  const initOutput = run("node", [bin, "init"], { includes: "Default deck=cet4" });
  for (const deck of ["cet4", "cet6", "toefl"]) {
    if (!initOutput.includes(`Default deck=${deck}`)) {
      throw new Error(`init did not seed default deck ${deck}\n${initOutput}`);
    }
  }

  const deckList = run("node", [bin, "deck", "list"]);
  for (const deck of ["cet4", "cet6", "toefl"]) {
    if (!deckList.includes(deck)) {
      throw new Error(`deck list did not include ${deck}\n${deckList}`);
    }
  }

  run("node", [bin, "current", "--deck", "cet4", "--json"], { includes: "\"term\"" });
  console.log("release-smoke:cli ok");
} finally {
  rmSync(tempRoot, { recursive: true, force: true });
}
