import type { ExtensionContext } from "@earendil-works/pi-coding-agent";
import { TextInputOverlay } from "../components/textInputOverlay.ts";
import { getErrorMessage, isErrorResponse, runFishword } from "../fishword.ts";
import type { DailyLimitResponse } from "../types.ts";

/**
 * Validates a user-supplied daily new-card limit.
 *
 * Returns the parsed limit, or a human-readable reason when the input is not a
 * positive integer. Kept pure so the validation rules can be unit tested
 * without rendering a terminal overlay.
 */
export function parseDailyLimit(raw: string): { value: number } | { error: string } {
  const trimmed = raw.trim();
  if (!trimmed) return { error: "请输入每日新词上限" };
  if (!/^\d+$/.test(trimmed)) return { error: "每日新词上限必须是正整数" };
  const value = Number.parseInt(trimmed, 10);
  if (!Number.isSafeInteger(value) || value <= 0) {
    return { error: "每日新词上限必须是正整数" };
  }
  return { value };
}

export async function readDailyLimit(): Promise<DailyLimitResponse | undefined> {
  try {
    const res = await runFishword(["deck", "daily-limit", "--json"]);
    if (isErrorResponse(res)) return undefined;
    if (res["schema"] !== "fishword.protocol.daily_limit.v1") return undefined;
    return res as DailyLimitResponse;
  } catch {
    return undefined;
  }
}

/** Saves the limit and returns an error message when the CLI rejects it. */
export async function writeDailyLimit(value: number): Promise<string | undefined> {
  try {
    const res = await runFishword(["deck", "daily-limit", String(value), "--json"]);
    if (isErrorResponse(res)) {
      return getErrorMessage(res) ?? "保存每日新词上限失败";
    }
    return undefined;
  } catch (err) {
    return err instanceof Error ? err.message : "保存每日新词上限失败";
  }
}

export type DailyLimitPromptResult =
  | { kind: "changed"; limit: number }
  | { kind: "unchanged" }
  | { kind: "cancelled" }
  | { kind: "unsupported" };

export async function promptDailyLimit(
  ctx: ExtensionContext,
): Promise<DailyLimitPromptResult> {
  const current = await readDailyLimit();
  if (!current) {
    ctx.ui.notify(
      "当前 fishword CLI 不支持每日新词上限，请升级 CLI 后重试",
      "error",
    );
    return { kind: "unsupported" };
  }

  const entered = await ctx.ui.custom<string | undefined>(
    (tui, theme, _kb, done) =>
      new TextInputOverlay(theme, tui, done, {
        title: "每日新词上限",
        label: "新词数",
        initialValue: String(current.daily_new_limit),
        emptyMessage: "请输入每日新词上限",
        footer: "Enter 保存 · Esc 取消",
        body: (bodyTheme) => [
          bodyTheme.fg(
            "dim",
            `当前 ${current.daily_new_limit}  ·  默认 ${current.default_limit}`,
          ),
          bodyTheme.fg("dim", "每个词库独立配额，每天引入的新卡不超过该值"),
        ],
      }),
    {
      overlay: true,
      overlayOptions: { anchor: "center", width: 60, margin: 1 },
    },
  );

  if (entered === undefined) return { kind: "cancelled" };

  const parsed = parseDailyLimit(entered);
  if ("error" in parsed) {
    ctx.ui.notify(parsed.error, "error");
    return { kind: "unchanged" };
  }
  if (parsed.value === current.daily_new_limit) return { kind: "unchanged" };

  const failure = await writeDailyLimit(parsed.value);
  if (failure) {
    ctx.ui.notify(failure, "error");
    return { kind: "unchanged" };
  }
  return { kind: "changed", limit: parsed.value };
}
