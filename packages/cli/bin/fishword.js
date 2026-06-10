#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { fishwordEnv, fishwordPath } from "../index.js";

const result = spawnSync(fishwordPath, process.argv.slice(2), {
  env: fishwordEnv(),
  stdio: "inherit"
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

if (typeof result.status === "number") {
  process.exit(result.status);
}

process.exit(1);
