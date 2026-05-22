import arMessages from "./messages/ar.json";
import bnMessages from "./messages/bn.json";
import deMessages from "./messages/de.json";
import enMessages from "./messages/en.json";
import esMessages from "./messages/es.json";
import faMessages from "./messages/fa.json";
import fiMessages from "./messages/fi.json";
import frMessages from "./messages/fr.json";
import hiMessages from "./messages/hi.json";
import idMessages from "./messages/id.json";
import jaMessages from "./messages/ja.json";
import mrMessages from "./messages/mr.json";
import pcmMessages from "./messages/pcm.json";
import ptMessages from "./messages/pt.json";
import ruMessages from "./messages/ru.json";
import urMessages from "./messages/ur.json";
import zhMessages from "./messages/zh.json";
import { applyDocumentDirection } from "./rtl.js";

export type AppLanguage =
  | "system"
  | "ar"
  | "bn"
  | "de"
  | "en"
  | "es"
  | "fa"
  | "fi"
  | "fr"
  | "hi"
  | "id"
  | "ja"
  | "mr"
  | "pcm"
  | "pt"
  | "ru"
  | "ur"
  | "zh";
export type DistanceUnit = "system" | "metric" | "imperial";

export const SUPPORTED_LOCALES = [
  "ar",
  "bn",
  "de",
  "en",
  "es",
  "fa",
  "fi",
  "fr",
  "hi",
  "id",
  "ja",
  "mr",
  "pcm",
  "pt",
  "ru",
  "ur",
  "zh",
] as const;
export type Locale = (typeof SUPPORTED_LOCALES)[number];

export const CATALOGS: Record<Locale, Record<string, string>> = {
  ar: arMessages,
  bn: bnMessages,
  de: deMessages,
  en: enMessages,
  es: esMessages,
  fa: faMessages,
  fi: fiMessages,
  fr: frMessages,
  hi: hiMessages,
  id: idMessages,
  ja: jaMessages,
  mr: mrMessages,
  pcm: pcmMessages,
  pt: ptMessages,
  ru: ruMessages,
  ur: urMessages,
  zh: zhMessages,
};

/** Native-language picker labels. Always presented in the language's own
 *  native form regardless of the active locale, matching iOS/Android
 *  system-language pickers. Not put in the catalog because translating
 *  these would defeat the convention. */
export const NATIVE_LANGUAGE_NAMES: Record<Locale, string> = {
  ar: "العربية",
  bn: "বাংলা",
  de: "Deutsch",
  en: "English",
  es: "Español",
  fa: "فارسی",
  fi: "Suomi",
  fr: "Français",
  hi: "हिन्दी",
  id: "Bahasa Indonesia",
  ja: "日本語",
  mr: "मराठी",
  pcm: "Naijá",
  pt: "Português",
  ru: "Русский",
  ur: "اردو",
  zh: "中文",
};

let activeLocale: Locale = "en";

export function getActiveLocale(): Locale {
  return activeLocale;
}

export function setActiveLocale(locale: Locale): void {
  activeLocale = locale;
  applyDocumentDirection(locale);
}

export function resolveLocale(language: AppLanguage): Locale {
  if (language !== "system") return language;
  const candidates =
    typeof navigator !== "undefined"
      ? (navigator.languages ?? [navigator.language ?? "en"])
      : ["en"];
  for (const tag of candidates) {
    const primary = tag.split("-")[0]?.toLowerCase();
    if (primary && (SUPPORTED_LOCALES as readonly string[]).includes(primary)) {
      return primary as Locale;
    }
  }
  return "en";
}

export function resolveDistanceUnit(
  preference: DistanceUnit,
  locale: Locale,
): "metric" | "imperial" {
  if (preference === "metric" || preference === "imperial") return preference;
  if (typeof navigator !== "undefined") {
    const tag = navigator.language ?? "en";
    const lower = tag.toLowerCase();
    if (lower.startsWith("en-us") || lower.startsWith("en-lr") || lower.startsWith("my")) {
      return "imperial";
    }
  }
  void locale;
  return "metric";
}
