import { describe, expect, it, vi } from "vitest";
import { attachPrefixShortcut, createPrefixShortcut } from "./prefixShortcut.ts";

describe("Fishword prefix shortcut", () => {
  it("runs an action after Ctrl+Q followed by its mnemonic key", () => {
    const onAction = vi.fn();
    const shortcut = createPrefixShortcut({ onAction });

    expect(shortcut.handleInput("\x11")).toEqual({ consume: true });
    expect(shortcut.handleInput("f")).toEqual({ consume: true });
    expect(onAction).toHaveBeenCalledWith("fw");
  });

  it.each([
    ["f", "fw"],
    ["i", "fw-detail"],
    ["a", "fw-again"],
    ["h", "fw-hard"],
    ["g", "fw-good"],
    ["e", "fw-easy"],
  ] as const)("maps %s to %s", (key, action) => {
    const onAction = vi.fn();
    const shortcut = createPrefixShortcut({ onAction });

    shortcut.handleInput("\x11");

    expect(shortcut.handleInput(key)).toEqual({ consume: true });
    expect(onAction).toHaveBeenCalledWith(action);
  });

  it("keeps waiting across Kitty key releases and accepts CSI-u action keys", () => {
    const onAction = vi.fn();
    const shortcut = createPrefixShortcut({ onAction });

    expect(shortcut.handleInput("\x1b[113;5u")).toEqual({ consume: true });
    expect(shortcut.handleInput("\x1b[113;5:3u")).toBeUndefined();
    expect(shortcut.handleInput("\x1b[102u")).toEqual({ consume: true });
    expect(onAction).toHaveBeenCalledWith("fw");
  });

  it("cancels the pending prefix without forwarding Escape", () => {
    const onAction = vi.fn();
    const shortcut = createPrefixShortcut({ onAction });

    shortcut.handleInput("\x11");

    expect(shortcut.handleInput("\x1b")).toEqual({ consume: true });
    expect(shortcut.handleInput("f")).toBeUndefined();
    expect(onAction).not.toHaveBeenCalled();
  });

  it("forwards an unknown action key and leaves prefix mode", () => {
    const onAction = vi.fn();
    const shortcut = createPrefixShortcut({ onAction });

    shortcut.handleInput("\x11");

    expect(shortcut.handleInput("q")).toEqual({ data: "q" });
    expect(shortcut.handleInput("f")).toBeUndefined();
    expect(onAction).not.toHaveBeenCalled();
  });

  it("forwards later input after the prefix times out", () => {
    vi.useFakeTimers();
    try {
      const onAction = vi.fn();
      const onPendingChange = vi.fn();
      const shortcut = createPrefixShortcut({ onAction, onPendingChange, timeoutMs: 1_500 });

      shortcut.handleInput("\x11");
      vi.advanceTimersByTime(1_500);

      expect(shortcut.handleInput("g")).toBeUndefined();
      expect(onAction).not.toHaveBeenCalled();
      expect(onPendingChange.mock.calls).toEqual([[true], [false]]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("reports when it starts and stops waiting for an action key", () => {
    const onPendingChange = vi.fn();
    const shortcut = createPrefixShortcut({
      onAction: vi.fn(),
      onPendingChange,
    });

    shortcut.handleInput("\x11");
    shortcut.handleInput("i");

    expect(onPendingChange.mock.calls).toEqual([[true], [false]]);
  });

  it("cancels pending input when disposed", () => {
    const onAction = vi.fn();
    const onPendingChange = vi.fn();
    const shortcut = createPrefixShortcut({ onAction, onPendingChange });

    shortcut.handleInput("\x11");
    shortcut.dispose();

    expect(shortcut.handleInput("f")).toBeUndefined();
    expect(onAction).not.toHaveBeenCalled();
    expect(onPendingChange.mock.calls).toEqual([[true], [false]]);
  });

  it("attaches to Pi terminal input and displays the available action keys", () => {
    let terminalHandler: ((data: string) => unknown) | undefined;
    const setStatus = vi.fn();
    const unsubscribe = vi.fn();
    const onAction = vi.fn();
    const ui = {
      onTerminalInput(handler: (data: string) => unknown) {
        terminalHandler = handler;
        return unsubscribe;
      },
      setStatus,
    };

    const detach = attachPrefixShortcut(ui, onAction);
    terminalHandler?.("\x11");
    terminalHandler?.("f");
    detach();

    expect(setStatus).toHaveBeenNthCalledWith(
      1,
      "fishword-shortcuts",
      "Fishword Ctrl+Q：F 隐藏 · I 详情 · A/H/G/E 评分",
    );
    expect(setStatus).toHaveBeenLastCalledWith("fishword-shortcuts", undefined);
    expect(onAction).toHaveBeenCalledWith("fw");
    expect(unsubscribe).toHaveBeenCalledOnce();
  });

  it("keeps the prefix hint hidden after Boss Key while allowing summon", () => {
    let terminalHandler: ((data: string) => unknown) | undefined;
    const setStatus = vi.fn();
    const onAction = vi.fn();
    const ui = {
      onTerminalInput(handler: (data: string) => unknown) {
        terminalHandler = handler;
        return vi.fn();
      },
      setStatus,
    };

    attachPrefixShortcut(ui, onAction, { isFishwordHidden: () => true });
    terminalHandler?.("\x11");

    expect(setStatus).not.toHaveBeenCalled();

    terminalHandler?.("f");
    expect(onAction).toHaveBeenCalledWith("fw");
  });
});
