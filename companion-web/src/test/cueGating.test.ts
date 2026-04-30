import { describe, expect, it } from "vitest";
import { shouldDispatchCues } from "../integrations/cues/cueGating.js";

// Spec line 144: "setting to enable audio cues only when app is in
// background and enabled by default". When ON, cues are suppressed while
// the rider has the tab/app focused — their map is already visible —
// and only fire after the page goes hidden (tab switch / screen lock).
//
// Test the pure decision function so the autorun-driven coordinator
// stays trivial. The autorun's job is to feed inputs; the gating logic
// lives here.

describe("shouldDispatchCues", () => {
  const base = {
    isRouting: true,
    audioCuesEnabled: true,
    allowBackgroundGps: true,
    pairedWithDevice: false,
    audioCuesOnlyInBackground: false,
    isAppInBackground: false,
  };

  it("fires when all base gates are open and the only-background guard is off", () => {
    expect(shouldDispatchCues(base)).toBe(true);
  });

  it("is silent until allowBackgroundGps is on", () => {
    expect(shouldDispatchCues({ ...base, allowBackgroundGps: false })).toBe(false);
  });

  it("is silent when paired with a device", () => {
    expect(shouldDispatchCues({ ...base, pairedWithDevice: true })).toBe(false);
  });

  it("is silent in foreground when audioCuesOnlyInBackground is on", () => {
    expect(
      shouldDispatchCues({
        ...base,
        audioCuesOnlyInBackground: true,
        isAppInBackground: false,
      }),
    ).toBe(false);
  });

  it("fires in background when audioCuesOnlyInBackground is on", () => {
    expect(
      shouldDispatchCues({
        ...base,
        audioCuesOnlyInBackground: true,
        isAppInBackground: true,
      }),
    ).toBe(true);
  });

  it("fires in foreground when audioCuesOnlyInBackground is off (rider opted in to always-on cues)", () => {
    expect(
      shouldDispatchCues({
        ...base,
        audioCuesOnlyInBackground: false,
        isAppInBackground: false,
      }),
    ).toBe(true);
  });
});

describe("audioCuesOnlyInBackground default", () => {
  it("defaults to true per spec line 144", async () => {
    const { DEFAULT_COMPANION_SETTINGS } = await import("../domain/models.js");
    expect(DEFAULT_COMPANION_SETTINGS.audioCuesOnlyInBackground).toBe(true);
  });
});
