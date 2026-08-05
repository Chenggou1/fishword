import { describe, expect, it, vi } from "vitest";
import {
  createMacOSSpeechAdapter,
  createSpeechPlayer,
  createSystemSpeechPlayer,
  type MacOSSpeechHost,
  type SpeechAdapter,
  type SpeechPlayback,
} from "./speech.ts";

function playback(): SpeechPlayback {
  return { stop: vi.fn(), done: Promise.resolve() };
}

describe("Fishword speech player", () => {
  it("uses a voice matching the card's BCP 47 language", async () => {
    const play = vi.fn(() => playback());
    const adapter: SpeechAdapter = {
      availableVoices: async () => [
        { id: "Samantha", language: "en-US" },
        { id: "Kyoko", language: "ja-JP" },
        { id: "Yuna", language: "ko-KR" },
      ],
      play,
    };
    const player = createSpeechPlayer(adapter);

    await player.speak({ text: "こんにちは", language: "ja-JP" });

    expect(play).toHaveBeenCalledWith("こんにちは", "Kyoko");
  });

  it("falls back to a voice for the same base language", async () => {
    const play = vi.fn(() => playback());
    const adapter: SpeechAdapter = {
      availableVoices: async () => [
        { id: "Samantha", language: "en-US" },
        { id: "Daniel", language: "en-GB" },
      ],
      play,
    };
    const player = createSpeechPlayer(adapter);

    await player.speak({ text: "colour", language: "en-AU" });

    expect(play).toHaveBeenCalledWith("colour", "Samantha");
  });

  it("stops the previous pronunciation before starting another", async () => {
    const first = { stop: vi.fn(), done: new Promise<void>(() => {}) };
    const second = playback();
    const adapter: SpeechAdapter = {
      availableVoices: async () => [],
      play: vi.fn().mockReturnValueOnce(first).mockReturnValueOnce(second),
    };
    const player = createSpeechPlayer(adapter);

    void player.speak({ text: "first", language: "en" });
    await vi.waitFor(() => expect(adapter.play).toHaveBeenCalledTimes(1));
    await player.speak({ text: "second", language: "en" });

    expect(first.stop).toHaveBeenCalledOnce();
  });

  it("uses an installed macOS voice for Japanese text", async () => {
    const play = vi.fn(() => playback());
    const host: MacOSSpeechHost = {
      listVoices: async () =>
        [
          "Samantha            en_US    # Hello! My name is Samantha.",
          "Eddy (日语（日本）)       ja_JP    # こんにちは! 私の名前はEddyです。",
          "Kyoko               ja_JP    # こんにちは! 私の名前はKyokoです。",
          "Yuna                ko_KR    # 안녕하세요. 제 이름은 유나입니다.",
        ].join("\n"),
      play,
    };
    const player = createSpeechPlayer(createMacOSSpeechAdapter(host));

    await player.speak({ text: "単語", language: "ja" });

    expect(play).toHaveBeenCalledWith("単語", "Kyoko");
  });

  it("reports an actionable error on platforms without an adapter", async () => {
    const player = createSystemSpeechPlayer("linux");

    await expect(player.speak({ text: "test", language: "en" })).rejects.toThrow(
      "not supported on linux",
    );
  });

  it("does not start speaking if stopped while voices are loading", async () => {
    let resolveVoices!: (voices: Array<{ id: string; language: string }>) => void;
    const voices = new Promise<Array<{ id: string; language: string }>>((resolve) => {
      resolveVoices = resolve;
    });
    const adapter: SpeechAdapter = {
      availableVoices: () => voices,
      play: vi.fn(() => playback()),
    };
    const player = createSpeechPlayer(adapter);

    const speaking = player.speak({ text: "word", language: "en" });
    player.stop();
    resolveVoices([]);
    await speaking;

    expect(adapter.play).not.toHaveBeenCalled();
  });
});
