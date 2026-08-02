import type { ExtensionContext } from "@earendil-works/pi-coding-agent";
import { describe, expect, it, vi } from "vitest";
import type { CardResponse } from "../types.ts";
import { showCardOverlay } from "./card.ts";

describe("card overlay", () => {
  it("remains visible when the workspace is taller than the terminal", () => {
    const tui = {
      terminal: { columns: 122, rows: 42 },
      render: () => Array.from({ length: 70 }, () => "workspace line"),
    };
    const theme = { fg: (_color: string, text: string) => text };
    const custom = vi.fn().mockImplementation((factory, options) => {
      const component = factory(tui, theme, {}, vi.fn());
      options.onHandle({ unfocus: vi.fn() });
      component.render(options.overlayOptions.width);
      return Promise.resolve(null);
    });
    const ctx = { ui: { custom } } as unknown as ExtensionContext;
    const response: CardResponse = {
      schema: "fishword.protocol.current.v1",
      card: {
        id: "repro:workspace",
        term: "workspace",
        language: "en",
        meanings: ["工作区"],
        deck: { id: "repro", name: "Repro", db_id: 1 },
        tags: ["repro"],
      },
      selection: { reason: "new" },
    };

    showCardOverlay(ctx, response, vi.fn());

    const overlayOptions = custom.mock.calls[0]![1].overlayOptions;
    const visible =
      typeof overlayOptions.visible === "function"
        ? overlayOptions.visible(tui.terminal.columns, tui.terminal.rows)
        : overlayOptions.visible !== false;

    expect(visible).toBe(true);
  });
});
