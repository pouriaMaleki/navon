// Thin wrapper over the Web Speech API used to speak audio cues. Cancels any
// in-flight utterance before queuing a new one so the latest cue is always
// what the rider hears (turn-by-turn cues need recency, not completeness).

export class WebTtsService {
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
      synthesis.speak(utterance);
    } catch {
      /* ignore */
    }
  }
}
