import { cpSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const source = fileURLToPath(
  new URL("../assets/dicts/qwerty-learner", import.meta.url)
);
const target = fileURLToPath(
  new URL("../packages/cli/assets/dicts/qwerty-learner", import.meta.url)
);
const requiredFiles = ["CET4_T.json", "CET6_T.json", "TOEFL_3_T.json"];

for (const fileName of requiredFiles) {
  const path = fileURLToPath(
    new URL(`../assets/dicts/qwerty-learner/dicts/${fileName}`, import.meta.url)
  );
  const text = readFileSync(path, "utf8");
  if (text.startsWith("version https://git-lfs.github.com/spec/v1")) {
    throw new Error(`${path} is a Git LFS pointer. Run git lfs pull before packing.`);
  }
}

rmSync(target, { recursive: true, force: true });
mkdirSync(fileURLToPath(new URL("../packages/cli/assets/dicts", import.meta.url)), {
  recursive: true
});
cpSync(source, target, {
  dereference: true,
  errorOnExist: false,
  force: true,
  recursive: true
});

console.log(`Copied default dictionaries from ${source} for ${repoRoot}`);
