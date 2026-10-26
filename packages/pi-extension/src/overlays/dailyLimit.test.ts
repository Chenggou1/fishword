import type { ExtensionContext } from "@earendil-works/pi-coding-agent";
import { beforeEach, describe, expect, it, vi } from "vitest";

const CLI_STATE = vi.hoisted(() => ({
  limit: 20,
  defaultLimit: 20,
  writes: [] as string[],
  failWrite: undefined as string | undefined,
  unsupported: false,
}));

const fishword = vi.hoisted(() => ({
  run: vi.fn((args: string[]) => {
    if (args[0] !== "deck" || args[1] !== "daily-limit") {
      return Promise.resolve({});
    }
    const value = args.slice(2).find((arg) => arg !== "--json");
    if (value === undefined) {
      if (CLI_STATE.unsupported) {
        return Promise.resolve({
          schema: "fishword.protocol.error.v1",
          error: { code: "unknown_argument", message: "unknown subcommand" },
        });
      }
      return Promise.resolve({
        schema: "fishword.protocol.daily_limit.v1",
        daily_new_limit: CLI_STATE.limit,
        default_limit: CLI_STATE.defaultLimit,
      });
    }
    CLI_STATE.writes.push(value);
    const parsed = Number.parseInt(value, 10);
    if (CLI_STATE.failWrite) {
      return Promise.resolve({
        schema: "fishword.protocol.error.v1",
        error: { code: "invalid_daily_limit", message: CLI_STATE.failWrite },
      });
    }
    CLI_STATE.limit = parsed;
    return Promise.resolve({
      schema: "fishword.protocol.daily_limit.v1",
      daily_new_limit: parsed,
      default_limit: CLI_STATE.defaultLimit,
    });
  }),
}));

vi.mock("../fishword.ts", () => ({
  runFishword: fishword.run,
  isErrorResponse: (res: Record<string, unknown>) =>
    res["schema"] === "fishword.protocol.error.v1",
  getErrorMessage: (res: Record<string, unknown>) =>
    (res["error"] as { message?: string } | undefined)?.message,
  getErrorCode: () => undefined,
  describeFishwordError: () => "unknown",
}));

import { parseDailyLimit, promptDailyLimit, readDailyLimit } from "./dailyLimit.ts";

type CustomFactory = (...args: unknown[]) => unknown;

function makeContext(answer: string | undefined) {
  const notify = vi.fn();
  const custom = vi.fn<(factory: CustomFactory, options?: unknown) => Promise<unknown>>(
    () => Promise.resolve(answer),
  );
  const ctx = { ui: { custom, notify } } as unknown as ExtensionContext;
  return { ctx, custom, notify };
}

beforeEach(() => {
  CLI_STATE.limit = 20;
  CLI_STATE.defaultLimit = 20;
  CLI_STATE.writes = [];
  CLI_STATE.failWrite = undefined;
  CLI_STATE.unsupported = false;
  fishword.run.mockClear();
});

describe("parseDailyLimit", () => {
  it("accepts positive integers and trims whitespace", () => {
    expect(parseDailyLimit("80")).toEqual({ value: 80 });
    expect(parseDailyLimit("  35  ")).toEqual({ value: 35 });
  });

  it("rejects empty, non-numeric, zero and negative input", () => {
    for (const raw of ["", "   ", "abc", "12abc", "0", "-3", "1.5"]) {
      expect(parseDailyLimit(raw)).toHaveProperty("error");
    }
  });

  it("rejects unsafe integers", () => {
    expect(parseDailyLimit("9007199254740992")).toHaveProperty("error");
  });
});

describe("readDailyLimit", () => {
  it("returns the CLI state", async () => {
    await expect(readDailyLimit()).resolves.toEqual({
      schema: "fishword.protocol.daily_limit.v1",
      daily_new_limit: 20,
      default_limit: 20,
    });
  });

  it("returns undefined when the CLI does not support the command", async () => {
    CLI_STATE.unsupported = true;
    await expect(readDailyLimit()).resolves.toBeUndefined();
  });
});

describe("promptDailyLimit", () => {
  it("writes the new limit and reports the change", async () => {
    const { ctx, custom } = makeContext("80");

    const result = await promptDailyLimit(ctx);

    expect(custom).toHaveBeenCalledOnce();
    expect(CLI_STATE.writes).toEqual(["80"]);
    expect(result).toEqual({ kind: "changed", limit: 80 });
  });

  it("prefills the current limit and shows the default", async () => {
    const { ctx, custom } = makeContext(undefined);
    CLI_STATE.limit = 50;
    CLI_STATE.defaultLimit = 20;

    await promptDailyLimit(ctx);

    const factory = custom.mock.calls[0][0];
    const rendered = factory(
      { requestRender() {} },
      { fg: (_style: string, text: string) => text },
      {},
      () => {},
    ) as { render(width: number): string[] };
    const lines = rendered.render(60).join("\n");
    expect(lines).toContain("50");
    expect(lines).toContain("20");
  });

  it("treats Escape as cancelled without writing", async () => {
    const { ctx } = makeContext(undefined);

    await expect(promptDailyLimit(ctx)).resolves.toEqual({ kind: "cancelled" });
    expect(CLI_STATE.writes).toEqual([]);
  });

  it("rejects invalid input without writing", async () => {
    const { ctx, notify } = makeContext("abc");

    await expect(promptDailyLimit(ctx)).resolves.toEqual({ kind: "unchanged" });
    expect(CLI_STATE.writes).toEqual([]);
    expect(notify).toHaveBeenCalledWith("每日新词上限必须是正整数", "error");
  });

  it("skips the write when the value is unchanged", async () => {
    const { ctx } = makeContext("20");

    await expect(promptDailyLimit(ctx)).resolves.toEqual({ kind: "unchanged" });
    expect(CLI_STATE.writes).toEqual([]);
  });

  it("reports CLI rejections and keeps the old value", async () => {
    CLI_STATE.failWrite = "Daily new limit must be positive, got 0";
    const { ctx, notify } = makeContext("80");

    await expect(promptDailyLimit(ctx)).resolves.toEqual({ kind: "unchanged" });
    expect(notify).toHaveBeenCalledWith(
      "Daily new limit must be positive, got 0",
      "error",
    );
  });

  it("reports unsupported CLIs without opening the input overlay", async () => {
    CLI_STATE.unsupported = true;
    const { ctx, custom, notify } = makeContext("80");

    await expect(promptDailyLimit(ctx)).resolves.toEqual({ kind: "unsupported" });
    expect(custom).not.toHaveBeenCalled();
    expect(notify).toHaveBeenCalledWith(expect.stringContaining("不支持"), "error");
  });
});
