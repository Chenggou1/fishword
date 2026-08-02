import type { ExtensionContext } from "@earendil-works/pi-coding-agent";
import { describe, expect, it, vi } from "vitest";

const fishword = vi.hoisted(() => ({
  run: vi.fn((args: string[]) => {
    if (args[0] === "catalog") {
      return Promise.resolve({ schema: "fishword.protocol.catalog_list.v1", decks: [] });
    }
    return Promise.resolve({ schema: "fishword.protocol.decks.v1", decks: [] });
  }),
}));

vi.mock("../fishword.ts", () => ({
  runFishword: fishword.run,
  isErrorResponse: () => false,
  getErrorCode: () => undefined,
  getErrorMessage: () => undefined,
  describeFishwordError: () => "unknown",
}));

import { showDeckManagerOverlay } from "./deckManager.ts";

describe("deck manager", () => {
  it("opens custom deck import from the My Decks tab with i", async () => {
    let component: { handleInput(data: string): void } | undefined;
    const onImportRequested = vi.fn();
    const onClose = vi.fn();
    const custom = vi.fn((factory, options) => {
      let finish: (result: "import" | undefined) => void = () => {};
      const promise = new Promise<"import" | undefined>((resolve) => {
        finish = resolve;
      });
      component = factory(
        { requestRender() {} },
        { fg: (_style: string, text: string) => text, bold: (text: string) => text },
        {},
        finish,
      );
      options.onHandle({ setHidden() {}, hide() {} });
      return promise;
    });
    const ctx = { ui: { custom } } as unknown as ExtensionContext;

    showDeckManagerOverlay(ctx, {
      onHandle: vi.fn(),
      onClose,
      onDeckChanged: vi.fn(),
      onImportRequested,
    });
    await vi.waitFor(() => expect(fishword.run).toHaveBeenCalledTimes(2));

    component?.handleInput("i");

    await vi.waitFor(() => expect(onImportRequested).toHaveBeenCalledOnce());
    expect(onClose).not.toHaveBeenCalled();
  });
});
