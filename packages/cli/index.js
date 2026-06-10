import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const packageDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(packageDir, "../..");

const platformPackages = {
  "darwin-arm64": "@fishword/cli-darwin-arm64",
  "darwin-x64": "@fishword/cli-darwin-x64",
  "linux-arm64": "@fishword/cli-linux-arm64",
  "linux-x64": "@fishword/cli-linux-x64",
  "win32-x64": "@fishword/cli-win32-x64"
};

function executableName() {
  return process.platform === "win32" ? "fishword.exe" : "fishword";
}

function devBinaryPath() {
  return join(repoRoot, "target", "debug", executableName());
}

function platformPackageName() {
  return platformPackages[`${process.platform}-${process.arch}`];
}

function platformBinaryPath() {
  const packageName = platformPackageName();
  if (!packageName) {
    return undefined;
  }

  try {
    const packageJsonPath = require.resolve(`${packageName}/package.json`);
    return join(dirname(packageJsonPath), "bin", executableName());
  } catch {
    return undefined;
  }
}

function defaultDictDir() {
  const packagedPath = join(packageDir, "assets", "dicts", "qwerty-learner", "dicts");
  if (existsSync(packagedPath)) {
    return packagedPath;
  }

  const devPath = join(repoRoot, "assets", "dicts", "qwerty-learner", "dicts");
  if (existsSync(devPath)) {
    return devPath;
  }

  return undefined;
}

export function fishwordEnv(env = process.env) {
  const dictDir = defaultDictDir();
  if (!dictDir || env.FISHWORD_DEFAULT_DICT_DIR) {
    return env;
  }

  return {
    ...env,
    FISHWORD_DEFAULT_DICT_DIR: dictDir
  };
}

export function resolveFishwordPath() {
  if (process.env.FISHWORD_CLI_PATH) {
    return resolve(process.env.FISHWORD_CLI_PATH);
  }

  const devPath = devBinaryPath();
  if (existsSync(devPath)) {
    return devPath;
  }

  const packagedPath = platformBinaryPath();
  if (packagedPath && existsSync(packagedPath)) {
    return packagedPath;
  }

  const platformName = `${process.platform}-${process.arch}`;
  const packageName = platformPackageName();
  const expected = packageName ?? "no supported @fishword/cli-* package";
  throw new Error(
    [
      `Cannot find fishword CLI for ${platformName}.`,
      `Expected ${expected}.`,
      "For local development, run `pnpm dev:cli` from the repository root.",
      "You can also set FISHWORD_CLI_PATH to a custom fishword binary."
    ].join(" ")
  );
}

export const fishwordPath = resolveFishwordPath();
