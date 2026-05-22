import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// `voiceAvailability.ts` reads `speechSynthesis.getVoices()` at module
// load time and subscribes to `voiceschanged`. To exercise the gate
// behaviour we stub `window.speechSynthesis` BEFORE the module is
// imported, then re-import it via vi.resetModules + dynamic import.

type VoiceLike = Pick<SpeechSynthesisVoice, "lang">;

function installSpeechSynthesis(voices: VoiceLike[]) {
  const listeners: Array<() => void> = [];
  const synth = {
    getVoices: () => voices as SpeechSynthesisVoice[],
    addEventListener: (event: string, fn: () => void) => {
      if (event === "voiceschanged") listeners.push(fn);
    },
    removeEventListener: () => {},
    speak: () => {},
    cancel: () => {},
  };
  Object.defineProperty(window, "speechSynthesis", {
    configurable: true,
    value: synth,
  });
  return {
    fireVoicesChanged: () => {
      for (const fn of listeners) fn();
    },
    setVoices: (next: VoiceLike[]) => {
      voices.splice(0, voices.length, ...next);
    },
  };
}

describe("hasVoiceForLocale", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  afterEach(() => {
    Object.defineProperty(window, "speechSynthesis", {
      configurable: true,
      value: undefined,
    });
  });

  it("returns true while the voices list is still empty (initial enumeration)", async () => {
    installSpeechSynthesis([]);
    const { hasVoiceForLocale } = await import("./voiceAvailability.js");
    // Empty list means "still loading" — caller should not false-alarm.
    expect(hasVoiceForLocale("fa")).toBe(true);
    expect(hasVoiceForLocale("en")).toBe(true);
  });

  it("returns true when a voice exists with the same primary tag", async () => {
    installSpeechSynthesis([{ lang: "fa-IR" }, { lang: "en-US" }]);
    const { hasVoiceForLocale } = await import("./voiceAvailability.js");
    expect(hasVoiceForLocale("fa")).toBe(true);
    expect(hasVoiceForLocale("en")).toBe(true);
    // Country-code shouldn't matter — primary tag match is enough.
    expect(hasVoiceForLocale("fa-AF")).toBe(true);
  });

  it("returns false when no voice matches the primary tag", async () => {
    installSpeechSynthesis([{ lang: "en-US" }, { lang: "fi-FI" }]);
    const { hasVoiceForLocale } = await import("./voiceAvailability.js");
    expect(hasVoiceForLocale("fa")).toBe(false);
    expect(hasVoiceForLocale("ar")).toBe(false);
  });

  it("re-evaluates after `voiceschanged` fires", async () => {
    const seed: VoiceLike[] = [];
    const handle = installSpeechSynthesis(seed);
    const { hasVoiceForLocale } = await import("./voiceAvailability.js");
    // Empty list → reports true (loading).
    expect(hasVoiceForLocale("ja")).toBe(true);
    // OS finished enumerating without Japanese.
    handle.setVoices([{ lang: "en-US" }]);
    handle.fireVoicesChanged();
    expect(hasVoiceForLocale("ja")).toBe(false);
    expect(hasVoiceForLocale("en")).toBe(true);
  });
});
