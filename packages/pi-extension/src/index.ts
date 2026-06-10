import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fishwordEnv, fishwordPath } from "@fishword/cli";

const execAsync = promisify(execFile);

async function runFishword(args: string[]): Promise<Record<string, unknown>> {
  try {
    const { stdout } = await execAsync(fishwordPath, args, {
      env: fishwordEnv()
    });
    return JSON.parse(stdout.trim()) as Record<string, unknown>;
  } catch (err: unknown) {
    // exit code 2 → JSON error was printed to stdout before process.exit(2)
    const execErr = err as { stdout?: string };
    if (execErr.stdout) {
      try {
        return JSON.parse(execErr.stdout.trim()) as Record<string, unknown>;
      } catch {
        // stdout wasn't valid JSON; fall through and rethrow original error
      }
    }
    throw err;
  }
}

function formatCardStatus(res: Record<string, unknown>): string {
  const card = res["card"] as {
    term: string;
    phonetic?: { us?: string; uk?: string };
    meanings: string[];
    deck: { name: string };
  };

  const term = card.term;

  // Wrap phonetic in / / and strip any stray leading/trailing slashes from source data
  const rawPhonetic = card.phonetic?.us || card.phonetic?.uk || "";
  const phonetic = rawPhonetic ? `/${rawPhonetic.replace(/^\/|\/$/g, "")}/` : "";

  // Collapse runs of whitespace inside the meaning string (source data has double spaces)
  const meaning = (card.meanings[0] ?? "").replace(/\s+/g, " ").trim();

  const deckName = card.deck.name;

  const parts = [`📚 ${term}`];
  if (phonetic) parts.push(phonetic);
  if (meaning) parts.push(meaning);
  parts.push(deckName);

  return parts.join("  ·  ");
}

async function refreshStatus(ctx: ExtensionContext): Promise<void> {
  try {
    const res = await runFishword(["current", "--json"]);
    if (res["schema"] === "fishword.protocol.error.v1") {
      const errorCode = (res["error"] as { code?: string })?.code;
      switch (errorCode) {
        case "no_active_deck":
          ctx.ui.setStatus("fishword", "📚 no active deck — /fd deck <name>");
          break;
        case "no_cards":
          ctx.ui.setStatus("fishword", "📚 no cards");
          break;
        default:
          ctx.ui.setStatus("fishword", undefined);
      }
    } else {
      ctx.ui.setStatus("fishword", formatCardStatus(res));
    }
  } catch {
    ctx.ui.setStatus("fishword", undefined);
  }
}

export default function (pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) => {
    await refreshStatus(ctx);
  });

  pi.registerCommand("fd", {
    description: "Fishword: show current vocab card in status bar",
    handler: async (_args, ctx) => {
      await refreshStatus(ctx);
    },
  });

  pi.registerShortcut("ctrl+alt+v", {
    description: "Fishword: refresh vocab status",
    handler: async (ctx) => {
      await refreshStatus(ctx);
    },
  });
}
