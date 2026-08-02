import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import type { OverlayHandle } from "@earendil-works/pi-tui";
import { seedDefaultDecks } from "./defaultDecks.ts";
import { getErrorCode, isErrorResponse, parseCardResponse, runFishword } from "./fishword.ts";
import { showCardOverlay, showDoneOverlay } from "./overlays/card.ts";
import { showCardDetailOverlay } from "./overlays/cardDetail.ts";
import { showDeckManagerOverlay } from "./overlays/deckManager.ts";
import { showStatsOverlay } from "./overlays/stats.ts";
import { OverlayManager } from "./overlayManager.ts";
import { attachPrefixShortcut } from "./prefixShortcut.ts";
import type { CardResponse, DeckItem, Rating, StatsResponse, StatusResponse } from "./types.ts";
import { RATINGS } from "./types.ts";
import { formatStatusLine, formatStatusLineMessage } from "./ui/statusLine.ts";

const SHORTCUT_MIGRATION_ISSUE = "https://github.com/Chenggou1/fishword/issues/19";

type FishwordAction = {
  command: string;
  description: string;
  handler: (ctx: ExtensionContext) => Promise<void> | void;
};

type OverlayState =
  | { kind: "none" }
  | { kind: "card"; handle: OverlayHandle; response: CardResponse }
  | { kind: "done"; handle: OverlayHandle; timer: ReturnType<typeof setInterval> }
  | { kind: "card-detail"; handle: OverlayHandle; response: CardResponse | null }
  | { kind: "stats"; handle: OverlayHandle }
  | { kind: "deck-manager"; handle: OverlayHandle };

