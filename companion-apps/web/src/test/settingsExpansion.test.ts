import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_COMPANION_SETTINGS } from "../domain/models.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";

describe("companion settings expansion (spec lines 128-145)", () => {
  beforeEach(() => {
    localStorage.clear();
  });
  afterEach(() => {
    localStorage.clear();
  });

  it("defaults match spec: keepScreenOn off, allowBackgroundGps off, audioCuesEnabled on, liveActivityEnabled off", () => {
    expect(DEFAULT_COMPANION_SETTINGS.keepScreenOn).toBe(false);
    expect(DEFAULT_COMPANION_SETTINGS.allowBackgroundGps).toBe(false);
    expect(DEFAULT_COMPANION_SETTINGS.audioCuesEnabled).toBe(true);
    expect(DEFAULT_COMPANION_SETTINGS.liveActivityEnabled).toBe(false);
  });

  it("legacy storage missing the new keys still loads with the spec defaults", () => {
    const legacy = {
      preferLiveHslRouting: true,
      hslSubscriptionKey: "key",
      hslEndpointURL: "https://example.com",
      cyclingSpeedKph: 20,
      speedUnit: "mph",
      ridingZoom: 17,
    };
    localStorage.setItem("companion.settings", JSON.stringify(legacy));
    const persistence = new LocalStoragePersistence();
    const loaded = persistence.loadSettings();
    expect(loaded.keepScreenOn).toBe(false);
    expect(loaded.allowBackgroundGps).toBe(false);
    expect(loaded.audioCuesEnabled).toBe(true);
    expect(loaded.liveActivityEnabled).toBe(false);
    expect(loaded.cyclingSpeedKph).toBe(20);
  });

  it("round-trips all four new fields through localStorage", () => {
    const persistence = new LocalStoragePersistence();
    persistence.saveSettings({
      ...DEFAULT_COMPANION_SETTINGS,
      keepScreenOn: true,
      allowBackgroundGps: true,
      audioCuesEnabled: false,
      liveActivityEnabled: true,
    });
    const loaded = persistence.loadSettings();
    expect(loaded.keepScreenOn).toBe(true);
    expect(loaded.allowBackgroundGps).toBe(true);
    expect(loaded.audioCuesEnabled).toBe(false);
    expect(loaded.liveActivityEnabled).toBe(true);
  });
});
