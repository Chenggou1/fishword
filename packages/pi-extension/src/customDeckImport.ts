import {
  describeFishwordError,
  getErrorCode,
  getErrorMessage,
  isErrorResponse,
  runFishword,
} from "./fishword.ts";

type FishwordRunner = (args: string[]) => Promise<Record<string, unknown>>;

type ImportResponse = {
  schema: "fishword.protocol.import.v1";
  deck_id: number;
  deck: string;
  input: number;
  inserted: number;
  updated: number;
  merged: number;
  skipped: number;
};

export type CustomDeckImportResult =
  | { ok: true; deckId: number; deck: string; imported: number }
  | { ok: false; message: string };

export function suggestDeckName(path: string): string {
  const filename = path.split(/[\\/]/).pop() ?? "";
  return filename.replace(/\.jsonl$/i, "").trim() || "我的词库";
}

export async function importAndActivateCustomDeck(
  path: string,
  name: string,
  run: FishwordRunner = runFishword,
): Promise<CustomDeckImportResult> {
  try {
    const importResult = await run([
      "import",
      "jsonl",
      path,
      "--create-deck",
      name,
      "--json",
    ]);
    if (isErrorResponse(importResult)) {
      const code = getErrorCode(importResult);
      if (code === "deck_already_exists") {
        return { ok: false, message: `已存在名为“${name}”的词库，请换一个名称` };
      }
      if (code === "empty_import_file") {
        return { ok: false, message: "文件中没有可以导入的单词" };
      }
      return { ok: false, message: getErrorMessage(importResult) ?? code ?? "导入失败" };
    }
    if (importResult["schema"] !== "fishword.protocol.import.v1") {
      return { ok: false, message: "导入失败" };
    }

    const imported = importResult as ImportResponse;
    const activateResult = await run(["deck", "use", String(imported.deck_id), "--json"]);
    if (isErrorResponse(activateResult) || activateResult["schema"] !== "fishword.protocol.deck_use.v1") {
      return { ok: false, message: `词库“${imported.deck}”已导入，但无法切换` };
    }

    return {
      ok: true,
      deckId: imported.deck_id,
      deck: imported.deck,
      imported: imported.inserted + imported.updated + imported.merged,
    };
  } catch (error) {
    const detail = describeFishwordError(error);
    const line = /JSONL line (\d+)/i.exec(detail)?.[1];
    if (line) return { ok: false, message: `JSONL 格式不正确，请检查第 ${line} 行` };
    return { ok: false, message: `导入失败：${detail}` };
  }
}
