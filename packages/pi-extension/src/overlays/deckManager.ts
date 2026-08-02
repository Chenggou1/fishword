import type { ExtensionContext } from "@earendil-works/pi-coding-agent";
import { Theme } from "@earendil-works/pi-coding-agent";
import type { OverlayHandle } from "@earendil-works/pi-tui";
import { Key, matchesKey, truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";
import { describeFishwordError, getErrorCode, getErrorMessage, isErrorResponse, runFishword } from "../fishword.ts";
import type { CatalogDeckEntry, CatalogFetchResponse, CatalogListResponse, DeckDeleteResponse, DeckItem } from "../types.ts";
import { handleVisibilityShortcut, type VisibilityShortcutOptions } from "./visibilityShortcut.ts";

type Tab = "catalog" | "my-decks";

type CatalogState =
  | { kind: "loading" }
  | { kind: "error"; msg: string }
  | { kind: "ready"; decks: CatalogDeckEntry[]; selectedIndex: number; downloading: string | null };

type MyDecksState =
  | { kind: "loading" }
  | { kind: "ready"; decks: DeckItem[]; selectedIndex: number; confirmDelete: DeckItem | null };

export type DeckManagerOptions = VisibilityShortcutOptions & {
  onHandle: (handle: OverlayHandle) => void;
  onClose: () => void;
  onDeckChanged: () => void;
  onImportRequested: () => void;
};

type DeckManagerResult = "import" | undefined;

const OVERLAY_WIDTH = 72;
const MAX_LIST_ROWS = 8;

export function showDeckManagerOverlay(ctx: ExtensionContext, options: DeckManagerOptions): void {
  const { onClose, onDeckChanged, onHandle, onImportRequested } = options;

  let activeTab: Tab = "my-decks";
  let catalogState: CatalogState = { kind: "loading" };
  let myDecksState: MyDecksState = { kind: "loading" };
  let statusMsg: string | null = null;
  let statusTimer: ReturnType<typeof setTimeout> | null = null;
  let requestRender: (() => void) | null = null;

  const downloadedIds = new Set<string>();

  function setStatus(msg: string): void {
    statusMsg = msg;
    if (statusTimer) clearTimeout(statusTimer);
    statusTimer = setTimeout(() => {
      statusMsg = null;
      requestRender?.();
    }, 3000);
    requestRender?.();
  }

  async function loadCatalog(): Promise<void> {
    try {
      const res = await runFishword(["catalog", "list", "--json"]);
      if (isErrorResponse(res)) {
        catalogState = { kind: "error", msg: getErrorMessage(res) ?? getErrorCode(res) ?? "unknown error" };
      } else if (res["schema"] === "fishword.protocol.catalog_list.v1") {
        catalogState = {
          kind: "ready",
          decks: (res as unknown as CatalogListResponse).decks,
          selectedIndex: 0,
          downloading: null,
        };
      } else {
        catalogState = { kind: "error", msg: "protocol mismatch" };
      }
    } catch (err) {
      catalogState = { kind: "error", msg: describeFishwordError(err) };
    }
    requestRender?.();
  }

  async function loadMyDecks(): Promise<void> {
    try {
      const res = await runFishword(["deck", "list", "--json"]);
      if (res["schema"] === "fishword.protocol.decks.v1") {
        const decks = res["decks"] as DeckItem[];
        const prev = myDecksState.kind === "ready" ? myDecksState.selectedIndex : 0;
        myDecksState = {
          kind: "ready",
          decks,
          selectedIndex: Math.min(prev, Math.max(0, decks.length - 1)),
          confirmDelete: null,
        };
      } else {
        myDecksState = { kind: "ready", decks: [], selectedIndex: 0, confirmDelete: null };
      }
    } catch {
      myDecksState = { kind: "ready", decks: [], selectedIndex: 0, confirmDelete: null };
    }
    requestRender?.();
  }

  void loadCatalog();
  void loadMyDecks();

  function renderTabBar(iw: number, theme: Theme): string {
    const myDecksLabel = activeTab === "my-decks"
      ? theme.fg("accent", theme.bold("[ 我的词库 ]"))
      : theme.fg("dim", "  我的词库  ");
    const catalogLabel = activeTab === "catalog"
      ? theme.fg("accent", theme.bold("[ 词库目录 ]"))
      : theme.fg("dim", "  词库目录  ");
    const tabLine = " " + myDecksLabel + "  " + catalogLabel;
    return theme.fg("border", "│") + truncateToWidth(tabLine, iw, "...", true) + theme.fg("border", "│");
  }

  function renderCatalogRows(iw: number, theme: Theme): string[] {
    if (catalogState.kind === "loading") {
      return [theme.fg("dim", " 加载词库目录中...")];
    }
    if (catalogState.kind === "error") {
      return [theme.fg("dim", ` 加载失败: ${catalogState.msg}`)];
    }
    const { decks, selectedIndex, downloading } = catalogState;
    if (decks.length === 0) {
      return [theme.fg("dim", " 词库目录为空")];
    }

    const start = decks.length <= MAX_LIST_ROWS
      ? 0
      : Math.min(Math.max(0, selectedIndex - MAX_LIST_ROWS + 1), decks.length - MAX_LIST_ROWS);
    const visible = decks.slice(start, start + MAX_LIST_ROWS);
    const rows: string[] = [];

    for (let i = 0; i < visible.length; i++) {
      const deck = visible[i];
      const idx = start + i;
      const isSelected = idx === selectedIndex;
      const isDownloading = deck.id === downloading;
      const isDownloaded = downloadedIds.has(deck.id);

      const prefix = isSelected ? "▶ " : "  ";
      const badge = isDownloading ? " ⟳" : isDownloaded ? " ✓" : "";
      const countStr = `${deck.word_count}词`;
      const tags = deck.tags.slice(0, 3).join(" ");

      const nameWidth = 14;
      const countWidth = 7;
      const nameCell = truncateToWidth(deck.name, nameWidth, "…");
      const namePad = nameCell + " ".repeat(Math.max(0, nameWidth - visibleWidth(nameCell)));
      // padStart uses JS length; compensate for CJK "词" taking 2 visual columns
      const countVisualWidth = visibleWidth(countStr);
      const countPad = " ".repeat(Math.max(0, countWidth - countVisualWidth)) + countStr;
      const rest = `  ${tags}${badge}`;
      const line = `${prefix}${namePad}  ${countPad}${rest}`;

      const colored = isSelected
        ? theme.fg("accent", truncateToWidth(line, iw, "...", true))
        : theme.fg("dim", truncateToWidth(line, iw, "...", true));
      rows.push(colored);
    }

    if (decks.length > MAX_LIST_ROWS) {
      rows.push(theme.fg("dim", truncateToWidth(` (${decks.length} 个，↑↓ 滚动)`, iw, "...", true)));
    }

    return rows;
  }

  function renderMyDecksRows(iw: number, theme: Theme): string[] {
    if (myDecksState.kind === "loading") {
      return [theme.fg("dim", " 加载中...")];
    }
    const { decks, selectedIndex, confirmDelete } = myDecksState;

    if (confirmDelete) {
      return [
        theme.fg("accent", truncateToWidth(` 删除词库 "${confirmDelete.name}"？`, iw, "...", true)),
        theme.fg("dim",    truncateToWidth("  删除后无法恢复，所有卡片将一并删除", iw, "...", true)),
        theme.fg("accent", truncateToWidth("  [y] 确认删除    [n/Esc] 取消", iw, "...", true)),
      ];
    }

    if (decks.length === 0) {
      return [theme.fg("dim", " 暂无词库：按 i 导入自建词库，或从词库目录下载")];
    }

    const start = decks.length <= MAX_LIST_ROWS
      ? 0
      : Math.min(Math.max(0, selectedIndex - MAX_LIST_ROWS + 1), decks.length - MAX_LIST_ROWS);
    const visible = decks.slice(start, start + MAX_LIST_ROWS);
    const rows: string[] = [];

    for (let i = 0; i < visible.length; i++) {
      const deck = visible[i];
      const idx = start + i;
      const isSelected = idx === selectedIndex;
      const activeMark = deck.active ? " ●" : "";
      const prefix = isSelected ? "▶ " : "  ";
      const line = `${prefix}${deck.name}${activeMark}`;
      const colored = isSelected
        ? theme.fg("accent", truncateToWidth(line, iw, "...", true))
        : truncateToWidth(line, iw, "...", true);
      rows.push(colored);
    }

    if (decks.length > MAX_LIST_ROWS) {
      rows.push(theme.fg("dim", truncateToWidth(` (${decks.length} 个，↑↓ 滚动)`, iw, "...", true)));
    }

    return rows;
  }

  function renderDetailRow(iw: number, theme: Theme): string | null {
    if (activeTab === "catalog") {
      if (catalogState.kind !== "ready" || catalogState.decks.length === 0) return null;
      const deck = catalogState.decks[catalogState.selectedIndex];
      if (!deck) return null;
      const parts: string[] = [];
      if (deck.description) parts.push(deck.description);
      if (deck.tags.length > 0) parts.push(deck.tags.join("  "));
      return parts.length > 0 ? " " + parts.join("    ") : null;
    } else {
      if (myDecksState.kind !== "ready" || myDecksState.confirmDelete) return null;
      const deck = myDecksState.decks[myDecksState.selectedIndex];
      if (!deck?.description) return null;
      return " " + deck.description;
    }
  }

  function renderHintBar(iw: number, theme: Theme): string {
    let hint: string;
    if (activeTab === "catalog") {
      const st = catalogState;
      const downloadingDeck = st.kind === "ready" && st.downloading
        ? st.decks.find((deck) => deck.id === st.downloading)
        : null;
      hint = downloadingDeck
        ? `正在下载 ${downloadingDeck.name}...`
        : "Enter 下载    Esc 关闭";
    } else {
      const st = myDecksState;
      const inConfirm = st.kind === "ready" && st.confirmDelete !== null;
      hint = inConfirm
        ? "y 确认    n/Esc 取消"
        : "Enter 激活    i 导入自建词库    d 删除    ←→ 切换    Esc 关闭";
    }
    return statusMsg
      ? theme.fg("accent", truncateToWidth(" " + statusMsg, iw, "...", true))
      : theme.fg("dim", truncateToWidth(" " + hint, iw, "...", true));
  }

  async function handleCatalogEnter(): Promise<void> {
    if (catalogState.kind !== "ready") return;
    if (catalogState.downloading !== null) return;
    const deck = catalogState.decks[catalogState.selectedIndex];
    if (!deck) return;

    catalogState = { ...catalogState, downloading: deck.id };
    requestRender?.();

    try {
      const res = await runFishword(["catalog", "fetch", deck.id, "--json"]);
      if (isErrorResponse(res)) {
        setStatus(`下载失败: ${getErrorMessage(res) ?? getErrorCode(res) ?? "unknown"}`);
      } else if (res["schema"] === "fishword.protocol.catalog_fetch.v1") {
        const r = res as unknown as CatalogFetchResponse;
        downloadedIds.add(deck.id);
        setStatus(`✓ 已导入 ${deck.name}，共 ${r.import.inserted + r.import.merged + r.import.updated} 词`);
        void loadMyDecks();
        onDeckChanged();
      } else {
        setStatus("下载失败: 协议不匹配");
      }
    } catch (err) {
      setStatus(`下载失败: ${describeFishwordError(err)}`);
    }

    if (catalogState.kind === "ready") {
      catalogState = { ...catalogState, downloading: null };
    }
    requestRender?.();
  }

  async function handleMyDecksEnter(): Promise<void> {
    if (myDecksState.kind !== "ready") return;
    if (myDecksState.confirmDelete) return;
    const deck = myDecksState.decks[myDecksState.selectedIndex];
    if (!deck) return;
    try {
      await runFishword(["deck", "use", String(deck.id), "--json"]);
      await loadMyDecks();
      onDeckChanged();
      setStatus(`已切换到: ${deck.name}`);
    } catch {
      setStatus("切换词库失败");
    }
  }

  async function handleDeleteConfirm(): Promise<void> {
    if (myDecksState.kind !== "ready" || !myDecksState.confirmDelete) return;
    const deck = myDecksState.confirmDelete;
    try {
      const res = await runFishword(["deck", "delete", String(deck.id), "--json"]);
      if (isErrorResponse(res)) {
        setStatus(`删除失败: ${getErrorCode(res) ?? "unknown"}`);
      } else if (res["schema"] === "fishword.protocol.deck_delete.v1") {
        const r = res as unknown as DeckDeleteResponse;
        setStatus(`已删除词库: ${r.deleted.name}`);
        onDeckChanged();
        await loadMyDecks();
      } else {
        setStatus("删除失败");
      }
    } catch {
      setStatus("删除失败");
    }
  }

  void ctx.ui.custom<DeckManagerResult>(
    (tui, theme, _kb, done) => {
      requestRender = () => tui.requestRender();

      return {
        render(width: number) {
          const w = Math.min(width, OVERLAY_WIDTH);
          const iw = w - 2;
          const row = (content: string) =>
            theme.fg("border", "│") + truncateToWidth(content, iw, "...", true) + theme.fg("border", "│");
          const separator = theme.fg("border", "├" + "─".repeat(iw) + "┤");
          const contentRows =
            activeTab === "catalog"
              ? renderCatalogRows(iw, theme)
              : renderMyDecksRows(iw, theme);
          const detail = renderDetailRow(iw, theme);

          return [
            theme.fg("border", "╭" + "─".repeat(iw) + "╮"),
            renderTabBar(iw, theme),
            separator,
            ...contentRows.map((r) => row(r)),
            separator,
            row(detail ? theme.fg("dim", truncateToWidth(detail, iw, "...", true)) : ""),
            separator,
            row(renderHintBar(iw, theme)),
            theme.fg("border", "╰" + "─".repeat(iw) + "╯"),
          ];
        },
        invalidate() {},
        handleInput(keyData: string) {
          if (handleVisibilityShortcut(keyData, options)) return;

          // Esc: in confirm-delete mode cancel confirm, otherwise close overlay
          if (matchesKey(keyData, Key.escape)) {
            if (myDecksState.kind === "ready" && myDecksState.confirmDelete) {
              myDecksState = { ...myDecksState, confirmDelete: null };
              requestRender?.();
            } else {
              done(undefined);
            }
            return;
          }

          // 左右箭头切换页面
          if (matchesKey(keyData, Key.left)) {
            activeTab = "my-decks";
            requestRender?.();
            return;
          }
          if (matchesKey(keyData, Key.right)) {
            activeTab = "catalog";
            requestRender?.();
            return;
          }

          if (activeTab === "catalog") {
            if (catalogState.kind !== "ready") return;
            if (matchesKey(keyData, Key.up)) {
              catalogState = {
                ...catalogState,
                selectedIndex: Math.max(0, catalogState.selectedIndex - 1),
              };
              requestRender?.();
            } else if (matchesKey(keyData, Key.down)) {
              catalogState = {
                ...catalogState,
                selectedIndex: Math.min(catalogState.decks.length - 1, catalogState.selectedIndex + 1),
              };
              requestRender?.();
            } else if (matchesKey(keyData, Key.enter)) {
              void handleCatalogEnter();
            }
          } else {
            if (myDecksState.kind !== "ready") return;
            const { confirmDelete } = myDecksState;

            if (confirmDelete) {
              if (keyData.toLowerCase() === "y") {
                void handleDeleteConfirm();
              } else if (keyData.toLowerCase() === "n") {
                myDecksState = { ...myDecksState, confirmDelete: null };
                requestRender?.();
              }
              return;
            }

            if (matchesKey(keyData, Key.up)) {
              myDecksState = {
                ...myDecksState,
                selectedIndex: Math.max(0, myDecksState.selectedIndex - 1),
              };
              requestRender?.();
            } else if (matchesKey(keyData, Key.down)) {
              myDecksState = {
                ...myDecksState,
                selectedIndex: Math.min(myDecksState.decks.length - 1, myDecksState.selectedIndex + 1),
              };
              requestRender?.();
            } else if (matchesKey(keyData, Key.enter)) {
              void handleMyDecksEnter();
            } else if (keyData.toLowerCase() === "i") {
              done("import");
            } else if (keyData.toLowerCase() === "d") {
              const deck = myDecksState.decks[myDecksState.selectedIndex];
              if (deck) {
                myDecksState = { ...myDecksState, confirmDelete: deck };
                requestRender?.();
              }
            }
          }
        },
      };
    },
    {
      overlay: true,
      overlayOptions: {
        anchor: "center",
        width: OVERLAY_WIDTH,
        margin: 1,
      },
      onHandle,
    },
  ).then((result) => {
    if (statusTimer) clearTimeout(statusTimer);
    if (result === "import") onImportRequested();
    else onClose();
  });
}
