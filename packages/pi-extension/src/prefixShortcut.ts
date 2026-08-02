import { isKeyRelease, matchesKey } from "@earendil-works/pi-tui";

export type PrefixShortcutAction =
  | "fw"
  | "fw-detail"
  | "fw-again"
  | "fw-hard"
  | "fw-good"
  | "fw-easy";

const PREFIX_ACTIONS = [
  { key: "f", action: "fw" },
  { key: "i", action: "fw-detail" },
  { key: "a", action: "fw-again" },
  { key: "h", action: "fw-hard" },
  { key: "g", action: "fw-good" },
  { key: "e", action: "fw-easy" },
] as const;

type TerminalInputResult = { consume?: boolean; data?: string } | undefined;

type PrefixShortcutUI = {
  onTerminalInput(handler: (data: string) => TerminalInputResult): () => void;
  setStatus(key: string, text: string | undefined): void;
};

export type PrefixShortcut = {
  handleInput(data: string): TerminalInputResult;
  dispose(): void;
};

export function createPrefixShortcut(options: {
  onAction: (action: PrefixShortcutAction) => void;
  onPendingChange?: (pending: boolean) => void;
  timeoutMs?: number;
}): PrefixShortcut {
  let waitingForAction = false;
  let timeout: ReturnType<typeof setTimeout> | undefined;

  function stopWaiting(): void {
    const wasWaiting = waitingForAction;
    waitingForAction = false;
    if (timeout) clearTimeout(timeout);
    timeout = undefined;
    if (wasWaiting) options.onPendingChange?.(false);
  }

  return {
    dispose: stopWaiting,
    handleInput(data) {
      if (isKeyRelease(data)) return undefined;

      if (!waitingForAction) {
        if (!matchesKey(data, "ctrl+q")) return undefined;
        waitingForAction = true;
        options.onPendingChange?.(true);
        timeout = setTimeout(stopWaiting, options.timeoutMs ?? 1_500);
        return { consume: true };
      }

      stopWaiting();
      if (matchesKey(data, "escape")) return { consume: true };

      const normalizedData = data.length === 1 ? data.toLowerCase() : data;
      const action = PREFIX_ACTIONS.find(({ key }) => matchesKey(normalizedData, key))?.action;
      if (!action) return { data };

      options.onAction(action);
      return { consume: true };
    },
  };
}

export function attachPrefixShortcut(
  ui: PrefixShortcutUI,
  onAction: (action: PrefixShortcutAction) => void,
  options: { isFishwordHidden?: () => boolean } = {},
): () => void {
  const shortcut = createPrefixShortcut({
    onAction,
    onPendingChange(pending) {
      if (pending && options.isFishwordHidden?.()) return;
      ui.setStatus(
        "fishword-shortcuts",
        pending ? "Fishword Ctrl+Q：F 隐藏 · I 详情 · A/H/G/E 评分" : undefined,
      );
    },
  });
  const unsubscribe = ui.onTerminalInput(shortcut.handleInput);

  return () => {
    unsubscribe();
    shortcut.dispose();
    ui.setStatus("fishword-shortcuts", undefined);
  };
}
