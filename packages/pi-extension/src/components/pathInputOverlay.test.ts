import { homedir } from "node:os";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { TUI } from "@earendil-works/pi-tui";
import { describe, expect, it, vi } from "vitest";
import { PathInputOverlay, normalizePathInput } from "./pathInputOverlay.ts";

describe("PathInputOverlay", () => {
  it("normalizes a quoted home-relative path before passing it to the CLI", () => {
    expect(normalizePathInput('  "~/Documents/my words.jsonl"  ')).toBe(
      `${homedir()}/Documents/my words.jsonl`,
    );
  });

  it("shows directories and matching JSONL files while hiding other files", () => {
    const root = mkdtempSync(join(tmpdir(), "fishword-path-input-"));
    try {
      mkdirSync(join(root, "nested"));
      writeFileSync(join(root, "words.jsonl"), "{}");
      writeFileSync(join(root, "notes.txt"), "ignore");
      const overlay = new PathInputOverlay(
        { fg: (_style, text) => text },
        { requestRender() {} } as unknown as TUI,
        () => {},
        {
          title: "导入自建词库",
          fileExtensions: [".jsonl"],
          directoryLabel: "目录",
          fileLabel: "JSONL",
        },
        { path: `${root}/`, selectedIndex: 0 },
      );

      const output = overlay.render(80).join("\n");

      expect(output).toContain("[目录] nested");
      expect(output).toContain("[JSONL] words.jsonl");
      expect(output).not.toContain("notes.txt");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("keeps an unsupported manually entered file in the picker and explains the error", () => {
    const root = mkdtempSync(join(tmpdir(), "fishword-path-input-"));
    try {
      const textPath = join(root, "notes.txt");
      writeFileSync(textPath, "ignore");
      const done = vi.fn();
      const overlay = new PathInputOverlay(
        { fg: (_style, text) => text },
        { requestRender() {} } as unknown as TUI,
        done,
        { title: "导入自建词库", fileExtensions: [".jsonl"] },
        { path: textPath, selectedIndex: 0 },
      );

      overlay.handleInput("\r");

      expect(done).not.toHaveBeenCalled();
      expect(overlay.render(80).join("\n")).toContain("仅支持 .jsonl 文件");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
