// Wraps navigator.geolocation to surface a single "request permission" entry
// point used when the user toggles "Allow GPS in background". Returns a
// classification the UI can use to render hints.

export type BackgroundGpsResult = "granted" | "denied" | "unsupported";

export class BackgroundGpsService {
  async requestPermission(): Promise<BackgroundGpsResult> {
    const geolocation = navigator.geolocation;
    if (!geolocation) return "unsupported";
    return new Promise((resolve) => {
      geolocation.getCurrentPosition(
        () => resolve("granted"),
        () => resolve("denied"),
        { enableHighAccuracy: true, timeout: 15000, maximumAge: 0 },
      );
    });
  }
}
