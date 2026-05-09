// Browser/platform detection used to surface the right hint when the user
// turns on "Allow GPS in background". iOS Safari does not actually honour
// background geolocation in a web context; the hint sets expectations.

export function detectSafariOnIOS(): boolean {
  const ua = navigator.userAgent ?? "";
  const isIOS = /iPad|iPhone|iPod/i.test(ua);
  const isSafari = /Safari/i.test(ua) && !/CriOS|FxiOS|EdgiOS|Chrome/i.test(ua);
  return isIOS && isSafari;
}

/** Returns a catalog key (not a translated string) so the caller can
 *  resolve it against the active locale via `t(...)`. Keeping the
 *  detection here decoupled from i18n means tests can stub navigator
 *  without dragging in the catalog. */
export function platformGpsHintKey(): string {
  return detectSafariOnIOS()
    ? "home.platformHint.locationSafariIos"
    : "home.platformHint.locationDefault";
}