export default function (pi: ExtensionAPI) {
  const overlayManager = new OverlayManager();
  let overlay: OverlayState = { kind: "none" };
  let isFishwordHidden = false;
  let lastStatusLine: string | undefined;
  let detachPrefixShortcut: (() => void) | undefined;

  function setFishwordStatus(ctx: ExtensionContext, text: string | undefined): void {
    lastStatusLine = text;
    ctx.ui.setStatus("fishword", isFishwordHidden ? undefined : text);
  }

  function applyFishwordHidden(ctx: ExtensionContext): void {
    overlayManager.setAllHidden(isFishwordHidden);
    ctx.ui.setStatus("fishword", isFishwordHidden ? undefined : lastStatusLine);
  }

  async function toggleFishwordVisibility(ctx: ExtensionContext): Promise<void> {
    isFishwordHidden = !isFishwordHidden;
    applyFishwordHidden(ctx);

    if (!isFishwordHidden && !overlayManager.hasAny()) {
      await refreshDisplay(ctx);
    }
  }

  /**
   * Close the current overlay. Pass hide=false when the UI framework has already
   * dismissed the overlay (e.g. from an onClose callback) to avoid a redundant hide call.
   */
  function teardown(hide: boolean = true): void {
    if (overlay.kind === "none") return;
    overlayManager.unregister(overlay.handle);
    if (hide) overlay.handle.hide();
    if (overlay.kind === "done") clearInterval(overlay.timer);
    overlay = { kind: "none" };
  }

  function showCurrentCard(ctx: ExtensionContext, cardResponse: Record<string, unknown>): void {
    teardown();
    const parsed = parseCardResponse(cardResponse);
    showCardOverlay(ctx, parsed, (handle) => {
      overlay = { kind: "card", handle, response: parsed };
      overlayManager.register(handle, isFishwordHidden);
    });
  }

  function showDone(ctx: ExtensionContext): void {
    teardown();
    const timer = setInterval(() => {
      void (async () => {
        const status = await refreshStatusLine(ctx);
        if (status && status.mode !== "complete") {
          await refreshDisplay(ctx);
        }
      })();
    }, 60_000);
    showDoneOverlay(ctx, (handle) => {
      overlay = { kind: "done", handle, timer };
      overlayManager.register(handle, isFishwordHidden);
    });
  }

  async function refreshStatusLine(ctx: ExtensionContext): Promise<StatusResponse | null> {
    try {
      const res = await runFishword(["status", "--json"]);
      if (isErrorResponse(res)) {
        const code = getErrorCode(res);
        setFishwordStatus(
          ctx,
          code === "no_active_deck" || code === "no_cards"
            ? formatStatusLineMessage("no-deck")
            : formatStatusLineMessage("unavailable"),
        );
        return null;
      }
      if (res["schema"] !== "fishword.protocol.status.v1") {
        setFishwordStatus(ctx, formatStatusLineMessage("unavailable"));
        return null;
      }
      const status = res as StatusResponse;
      setFishwordStatus(ctx, formatStatusLine(status));
      return status;
    } catch {
      setFishwordStatus(ctx, formatStatusLineMessage("unavailable"));
      return null;
    }
  }

  async function refreshDisplay(ctx: ExtensionContext): Promise<void> {
    const status = await refreshStatusLine(ctx);
    if (status?.mode === "complete") {
      showDone(ctx);
      return;
    }
    try {
      const res = await runFishword(["current", "--json"]);
      if (isErrorResponse(res)) {
        teardown();
      } else {
        showCurrentCard(ctx, res);
      }
    } catch {
      teardown();
    }
  }

  async function rateAndAdvance(ctx: ExtensionContext, rating: Rating): Promise<void> {
    if (isFishwordHidden) return;
    if (overlay.kind === "done") return;
    try {
      const res = await runFishword(["rate", rating, "--json"]);
      if (isErrorResponse(res)) {
        teardown();
        await refreshStatusLine(ctx);
      } else {
        const latestStatus = await refreshStatusLine(ctx);
        const next = res["next"] as Record<string, unknown> | null;
        if (next) {
          showCurrentCard(ctx, next);
        } else if (latestStatus?.mode === "complete") {
          showDone(ctx);
        } else {
          teardown();
        }
      }
    } catch {
      teardown();
      setFishwordStatus(ctx, formatStatusLineMessage("unavailable"));
    }
  }

  async function openStatsOverlay(ctx: ExtensionContext): Promise<void> {
    let statusRes: Record<string, unknown>;
    let statsRes: Record<string, unknown>;
    try {
      [statusRes, statsRes] = await Promise.all([
        runFishword(["status", "--json"]),
        runFishword(["stats", "--range", "7d", "--json"]),
      ]);
    } catch {
      ctx.ui.notify("无法读取 Fishword 学习统计", "error");
      return;
    }

    if (isErrorResponse(statusRes) || isErrorResponse(statsRes)) {
      const code = getErrorCode(isErrorResponse(statusRes) ? statusRes : statsRes);
      ctx.ui.notify(code === "no_active_deck" ? "请先选择词库" : "暂无可展示的学习统计", "info");
      return;
    }
    if (statsRes["schema"] !== "fishword.protocol.stats.v1" || statusRes["schema"] !== "fishword.protocol.status.v1") {
      ctx.ui.notify("Fishword 统计协议不匹配", "error");
      return;
    }

    teardown();
    showStatsOverlay(ctx, {
      status: statusRes as StatusResponse,
      stats: statsRes as StatsResponse,
      onHandle: (handle) => {
        overlay = { kind: "stats", handle };
        overlayManager.register(handle, isFishwordHidden);
      },
      onDone: () => {
        // Stats overlay Promise resolved; UI already dismissed — only unregister.
        teardown(false);
      },
      onRefresh: () => {
        void openStatsOverlay(ctx);
      },
      onClose: () => {
        void refreshDisplay(ctx);
      },
    });
  }

  function openDeckManager(ctx: ExtensionContext): void {
    teardown();

    showDeckManagerOverlay(ctx, {
      onHandle: (handle) => {
        overlay = { kind: "deck-manager", handle };
        overlayManager.register(handle, isFishwordHidden);
      },
      onClose: () => {
        teardown(false);
        void refreshDisplay(ctx);
      },
      onDeckChanged: () => {
        void refreshStatusLine(ctx);
      },
    });
  }

  function openCardDetail(ctx: ExtensionContext, responseOverride?: CardResponse | null): void {
    const response =
      responseOverride !== undefined
        ? responseOverride
        : overlay.kind === "card"
          ? overlay.response
          : null;

    teardown();

    showCardDetailOverlay(ctx, {
      response,
      onHandle: (handle) => {
        overlay = { kind: "card-detail", handle, response };
        overlayManager.register(handle, isFishwordHidden);
      },
      onClose: () => {
        // UI dismissed — only unregister, then restore card overlay if we have a response.
        teardown(false);
        if (response) {
          showCardOverlay(ctx, response, (handle) => {
            overlay = { kind: "card", handle, response: response };
            overlayManager.register(handle, isFishwordHidden);
          });
        }
      },
      onRate: (rating) => {
        void rateInDetail(ctx, rating);
      },
    });
  }

  async function rateInDetail(ctx: ExtensionContext, rating: Rating): Promise<void> {
    if (isFishwordHidden) return;
    if (overlay.kind !== "card-detail") return;
    // onRate fires after the detail overlay's Promise resolves (UI already dismissed).
    // teardown(false) unregisters the handle without calling hide() again.
    teardown(false);
    try {
      const res = await runFishword(["rate", rating, "--json"]);
      if (isErrorResponse(res)) {
        await refreshStatusLine(ctx);
        openCardDetail(ctx, null);
        return;
      }
      await refreshStatusLine(ctx);
      const next = res["next"] as Record<string, unknown> | null;
      const nextResponse = next ? parseCardResponse(next) : null;
      openCardDetail(ctx, nextResponse);
    } catch {
      setFishwordStatus(ctx, formatStatusLineMessage("unavailable"));
    }
  }

  const fishwordActions: FishwordAction[] = [
    {
      command: "fw-manage",
      description: "Manage decks — browse catalog or delete local decks",
      handler: openDeckManager,
    },
    {
      command: "fw-stats",
      description: "Show learning stats overlay",
      handler: openStatsOverlay,
    },
    {
      command: "fw",
      description: "Hide or summon review UI",
      handler: toggleFishwordVisibility,
    },
    ...RATINGS.map((rating): FishwordAction => ({
      command: `fw-${rating}`,
      description: `Rate ${rating} → next card`,
      handler: (ctx) => rateAndAdvance(ctx, rating),
    })),
    {
      command: "fw-detail",
      description: "Show detailed card info (phonetics, meanings, examples)",
      handler: openCardDetail,
    },
  ];

  const fishwordActionByCommand = new Map(fishwordActions.map((action) => [action.command, action]));

  function registerPrefixShortcut(ctx: ExtensionContext): void {
    detachPrefixShortcut?.();
    detachPrefixShortcut = undefined;
    if (ctx.mode !== "tui") return;

    detachPrefixShortcut = attachPrefixShortcut(
      ctx.ui,
      (command) => {
        const action = fishwordActionByCommand.get(command);
        if (!action) return;
        void (async () => {
          try {
            await action.handler(ctx);
          } catch {
            ctx.ui.notify("Fishword 快捷键执行失败", "error");
          }
        })();
      },
      { isFishwordHidden: () => isFishwordHidden },
    );
  }

  pi.on("session_start", async (event, ctx) => {
    registerPrefixShortcut(ctx);
    if (event.reason === "startup" && ctx.hasUI) {
      ctx.ui.notify(
        [
          "Fishword 快捷键已更新",
          "先按并松开 Ctrl+Q：F 隐藏/唤起 · I 详情 · A/H/G/E 评分",
          "原 Ctrl+Shift 系列快捷键已移除。",
          `详情：${SHORTCUT_MIGRATION_ISSUE}`,
        ].join("\n"),
        "warning",
      );
    }
    await seedDefaultDecks(ctx);
    await refreshDisplay(ctx);
  });

  pi.on("session_shutdown", () => {
    detachPrefixShortcut?.();
    detachPrefixShortcut = undefined;
  });

  for (const action of fishwordActions) {
    pi.registerCommand(action.command, {
      description: action.description,
      handler: async (_args, ctx) => {
        await action.handler(ctx);
      },
    });
  }
}
