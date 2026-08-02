import type { ExtensionContext } from "@earendil-works/pi-coding-agent";
import { visibleWidth } from "@earendil-works/pi-tui";
import { describe, expect, it, vi } from "vitest";
import type { CardResponse } from "../types.ts";
import { clearReviewWidget, showCardWidget, showDoneWidget } from "./card.ts";

describe("card widget", () => {
  it("shows the current card as a two-line study bar above the editor", () => {
    const setWidget = vi.fn();
    const ctx = { ui: { setWidget } } as unknown as ExtensionContext;
    const response: CardResponse = {
      schema: "fishword.protocol.current.v1",
      card: {
        id: "repro:workspace",
        term: "workspace",
        language: "en",
        phonetic: { us: "ˈwɜːrkˌspeɪs" },
        meanings: ["工作区"],
        deck: { id: "repro", name: "Repro", db_id: 1 },
        tags: ["repro"],
      },
      selection: { reason: "new" },
    };

    showCardWidget(ctx, response);

    const [key, factory, options] = setWidget.mock.calls[0]!;
    const theme = { fg: (_color: string, text: string) => text };
    const component = factory({}, theme);
    const lines = component.render(80);

    expect(key).toBe("fishword-review");
    expect(options).toEqual({ placement: "aboveEditor" });
    expect(lines).toHaveLength(2);
    expect(lines.join(" ")).toContain("Ctrl+Q");
    expect(lines.join(" ")).toContain("workspace");
    expect(lines.join(" ")).toContain("工作区");
    expect(lines.join(" ")).toContain("按下开始");
    expect(lines.every((line: string) => visibleWidth(line) <= 80)).toBe(true);
  });

  it("clears the study bar", () => {
    const setWidget = vi.fn();
    const ctx = { ui: { setWidget } } as unknown as ExtensionContext;

    clearReviewWidget(ctx);

    expect(setWidget).toHaveBeenCalledWith("fishword-review", undefined);
  });

  it("shows completion in the same study bar", () => {
    const setWidget = vi.fn();
    const ctx = { ui: { setWidget } } as unknown as ExtensionContext;

    showDoneWidget(ctx);

    const factory = setWidget.mock.calls[0]![1];
    const theme = { fg: (_color: string, text: string) => text };
    const component = factory({ requestRender: vi.fn() }, theme);
    const lines = component.render(80);

    expect(lines).toHaveLength(2);
    expect(lines.join(" ")).toContain("Ctrl+Q");
    expect(lines.join(" ")).toContain("DONE");
    component.dispose();
  });
});
