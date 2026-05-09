import { afterEach, describe, expect, it, vi } from "vitest";
import { WebTtsService } from "../integrations/audio/WebTtsService.js";

type FakeUtterance = { text: string };

function installFakeSpeechSynthesis() {
  const speak = vi.fn();
  const cancel = vi.fn();
  const fake = { speak, cancel, speaking: false };
  Object.defineProperty(window, "speechSynthesis", {
    configurable: true,
    value: fake,
  });
  // Polyfill SpeechSynthesisUtterance for jsdom.
  const Utterance = vi.fn(function (this: FakeUtterance, text: string) {
    this.text = text;
  }) as unknown as typeof SpeechSynthesisUtterance;
  Object.defineProperty(window, "SpeechSynthesisUtterance", {
    configurable: true,
    value: Utterance,
  });
  return { speak, cancel, fake };
}

describe("WebTtsService", () => {
  const originalSynthesis = (window as unknown as { speechSynthesis?: unknown }).speechSynthesis;
  const originalUtterance = (window as unknown as { SpeechSynthesisUtterance?: unknown })
    .SpeechSynthesisUtterance;

  afterEach(() => {
    Object.defineProperty(window, "speechSynthesis", {
      configurable: true,
      value: originalSynthesis,
    });
    Object.defineProperty(window, "SpeechSynthesisUtterance", {
      configurable: true,
      value: originalUtterance,
    });
  });

  it("speaks the given text via SpeechSynthesisUtterance", () => {
    const fake = installFakeSpeechSynthesis();
    const service = new WebTtsService();
    service.speak("Route started");
    expect(fake.speak).toHaveBeenCalledTimes(1);
    const utterance = fake.speak.mock.calls[0][0] as FakeUtterance;
    expect(utterance.text).toBe("Route started");
  });

  it("cancels in-flight speech when a new cue arrives so the latest cue plays", () => {
    const fake = installFakeSpeechSynthesis();
    const service = new WebTtsService();
    service.speak("In 50 meters, turn left");
    service.speak("Turn left");
    expect(fake.cancel).toHaveBeenCalled();
    const lastUtterance = fake.speak.mock.calls.at(-1)?.[0] as FakeUtterance;
    expect(lastUtterance.text).toBe("Turn left");
  });

  it("is a no-op on browsers without speechSynthesis", () => {
    Object.defineProperty(window, "speechSynthesis", {
      configurable: true,
      value: undefined,
    });
    const service = new WebTtsService();
    expect(() => service.speak("hello")).not.toThrow();
  });
});
