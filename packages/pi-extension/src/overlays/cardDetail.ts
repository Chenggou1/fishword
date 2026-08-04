import type { ExtensionContext } from "@earendil-works/pi-coding-agent";
import type { OverlayHandle } from "@earendil-works/pi-tui";
import { Key, matchesKey, visibleWidth, wrapTextWithAnsi } from "@earendil-works/pi-tui";
import type { CardResponse, Rating } from "../types.ts";
import { fitCell } from "../ui/text.ts";
import { handleVisibilityShortcut, type VisibilityShortcutOptions } from "./visibilityShortcut.ts";

const PANEL_WIDTH = 62;
const DONE_MESSAGE = "今日词库已清空，给自己放个短假吧。";

export type CardDetailOptions = VisibilityShortcutOptions & {
  response: CardResponse | null;
  onHandle: (handle: OverlayHandle) => void;
  onClose: () => void;
  onRate: (rating: Rating) => void;
};

function formatBothPhonetics(card: CardResponse["card"]): string {
  const fmt = (s: string | null | undefined) => (s ? `/${s.replace(/^\/|\/$/g, "")}/` : "");
  const us = fmt(card.phonetic?.us);
  const uk = fmt(card.phonetic?.uk);
  if (us && uk && us !== uk) return `US ${us}   UK ${uk}`;
  if (us) return `US ${us}`;
  if (uk) return `UK ${uk}`;
  return "";
}

export function showCardDetailOverlay(ctx: ExtensionContext, options: CardDetailOptions): void {
  const { response, onHandle, onClose, onRate } = options;
  const card = response?.card ?? null;
  let overlayHandle: OverlayHandle | null = null;

  void ctx.ui.custom<"close" | Rating>(
    (_tui, theme, _kb, done) => ({
      render(width: number) {
        const w = Math.min(width, PANEL_WIDTH);
        const iw = w - 2;
        const row = (content: string) =>
          theme.fg("border", "│") + fitCell(content, iw) + theme.fg("border", "│");
        const wrappedRows = (
          content: string,
          firstPrefix = "",
          continuationPrefix = firstPrefix,
        ) => {
          const contentWidth = Math.max(
            1,
            iw - Math.max(visibleWidth(firstPrefix), visibleWidth(continuationPrefix)),
          );
          return wrapTextWithAnsi(content, contentWidth).map((line, index) =>
            row((index === 0 ? firstPrefix : continuationPrefix) + line),
          );
        };
        const separator = theme.fg("border", "├" + "─".repeat(iw) + "┤");
        const title = " DETAIL ";
        const leftDashes = Math.max(0, iw - visibleWidth(title) - 2);
        const topBorder =
          theme.fg("border", "╭" + "─".repeat(leftDashes)) +
          theme.fg("accent", title) +
          theme.fg("border", "──╮");

        const lines: string[] = [topBorder];

        if (!card) {
          lines.push(row(""));
          lines.push(row(theme.fg("dim", DONE_MESSAGE)));
          lines.push(row(""));
          lines.push(separator);
          lines.push(row(theme.fg("dim", "Esc 关闭")));
        } else {
          lines.push(...wrappedRows(theme.fg("accent", card.term)));
          const phonetics = formatBothPhonetics(card);
          if (phonetics) {
            lines.push(...wrappedRows(theme.fg("dim", phonetics)));
          }
          lines.push(separator);
          if (card.meanings.length === 0) {
            lines.push(row(theme.fg("dim", "(无释义)")));
          } else {
            for (const m of card.meanings) {
              if (typeof m === "string") {
                lines.push(...wrappedRows(m));
              } else {
                const pos = m.part_of_speech
                  ? theme.fg("accent", m.part_of_speech.padEnd(4)) + " "
                  : "     ";
                lines.push(...wrappedRows(m.definition, pos, " ".repeat(visibleWidth(pos))));
                if (m.example) {
                  lines.push(...wrappedRows(theme.fg("dim", m.example), "     "));
                }
              }
            }
          }
          lines.push(separator);
          lines.push(
            row(theme.fg("dim", "[A]gain  [H]ard  [G]ood  [E]asy    Esc 关闭")),
          );
        }

        lines.push(theme.fg("border", "╰" + "─".repeat(iw) + "╯"));
        return lines;
      },
      invalidate() {},
      handleInput(keyData: string) {
        if (handleVisibilityShortcut(keyData, options)) return;
        if (matchesKey(keyData, Key.escape)) {
          overlayHandle?.unfocus();
          done("close");
          return;
        }
        if (card) {
          const k = keyData.toLowerCase();
          if (k === "a") { done("again"); return; }
          if (k === "h") { done("hard"); return; }
          if (k === "g") { done("good"); return; }
          if (k === "e") { done("easy"); return; }
        }
      },
    }),
    {
      overlay: true,
      overlayOptions: { anchor: "center", width: PANEL_WIDTH, margin: 1 },
      onHandle: (handle) => {
        overlayHandle = handle;
        onHandle(handle);
      },
    },
  ).then((result) => {
    if (result === "close") {
      onClose();
    } else {
      onRate(result as Rating);
    }
  });
}
