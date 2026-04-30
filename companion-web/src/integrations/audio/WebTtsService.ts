// Thin wrapper over the Web Speech API used to speak audio cues. Cancels any
// in-flight utterance before queuing a new one so the latest cue is always
// what the rider hears (turn-by-turn cues need recency, not completeness).

export class WebTtsService {
  /** BCP-47 language tag passed to `SpeechSynthesisUtterance.lang`. The
   *  routing coordinator updates this whenever the active app locale
   *  changes; if the OS has no installed voice for the tag, the browser
   *  falls back silently to its default voice. */
  private lang: string = "en";

  setLang(lang: string): void {
    this.lang = lang;
  }

  speak(text: string): void {
    const synthesis = (window as unknown as { speechSynthesis?: SpeechSynthesis }).speechSynthesis;
    if (!synthesis) return;
    const Utterance = (
      window as unknown as {
        SpeechSynthesisUtterance?: typeof SpeechSynthesisUtterance;
      }
    ).SpeechSynthesisUtterance;
    if (!Utterance) return;
    try {
      synthesis.cancel();
    } catch {
      /* ignore */
    }
    try {
      const utterance = new Utterance(text);
      utterance.lang = this.lang;
      synthesis.speak(utterance);
    } catch {
      /* ignore */
    }
  }

  /** True when the current device has a TTS voice installed for the active
   *  locale. UI uses this to surface a "Voice for X is not installed" hint
   *  in Settings. */
  hasVoiceForActiveLang(): boolean {
    const synthesis = (window as unknown as { speechSynthesis?: SpeechSynthesis }).speechSynthesis;
    if (!synthesis || typeof synthesis.getVoices !== "function") return true;
    const primary = this.lang.split("-")[0]?.toLowerCase();
    if (!primary) return true;
    const voices = synthesis.getVoices();
    if (voices.length === 0) return true; // not yet enumerated; don't false-alarm
    return voices.some((v) => v.lang.toLowerCase().startsWith(primary));
  }
}
