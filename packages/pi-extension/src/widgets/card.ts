import type { ExtensionContext } from "@earendil-works/pi-coding-agent";
import { truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";
import type { CardResponse } from "../types.ts";
import { formatMeaning, formatPhonetic } from "../ui/text.ts";

const REVIEW_WIDGET_KEY = "fishword-review";
const IDLE_PRIMARY_HINT = "[ Ctrl+Q ]  按下开始";
const IDLE_SECONDARY_HINT = "            再按一个字母选择操作";
const PENDING_PRIMARY_HINT = "✓ Ctrl+Q  已按下 · 再按一个键";
const BASIC_ACTION_HINTS = "[F] 隐藏 · [I] 详情";
const ALL_ACTION_HINTS = `${BASIC_ACTION_HINTS} · [A] 重来 · [H] 困难 · [G] 记住 · [E] 简单`;
const ACTION_REVEAL_FIRST_MS = 70;
const ACTION_REVEAL_ALL_MS = 140;
const INTRO_HIGHLIGHT_START_MS = 250;
const INTRO_HIGHLIGHT_END_MS = 700;
const DONE_MESSAGE_CYCLE_MS = 5 * 60 * 1_000;
const DONE_MESSAGES = [
  "公司是老板的，身体是自己的，记得按时吃饭喔。",
  "恭喜你，在工位上偷偷变强了一点。",
  "这波不亏：工资照拿，单词照背。",
  "单词已清空，建议切回代码界面装作刚才在思考架构。",
  "知识已入库，疲惫请出栈。",
  "你在摸鱼，但鱼也在学习。",
  "今日已偷偷进步，建议保持神秘。",
  "你刚刚完成了一次合法的精神出逃。",
  "单词搞定，接下来请带薪呼吸三十秒。",
  "老板看不见你的努力，但单词记住了。",
];

function alignColumns(left: string, right: string, width: number): string {
  if (width <= 0) return "";
  const clippedRight = truncateToWidth(right, width);
  const rightWidth = visibleWidth(clippedRight);
  const leftWidth = Math.max(0, width - rightWidth - 1);
  const clippedLeft = truncateToWidth(left, leftWidth);
  const gap = " ".repeat(Math.max(1, width - visibleWidth(clippedLeft) - rightWidth));
  return truncateToWidth(clippedLeft + gap + clippedRight, width);
}

function createShortcutAnimation(
  tui: { requestRender(): void },
  prefixPending: boolean,
  animateIntroduction: boolean,
) {
  let introductionHighlighted = false;
  let actionRevealStage = prefixPending ? 0 : 2;
  const timers: ReturnType<typeof setTimeout>[] = [];

  const schedule = (delay: number, update: () => void) => {
    timers.push(
      setTimeout(() => {
        update();
        tui.requestRender();
      }, delay),
    );
  };

  if (prefixPending) {
    schedule(ACTION_REVEAL_FIRST_MS, () => {
      actionRevealStage = 1;
    });
    schedule(ACTION_REVEAL_ALL_MS, () => {
      actionRevealStage = 2;
    });
  } else if (animateIntroduction) {
    schedule(INTRO_HIGHLIGHT_START_MS, () => {
      introductionHighlighted = true;
    });
    schedule(INTRO_HIGHLIGHT_END_MS, () => {
      introductionHighlighted = false;
    });
  }

  return {
    isIntroductionHighlighted: () => introductionHighlighted,
    getActionRevealStage: () => actionRevealStage,
    dispose() {
      for (const timer of timers) clearTimeout(timer);
    },
  };
}

function pendingActionHints(stage: number, includeRatings: boolean): string {
  if (stage === 0) return "";
  if (stage === 1 || !includeRatings) return BASIC_ACTION_HINTS;
  return ALL_ACTION_HINTS;
}

export function showCardWidget(
  ctx: ExtensionContext,
  response: CardResponse,
  prefixPending = false,
  animateIntroduction = false,
): void {
  const { card } = response;

  ctx.ui.setWidget(
    REVIEW_WIDGET_KEY,
    (tui, theme) => {
      const animation = createShortcutAnimation(tui, prefixPending, animateIntroduction);

      return {
        render(width: number) {
          const phonetic = formatPhonetic(card);
          const term = theme.fg("accent", card.term);
          const termAndPhonetic = term + (phonetic ? "  " + theme.fg("dim", phonetic) : "");
          const meaning = formatMeaning(card);
          const primaryHint = prefixPending ? PENDING_PRIMARY_HINT : IDLE_PRIMARY_HINT;
          const secondaryHint = prefixPending
            ? pendingActionHints(animation.getActionRevealStage(), true)
            : IDLE_SECONDARY_HINT;
          const styledPrimaryHint = prefixPending || animation.isIntroductionHighlighted()
            ? theme.bold(theme.fg("accent", primaryHint))
            : theme.fg("dim", primaryHint);

          return [
            alignColumns(styledPrimaryHint, termAndPhonetic, width),
            alignColumns(theme.fg("dim", secondaryHint), meaning, width),
          ];
        },
        invalidate() {},
        dispose: animation.dispose,
      };
    },
    { placement: "aboveEditor" },
  );
}

export function clearReviewWidget(ctx: ExtensionContext): void {
  ctx.ui.setWidget(REVIEW_WIDGET_KEY, undefined);
}

export function showDoneWidget(
  ctx: ExtensionContext,
  prefixPending = false,
  animateIntroduction = false,
): void {
  ctx.ui.setWidget(
    REVIEW_WIDGET_KEY,
    (tui, theme) => {
      let messageIndex = Math.floor(Math.random() * DONE_MESSAGES.length);
      const animation = createShortcutAnimation(tui, prefixPending, animateIntroduction);
      const timer = setInterval(() => {
        messageIndex = (messageIndex + 1) % DONE_MESSAGES.length;
        tui.requestRender();
      }, DONE_MESSAGE_CYCLE_MS);

      return {
        render(width: number) {
          const primaryHint = prefixPending ? PENDING_PRIMARY_HINT : IDLE_PRIMARY_HINT;
          const styledPrimaryHint = prefixPending || animation.isIntroductionHighlighted()
            ? theme.bold(theme.fg("accent", primaryHint))
            : theme.fg("dim", primaryHint);
          const secondaryHint = prefixPending
            ? pendingActionHints(animation.getActionRevealStage(), false)
            : "今日复习完成";

          return [
            alignColumns(
              styledPrimaryHint,
              theme.fg("success", "DONE"),
              width,
            ),
            alignColumns(theme.fg("dim", secondaryHint), DONE_MESSAGES[messageIndex]!, width),
          ];
        },
        invalidate() {},
        dispose() {
          clearInterval(timer);
          animation.dispose();
        },
      };
    },
    { placement: "aboveEditor" },
  );
}
