import { afterEach, describe, expect, it, vi } from "vitest";
import { detectSafariOnIOS, platformGpsHint } from "../features/settings/PlatformHints.js";
import { BackgroundGpsService } from "../integrations/permissions/BackgroundGpsService.js";

describe("BackgroundGpsService", () => {
  const originalGeolocation = navigator.geolocation;

  afterEach(() => {
    Object.defineProperty(navigator, "geolocation", {
      configurable: true,
      value: originalGeolocation,
    });
  });

  it("calls getCurrentPosition with high accuracy when requesting permission", async () => {
    const getCurrentPosition = vi.fn(
      (
        success: PositionCallback,
        _error: PositionErrorCallback | null,
        options?: PositionOptions,
      ) => {
        expect(options?.enableHighAccuracy).toBe(true);
        success({
          coords: { latitude: 1, longitude: 2 } as GeolocationCoordinates,
          timestamp: 0,
        } as GeolocationPosition);
      },
    );
    Object.defineProperty(navigator, "geolocation", {
      configurable: true,
      value: { getCurrentPosition },
    });
    const service = new BackgroundGpsService();
    await service.requestPermission();
    expect(getCurrentPosition).toHaveBeenCalled();
  });

  it("resolves to denied when geolocation API is unavailable", async () => {
    Object.defineProperty(navigator, "geolocation", {
      configurable: true,
      value: undefined,
    });
    const service = new BackgroundGpsService();
    const result = await service.requestPermission();
    expect(result).toBe("unsupported");
  });
});

describe("PlatformHints", () => {
  const originalUA = navigator.userAgent;

  afterEach(() => {
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: originalUA,
    });
  });

  function setUserAgent(ua: string) {
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: ua,
    });
  }

  it("detects iOS Safari from a typical iPhone user agent", () => {
    setUserAgent(
      "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1",
    );
    expect(detectSafariOnIOS()).toBe(true);
  });

  it("returns false for desktop Chrome", () => {
    setUserAgent("Mozilla/5.0 (Macintosh) AppleWebKit/537.36 Chrome/120 Safari/537.36");
    expect(detectSafariOnIOS()).toBe(false);
  });

  it("returns an iOS-specific hint string when on iOS Safari", () => {
    setUserAgent(
      "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 Version/17.5 Mobile/15E148 Safari/604.1",
    );
    const hint = platformGpsHint();
    expect(hint).toMatch(/Safari/i);
    expect(hint).toMatch(/background/i);
  });

  it("returns a generic browser hint on non-iOS", () => {
    setUserAgent("Mozilla/5.0 (Macintosh) Chrome/120 Safari/537.36");
    const hint = platformGpsHint();
    expect(hint).not.toMatch(/Safari/i);
  });
});
