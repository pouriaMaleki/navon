import { formatMessage, type MessageValues } from "./messageFormat.js";
import { CATALOGS, getActiveLocale } from "./locale.js";

export {
  type AppLanguage,
  CATALOGS,
  type DistanceUnit,
  getActiveLocale,
  type Locale,
  NATIVE_LANGUAGE_NAMES,
  resolveDistanceUnit,
  resolveLocale,
  setActiveLocale,
  SUPPORTED_LOCALES,
} from "./locale.js";
export { applyDocumentDirection, isRtlLocale } from "./rtl.js";

export function t(key: string, values?: MessageValues): string {
  const activeLocale = getActiveLocale();
  const template = CATALOGS[activeLocale][key] ?? CATALOGS.en[key] ?? key;
  if (!values || Object.keys(values).length === 0) return template;
  try {
    return formatMessage(template, values, activeLocale);
  } catch {
    return template;
  }
}

export function tIn(locale: import("./locale.js").Locale, key: string, values?: MessageValues): string {
  const template = CATALOGS[locale][key] ?? CATALOGS.en[key] ?? key;
  if (!values || Object.keys(values).length === 0) return template;
  try {
    return formatMessage(template, values, locale);
  } catch {
    return template;
  }
}
