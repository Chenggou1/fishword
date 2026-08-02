import { describe, expect, it, vi } from "vitest";
import { importAndActivateCustomDeck, suggestDeckName } from "./customDeckImport.ts";

describe("custom deck import", () => {
  it.each([
    ["/tmp/work-words.jsonl", "work-words"],
    ["C:\\Users\\me\\专业词汇.JSONL", "专业词汇"],
  ])("suggests a deck name from %s", (path, expected) => {
    expect(suggestDeckName(path)).toBe(expected);
  });

  it("creates a deck from JSONL and activates the imported deck", async () => {
    const run = vi.fn()
      .mockResolvedValueOnce({
        schema: "fishword.protocol.import.v1",
        deck_id: 42,
        deck: "工作词汇",
        input: 3,
        inserted: 3,
        updated: 0,
        merged: 0,
        skipped: 0,
      })
      .mockResolvedValueOnce({ schema: "fishword.protocol.deck_use.v1" });

    const result = await importAndActivateCustomDeck(
      "/tmp/work.jsonl",
      "工作词汇",
      run,
    );

    expect(run.mock.calls).toEqual([
      [["import", "jsonl", "/tmp/work.jsonl", "--create-deck", "工作词汇", "--json"]],
      [["deck", "use", "42", "--json"]],
    ]);
    expect(result).toEqual({
      ok: true,
      deckId: 42,
      deck: "工作词汇",
      imported: 3,
    });
  });

  it("explains when the requested deck name already exists", async () => {
    const run = vi.fn().mockResolvedValue({
      schema: "fishword.protocol.error.v1",
      error: { code: "deck_already_exists", message: "Deck already exists" },
    });

    const result = await importAndActivateCustomDeck("/tmp/work.jsonl", "工作词汇", run);

    expect(result).toEqual({ ok: false, message: "已存在名为“工作词汇”的词库，请换一个名称" });
    expect(run).toHaveBeenCalledOnce();
  });

  it("turns JSONL parse failures into an actionable message", async () => {
    const run = vi.fn().mockRejectedValue(
      new Error("failed to parse /tmp/work.jsonl: JSONL line 2: expected value"),
    );

    const result = await importAndActivateCustomDeck("/tmp/work.jsonl", "工作词汇", run);

    expect(result).toEqual({ ok: false, message: "JSONL 格式不正确，请检查第 2 行" });
  });

  it("explains when the JSONL file contains no importable cards", async () => {
    const run = vi.fn().mockResolvedValue({
      schema: "fishword.protocol.error.v1",
      error: { code: "empty_import_file", message: "No importable cards found" },
    });

    const result = await importAndActivateCustomDeck("/tmp/empty.jsonl", "空词库", run);

    expect(result).toEqual({ ok: false, message: "文件中没有可以导入的单词" });
  });
});
