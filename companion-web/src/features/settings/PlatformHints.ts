// Browser/platform detection used to surface the right hint when the user
// turns on "Allow GPS in background". iOS Safari does not actually honour
// background geolocation in a web context; the hint sets expectations.

export function detectSafariOnIOS(): boolean {
  const ua = navigator.userAgent ?? "";
  const isIOS = /iPad|iPhone|iPod/i.test(ua);
  const isSafari = /Safari/i.test(ua) && !/CriOS|FxiOS|EdgiOS|Chrome/i.test(ua);
  return isIOS && isSafari;
}

export function platformGpsHint(): string {
  if (detectSafariOnIOS()) {
    return "Safari on iPhone does not deliver true background GPS. Keep the tab active and the screen on so location updates continue while you ride.";
  }
  return "Your browser will ask for location permission. Allow it so guidance keeps tracking your position while the page is in the background.";
}
