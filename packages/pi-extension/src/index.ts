import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import type { OverlayHandle } from "@earendil-works/pi-tui";
import { PathInputOverlay, type PathInputOverlayResult } from "./components/pathInputOverlay.ts";
import { TextInputOverlay } from "./components/textInputOverlay.ts";
import { importAndActivateCustomDeck, suggestDeckName } from "./customDeckImport.ts";
import { seedDefaultDecks } from "./defaultDecks.ts";
import { getErrorCode, isErrorResponse, parseCardResponse, runFishword } from "./fishword.ts";
import { showCardDetailOverlay } from "./overlays/cardDetail.ts";
import { showDeckManagerOverlay } from "./overlays/deckManager.ts";
import { showStatsOverlay } from "./overlays/stats.ts";
import { OverlayManager } from "./overlayManager.ts";
import { attachPrefixShortcut } from "./prefixShortcut.ts";
import { createSystemSpeechPlayer } from "./speech.ts";
import type { CardResponse, DeckItem, Rating, StatsResponse, StatusResponse } from "./types.ts";
import { RATINGS } from "./types.ts";
import { formatStatusLine, formatStatusLineMessage } from "./ui/statusLine.ts";
import { clearReviewWidget, showCardWidget, showDoneWidget } from "./widgets/card.ts";

const SHORTCUT_MIGRATION_ISSUE = "https://github.com/Chenggou1/fishword/issues/19";

type FishwordAction = {
  command: string;
  description: string;
  handler: (ctx: ExtensionContext) => Promise<void> | void;
};

type OverlayState =
  | { kind: "none" }
  | { kind: "card-detail"; handle: OverlayHandle; response: CardResponse | null }
  | { kind: "stats"; handle: OverlayHandle }
  | { kind: "deck-manager"; handle: OverlayHandle }
  | { kind: "deck-import"; handle: OverlayHandle };

type ReviewState =
  | { kind: "none" }
  | { kind: "card"; response: CardResponse }
  | { kind: "done" };

