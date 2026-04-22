import type { Page } from "@playwright/test";

/**
 * Replace `navigator.geolocation.watchPosition` with a deterministic fake before
 * the app boots. Tests drive the rider position by calling `window.__fakeGeo`.
 */
export async function injectFakeGeolocation(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const listeners = new Map<number, (p: GeolocationPosition) => void>();
    let nextId = 1;
    const api = {
      getCurrentPosition: (success: (p: GeolocationPosition) => void) => {
        success(buildFix(60.17, 24.94));
      },
      watchPosition: (success: (p: GeolocationPosition) => void) => {
        const id = nextId++;
        listeners.set(id, success);
        return id;
      },
      clearWatch: (id: number) => {
        listeners.delete(id);
      },
    } satisfies Partial<Geolocation>;
    Object.defineProperty(navigator, "geolocation", {
      configurable: true,
      get: () => api as Geolocation,
    });
    (window as unknown as { __fakeGeo: (lat: number, lon: number) => void }).__fakeGeo = (
      lat: number,
      lon: number,
    ) => {
      for (const cb of listeners.values()) cb(buildFix(lat, lon));
    };
    function buildFix(lat: number, lon: number): GeolocationPosition {
      return {
        coords: {
          latitude: lat,
          longitude: lon,
          accuracy: 10,
          altitude: null,
          altitudeAccuracy: null,
          heading: null,
          speed: null,
          toJSON() {
            return this;
          },
        },
        timestamp: Date.now(),
        toJSON() {
          return this;
        },
      } as GeolocationPosition;
    }
  });
}
