// Observable bridge over the Web Speech API voices list. Browsers (Chrome
// in particular) populate `speechSynthesis.getVoices()` asynchronously and
// fire the `voiceschanged` event when the list is ready. We mirror that
// list into a MobX box so that:
//
//  1. The Settings hint (`settings.language.noVoiceFallback`) renders
//     correctly on first paint and re-renders if the OS finishes loading
//     voices later.
//  2. The cue dispatcher can decide on each tick whether to fall back to
//     English audio for a locale that has no installed voice.

import { observable, runInAction } from "mobx";

const voicesBox = observable.box<SpeechSynthesisVoice[]>([], { deep: false });

if (typeof window !== "undefined") {
  const synth = (window as unknown as { speechSynthesis?: SpeechSynthesis }).speechSynthesis;
  if (synth && typeof synth.getVoices === "function") {
    const refresh = () => {
      runInAction(() => voicesBox.set(synth.getVoices()));
    };
    refresh();
    if (typeof synth.addEventListener === "function") {
      synth.addEventListener("voiceschanged", refresh);
    }
  }
}

/**
 * True when the platform has at least one TTS voice whose primary tag
 * matches `locale`. Returns `true` while voices are still enumerating
 * (empty list) so we don't false-alarm during the initial load — the
 * MobX box updates once `voiceschanged` fires and the UI re-renders.
 */
export function hasVoiceForLocale(locale: string): boolean {
  const voices = voicesBox.get();
  if (voices.length === 0) return true;
  const primary = locale.split("-")[0]?.toLowerCase();
  if (!primary) return true;
  return voices.some((v) => v.lang.toLowerCase().startsWith(primary));
}
