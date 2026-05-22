import { observer } from "mobx-react-lite";
import type { RootStore } from "../../../app/RootStore.js";
import type { AppLanguagePref, DistanceUnitPref } from "../../../domain/models.js";
import { NATIVE_LANGUAGE_NAMES, resolveLocale, SUPPORTED_LOCALES } from "../../../i18n/index.js";
import { useT } from "../../../i18n/useT.js";
import { hasVoiceForLocale } from "../../../integrations/audio/voiceAvailability.js";
import styles from "./LocaleSettingsSection.module.css";

type Props = { store: RootStore };

/**
 * Language + Distance Units pickers. Persists to `CompanionSettings`; the
 * RoutingActivityCoordinator's autorun picks up the change and applies it
 * to the i18n runtime + TTS lang on the next render. No app restart
 * required on web (browser tabs aren't bundle-cached the way iOS is).
 *
 * The language picker iterates `SUPPORTED_LOCALES` and labels each option
 * with its native name (e.g. `Suomi`, `العربية`, `中文`) — matches iOS/
 * Android system pickers and avoids the AI-translator clobbering language
 * names like "English" → "Englanti" in a Finnish UI.
 */
export const LocaleSettingsSection = observer(({ store }: Props) => {
  const t = useT(store);
  const settings = store.settingsStore.settings;
  // Voice availability is reactive: the MobX-backed `hasVoiceForLocale`
  // re-runs this observer when the browser fires `voiceschanged`, so the
  // hint appears as soon as the OS finishes enumerating voices.
  const resolvedLocale = resolveLocale(settings.language);
  const showVoiceHint = !hasVoiceForLocale(resolvedLocale);

  return (
    <div className={styles.section} data-testid="locale-settings">
      <label className={styles.row} data-testid="setting-language">
        <div style={{ flex: 1, textAlign: "start" }}>
          <div className={styles.title}>{t("settings.language.title")}</div>
          <div className={styles.subtitle}>{t("settings.language.subtitle")}</div>
          {showVoiceHint && (
            <div
              className={[styles.subtitle, styles.warnHint].join(" ")}
              data-testid="setting-language-no-voice-hint"
            >
              {t("settings.language.noVoiceFallback", {
                language: NATIVE_LANGUAGE_NAMES[resolvedLocale],
              })}
            </div>
          )}
        </div>
        <select
          aria-label={t("settings.language.title")}
          value={settings.language}
          onChange={(e) =>
            store.settingsStore.updateSettings({
              language: e.target.value as AppLanguagePref,
            })
          }
        >
          <option value="system">{t("settings.language.system")}</option>
          {SUPPORTED_LOCALES.map((code) => (
            <option key={code} value={code}>
              {NATIVE_LANGUAGE_NAMES[code]}
            </option>
          ))}
        </select>
      </label>

      <label className={styles.row} data-testid="setting-distanceUnit">
        <div style={{ flex: 1, textAlign: "start" }}>
          <div className={styles.title}>{t("settings.distanceUnit.title")}</div>
        </div>
        <select
          aria-label={t("settings.distanceUnit.title")}
          value={settings.distanceUnit}
          onChange={(e) =>
            store.settingsStore.updateSettings({
              distanceUnit: e.target.value as DistanceUnitPref,
            })
          }
        >
          <option value="system">{t("settings.distanceUnit.system")}</option>
          <option value="metric">{t("settings.distanceUnit.metric")}</option>
          <option value="imperial">{t("settings.distanceUnit.imperial")}</option>
        </select>
      </label>
    </div>
  );
});
