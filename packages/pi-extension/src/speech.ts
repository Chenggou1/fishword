import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export type SpeechRequest = {
  text: string;
  language: string;
};

export type SpeechVoice = {
  id: string;
  language: string;
};

export type SpeechPlayback = {
  stop(): void;
  done: Promise<void>;
};

export type SpeechAdapter = {
  availableVoices(): Promise<SpeechVoice[]>;
  play(text: string, voice?: string): SpeechPlayback;
};

export type MacOSSpeechHost = {
  listVoices(): Promise<string>;
  play(text: string, voice?: string): SpeechPlayback;
};

export type SpeechPlayer = {
  speak(request: SpeechRequest): Promise<void>;
  stop(): void;
};

export function createSpeechPlayer(adapter: SpeechAdapter): SpeechPlayer {
  let current: SpeechPlayback | undefined;
  let revision = 0;

  return {
    async speak(request) {
      const requestRevision = ++revision;
      current?.stop();
      current = undefined;
      const voices = await adapter.availableVoices();
      if (requestRevision !== revision) return;
      const requestedLanguage = normalizeLanguage(request.language);
      const exactVoice = voices.find(
        (candidate) => normalizeLanguage(candidate.language) === requestedLanguage,
      );
      const baseLanguage = requestedLanguage.split("-")[0];
      const voice =
        exactVoice ??
        voices.find(
          (candidate) => normalizeLanguage(candidate.language).split("-")[0] === baseLanguage,
        );
      const playback = adapter.play(request.text, voice?.id);
      current = playback;
      try {
        await playback.done;
      } finally {
        if (current === playback) current = undefined;
      }
    },
    stop() {
      revision += 1;
      current?.stop();
      current = undefined;
    },
  };
}

export function createMacOSSpeechAdapter(host: MacOSSpeechHost): SpeechAdapter {
  let voices: Promise<SpeechVoice[]> | undefined;
  return {
    availableVoices() {
      voices ??= host.listVoices().then(parseMacOSVoices);
      return voices;
    },
    play: host.play,
  };
}

export function createSystemSpeechPlayer(platform: string = process.platform): SpeechPlayer {
  if (platform === "darwin") {
    return createSpeechPlayer(createMacOSSpeechAdapter(nodeMacOSSpeechHost));
  }
  return {
    async speak() {
      throw new Error(`System pronunciation is not supported on ${platform}`);
    },
    stop() {},
  };
}

const nodeMacOSSpeechHost: MacOSSpeechHost = {
  async listVoices() {
    const { stdout } = await execFileAsync("say", ["-v", "?"]);
    return stdout;
  },
  play(text, voice) {
    const child = spawn("say", voice ? ["-v", voice] : [], {
      stdio: ["pipe", "ignore", "ignore"],
    });
    let stopped = false;
    child.stdin.end(text);
    const done = new Promise<void>((resolve, reject) => {
      child.once("error", reject);
      child.once("exit", (code, signal) => {
        if (code === 0 || stopped) resolve();
        else reject(new Error(`say exited with ${code ?? signal ?? "unknown status"}`));
      });
    });
    return {
      done,
      stop() {
        stopped = true;
        child.kill();
      },
    };
  },
};

function parseMacOSVoices(output: string): SpeechVoice[] {
  const result: SpeechVoice[] = [];
  for (const line of output.split("\n")) {
    const match = line.match(/^(.+?)\s+([a-z]{2,3}(?:_[A-Z][A-Za-z0-9]{1,7})+)\s+#/);
    if (!match) continue;
    result.push({ id: match[1].trim(), language: match[2].replaceAll("_", "-") });
  }
  return result.sort((left, right) => macOSVoicePriority(left) - macOSVoicePriority(right));
}

const PREFERRED_MACOS_VOICES: Record<string, string> = {
  "en-us": "Samantha",
  "en-gb": "Daniel",
  "ja-jp": "Kyoko",
  "ko-kr": "Yuna",
};

function macOSVoicePriority(voice: SpeechVoice): number {
  const preferred = PREFERRED_MACOS_VOICES[normalizeLanguage(voice.language)];
  return preferred === voice.id ? 0 : 1;
}

function normalizeLanguage(language: string): string {
  try {
    return new Intl.Locale(language.replaceAll("_", "-")).toString().toLowerCase();
  } catch {
    return language.replaceAll("_", "-").toLowerCase();
  }
}
