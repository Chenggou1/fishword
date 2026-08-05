import type { ExtensionContext } from "@earendil-works/pi-coding-agent";
import { visibleWidth } from "@earendil-works/pi-tui";
import { describe, expect, it, vi } from "vitest";
import type { CardResponse } from "../types.ts";
import { showCardDetailOverlay } from "./cardDetail.ts";

describe("card detail overlay", () => {
  it("wraps a long example without dropping any text", () => {
    let component: { render(width: number): string[] } | undefined;
    const custom = vi.fn((factory, options) => {
      component = factory(
        {},
        { fg: (_style: string, text: string) => text },
        {},
        () => {},
      );
      options.onHandle({ unfocus() {} });
      return new Promise(() => {});
    });
    const ctx = { ui: { custom } } as unknown as ExtensionContext;
    const example =
      "They acted unreasonably when they turned down Jill's carefully prepared proposal.";
    const response: CardResponse = {
      schema: "fishword.protocol.current.v1",
      card: {
        id: "repro:act",
        term: "act",
        language: "en",
        phonetic: { us: "aekt" },
        meanings: [
          {
            part_of_speech: "v",
            definition: "表演；举动；起作用",
            example,
          },
        ],
        deck: { id: "repro", name: "Repro", db_id: 1 },
        tags: [],
      },
      selection: { reason: "new" },
    };

    showCardDetailOverlay(ctx, {
      response,
      onHandle: vi.fn(),
      onClose: vi.fn(),
      onRate: vi.fn(),
      onPronounce: vi.fn(),
    });

    const lines = component!.render(80);
    const renderedText = lines.join(" ");
    for (const word of example.split(" ")) {
      expect(renderedText).toContain(word);
    }
    expect(lines.every((line) => visibleWidth(line) <= 62)).toBe(true);
  });

  it("pronounces the displayed card when P is pressed", () => {
    let component: { handleInput(keyData: string): void } | undefined;
    const custom = vi.fn((factory, options) => {
      component = factory(
        {},
        { fg: (_style: string, text: string) => text },
        {},
        () => {},
      );
      options.onHandle({ unfocus() {} });
      return new Promise(() => {});
    });
    const ctx = { ui: { custom } } as unknown as ExtensionContext;
    const onPronounce = vi.fn();

    showCardDetailOverlay(ctx, {
      response: {
        schema: "fishword.protocol.current.v1",
        card: {
          id: "repro:word",
          term: "word",
          language: "en",
          meanings: [],
          deck: { id: "repro", name: "Repro", db_id: 1 },
          tags: [],
        },
        selection: { reason: "new" },
      },
      onHandle: vi.fn(),
      onClose: vi.fn(),
      onRate: vi.fn(),
      onPronounce,
    });

    component!.handleInput("p");

    expect(onPronounce).toHaveBeenCalledOnce();
  });
});
