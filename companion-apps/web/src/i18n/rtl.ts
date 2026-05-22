import type { Locale } from "./locale.js";

/** Right-to-left scripts. Mirror in catalog.config.json:localeOptions.
 *  Drives `<html dir="rtl">` and any layout helpers. */
const RTL_LOCALES: ReadonlySet<Locale> = new Set<Locale>(["ar", "fa", "ur"]);

export function isRtlLocale(locale: Locale): boolean {
  return RTL_LOCALES.has(locale);
}

/** Apply `<html dir="rtl|ltr">` so CSS logical properties (margin-inline-start,
 *  padding-inline-end, etc.) flip correctly for RTL languages. SSR-safe. */
export function applyDocumentDirection(locale: Locale): void {
  if (typeof document === "undefined") return;
  document.documentElement.dir = isRtlLocale(locale) ? "rtl" : "ltr";
  document.documentElement.lang = locale;
}
