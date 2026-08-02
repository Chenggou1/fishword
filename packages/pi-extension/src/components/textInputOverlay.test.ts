import type { TUI } from "@earendil-works/pi-tui";
import { describe, expect, it, vi } from "vitest";
import { TextInputOverlay } from "./textInputOverlay.ts";

describe("TextInputOverlay", () => {
  it("submits the prefilled deck name", () => {
    const done = vi.fn();
    const overlay = new TextInputOverlay(
      { fg: (_style, text) => text },
      { requestRender() {} } as unknown as TUI,
      done,
      { title: "确认词库名称", initialValue: "工作词汇" },
    );

    overlay.handleInput("\r");

    expect(done).toHaveBeenCalledWith("工作词汇");
  });
});