export default function (pi: ExtensionAPI) {
  const overlayManager = new OverlayManager();
  const speech = createSystemSpeechPlayer();
  let overlay: OverlayState = { kind: "none" };
  let review: ReviewState = { kind: "none" };
  let doneRefreshTimer: ReturnType<typeof setInterval> | null = null;
  let isFishwordHidden = false;
  let isPrefixShortcutPending = false;
  let shouldAnimateShortcutIntroduction = true;
  let lastStatusLine: string | undefined;
  let detachPrefixShortcut: (() => void) | undefined;

  function setFishwordStatus(ctx: ExtensionContext, text: string | undefined): void {
    lastStatusLine = text;
    ctx.ui.setStatus("fishword", isFishwordHidden ? undefined : text);
  }

  function applyFishwordHidden(ctx: ExtensionContext): void {
    overlayManager.setAllHidden(isFishwordHidden);
    ctx.ui.setStatus("fishword", isFishwordHidden ? undefined : lastStatusLine);
    if (isFishwordHidden) {
      speech.stop();
      clearReviewWidget(ctx);
    }
  }

  function renderReview(ctx: ExtensionContext): void {
    if (isFishwordHidden || overlayManager.hasAny()) {
      clearReviewWidget(ctx);
      return;
    }
    const animateIntroduction = !isPrefixShortcutPending && shouldAnimateShortcutIntroduction;
    if (review.kind === "card") {
      showCardWidget(ctx, review.response, isPrefixShortcutPending, animateIntroduction);
      if (animateIntroduction) shouldAnimateShortcutIntroduction = false;
    } else if (review.kind === "done") {
      showDoneWidget(ctx, isPrefixShortcutPending, animateIntroduction);
      if (animateIntroduction) shouldAnimateShortcutIntroduction = false;
    } else {
      clearReviewWidget(ctx);
    }
  }

  async function toggleFishwordVisibility(ctx: ExtensionContext): Promise<void> {
    isFishwordHidden = !isFishwordHidden;
    applyFishwordHidden(ctx);

    if (isFishwordHidden) {
      ctx.ui.notify(
        "小声 bb：Fishword 已经藏好啦。想继续时，你可以先按并松开 Ctrl+Q，再按 F。",
        "info",
      );
    } else if (!overlayManager.hasAny()) {
      if (review.kind === "none") await refreshDisplay(ctx);
      else renderReview(ctx);
    }
  }

  function stopDoneRefreshTimer(): void {
    if (doneRefreshTimer) clearInterval(doneRefreshTimer);
    doneRefreshTimer = null;
  }

  function clearReview(ctx: ExtensionContext): void {
    speech.stop();
    stopDoneRefreshTimer();
    review = { kind: "none" };
    clearReviewWidget(ctx);
  }

  /**
   * Close the current overlay. Pass hide=false when the UI framework has already
   * dismissed the overlay (e.g. from an onClose callback) to avoid a redundant hide call.
   */
  function teardown(hide: boolean = true): void {
    if (overlay.kind === "none") return;
    overlayManager.unregister(overlay.handle);
    if (hide) overlay.handle.hide();
    overlay = { kind: "none" };
  }

  function showCurrentCard(ctx: ExtensionContext, cardResponse: Record<string, unknown>): void {
    speech.stop();
    teardown();
    stopDoneRefreshTimer();
    const parsed = parseCardResponse(cardResponse);
    review = { kind: "card", response: parsed };
    renderReview(ctx);
  }

  function showDone(ctx: ExtensionContext): void {
    speech.stop();
    teardown();
    stopDoneRefreshTimer();
    review = { kind: "done" };
    renderReview(ctx);
    doneRefreshTimer = setInterval(() => {
      void (async () => {
        const status = await refreshStatusLine(ctx);
        if (status && status.mode !== "complete") {
          await refreshDisplay(ctx);
        }
      })();
    }, 60_000);
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
        clearReview(ctx);
      } else {
        showCurrentCard(ctx, res);
      }
    } catch {
      teardown();
      clearReview(ctx);
    }
  }

  async function rateAndAdvance(ctx: ExtensionContext, rating: Rating): Promise<void> {
    if (isFishwordHidden) return;
    if (overlay.kind !== "none") return;
    if (review.kind === "done") return;
    speech.stop();
    try {
      const res = await runFishword(["rate", rating, "--json"]);
      if (isErrorResponse(res)) {
        teardown();
        clearReview(ctx);
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
          clearReview(ctx);
        }
      }
    } catch {
      teardown();
      clearReview(ctx);
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
    clearReviewWidget(ctx);
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
    clearReviewWidget(ctx);

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
        speech.stop();
        void refreshStatusLine(ctx);
      },
      onImportRequested: () => {
        teardown(false);
        void openCustomDeckImport(ctx);
      },
    });
  }

  async function chooseCustomDeckFile(
    ctx: ExtensionContext,
    initialPath?: string,
  ): Promise<string | undefined> {
    clearReviewWidget(ctx);
    const result = await ctx.ui.custom<PathInputOverlayResult>(
      (tui, theme, _kb, done) =>
        new PathInputOverlay(
          theme,
          tui,
          done,
          {
            title: "导入自建词库 · 1/2",
            label: "路径",
            emptyMessage: "没有匹配的目录或 JSONL 文件",
            footer: "Tab 补全 · ↑↓ 选择 · Enter 下一步 · Esc 取消",
            directoryLabel: "目录",
            fileLabel: "JSONL",
            fileExtensions: [".jsonl"],
            suggestionLimit: 6,
          },
          initialPath ? { path: initialPath, selectedIndex: 0 } : undefined,
        ),
      {
        overlay: true,
        overlayOptions: { anchor: "center", width: 72, margin: 1 },
        onHandle: (handle) => {
          overlay = { kind: "deck-import", handle };
          overlayManager.register(handle, isFishwordHidden);
        },
      },
    );
    teardown(false);
    return result;
  }

  async function confirmCustomDeckName(
    ctx: ExtensionContext,
    path: string,
  ): Promise<string | undefined> {
    const result = await ctx.ui.custom<string | undefined>(
      (tui, theme, _kb, done) =>
        new TextInputOverlay(theme, tui, done, {
          title: "导入自建词库 · 2/2",
          label: "名称",
          initialValue: suggestDeckName(path),
          emptyMessage: "词库名称不能为空",
          footer: "Enter 导入并切换 · Esc 返回选文件",
          body: (bodyTheme) => [
            bodyTheme.fg("dim", `文件  ${path}`),
            bodyTheme.fg("dim", "格式  fishword.deck.v1 JSONL"),
          ],
        }),
      {
        overlay: true,
        overlayOptions: { anchor: "center", width: 72, margin: 1 },
        onHandle: (handle) => {
          overlay = { kind: "deck-import", handle };
          overlayManager.register(handle, isFishwordHidden);
        },
      },
    );
    teardown(false);
    return result;
  }

  async function openCustomDeckImport(ctx: ExtensionContext, initialPath?: string): Promise<void> {
    let pathToRestore = initialPath;
    while (true) {
      const path = await chooseCustomDeckFile(ctx, pathToRestore);
      if (!path) {
        openDeckManager(ctx);
        return;
      }

      const name = await confirmCustomDeckName(ctx, path);
      if (!name) {
        pathToRestore = path;
        continue;
      }

      ctx.ui.notify(`正在导入词库“${name}”...`, "info");
      const result = await importAndActivateCustomDeck(path, name);
      if (!result.ok) {
        ctx.ui.notify(result.message, "error");
        openDeckManager(ctx);
        return;
      }

      ctx.ui.notify(`已导入并切换到“${result.deck}”，共 ${result.imported} 个单词`, "info");
      await refreshDisplay(ctx);
      return;
    }
  }

  function openCardDetail(ctx: ExtensionContext, responseOverride?: CardResponse | null): void {
    const response =
      responseOverride !== undefined
        ? responseOverride
        : review.kind === "card"
          ? review.response
          : null;

    teardown();
    clearReviewWidget(ctx);

    showCardDetailOverlay(ctx, {
      response,
      onHandle: (handle) => {
        overlay = { kind: "card-detail", handle, response };
        overlayManager.register(handle, isFishwordHidden);
      },
      onClose: () => {
        // UI dismissed — only unregister, then restore the current review state.
        teardown(false);
        renderReview(ctx);
      },
      onRate: (rating) => {
        void rateInDetail(ctx, rating);
      },
      onPronounce: () => {
        void pronounceCurrentCard(ctx);
      },
    });
  }

  async function rateInDetail(ctx: ExtensionContext, rating: Rating): Promise<void> {
    if (isFishwordHidden) return;
    if (overlay.kind !== "card-detail") return;
    speech.stop();
    // onRate fires after the detail overlay's Promise resolves (UI already dismissed).
    // teardown(false) unregisters the handle without calling hide() again.
    teardown(false);
    try {
      const res = await runFishword(["rate", rating, "--json"]);
      if (isErrorResponse(res)) {
        await refreshStatusLine(ctx);
        clearReview(ctx);
        openCardDetail(ctx, null);
        return;
      }
      const latestStatus = await refreshStatusLine(ctx);
      const next = res["next"] as Record<string, unknown> | null;
      const nextResponse = next ? parseCardResponse(next) : null;
      if (nextResponse) {
        review = { kind: "card", response: nextResponse };
      } else if (latestStatus?.mode === "complete") {
        showDone(ctx);
      } else {
        clearReview(ctx);
      }
      openCardDetail(ctx, nextResponse);
    } catch {
      setFishwordStatus(ctx, formatStatusLineMessage("unavailable"));
    }
  }

  async function pronounceCurrentCard(ctx: ExtensionContext): Promise<void> {
    if (isFishwordHidden) return;
    const response =
      overlay.kind === "card-detail" && overlay.response
        ? overlay.response
        : review.kind === "card"
          ? review.response
          : null;
    if (!response) {
      ctx.ui.notify("当前没有可朗读的单词", "info");
      return;
    }
    try {
      await speech.speak({
        text: response.card.term,
        language: response.card.language,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : "unknown error";
      ctx.ui.notify(`无法朗读当前单词: ${message}`, "error");
    }
  }

  const fishwordActions: FishwordAction[] = [
    {
      command: "fw-manage",
      description: "Manage decks — import, browse catalog, switch, or delete",
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
    {
      command: "fw-pronounce",
      description: "Pronounce the current word using a language-matched system voice",
      handler: pronounceCurrentCard,
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
      {
        onPendingChange(pending) {
          isPrefixShortcutPending = pending;
          renderReview(ctx);
        },
      },
    );
  }

  pi.on("session_start", async (event, ctx) => {
    registerPrefixShortcut(ctx);
    if (event.reason === "startup" && ctx.hasUI) {
      ctx.ui.notify(
        [
          "Fishword 快捷键已更新",
          "先按并松开 Ctrl+Q：F 隐藏/唤起 · I 详情 · P 朗读 · A/H/G/E 评分",
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
    speech.stop();
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
