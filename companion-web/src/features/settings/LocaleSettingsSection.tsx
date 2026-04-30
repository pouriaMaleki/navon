import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import type { AppLanguagePref, DistanceUnitPref } from "../../domain/models.js";
import { useT } from "../../i18n/useT.js";

type Props = { store: RootStore };

/**
 * Language + Distance Units pickers. Persists to `CompanionSettings`; the
 * RoutingActivityCoordinator's autorun picks up the change and applies it
 * to the i18n runtime + TTS lang on the next render. No app restart
 * required on web (browser tabs aren't bundle-cached the way iOS is).
 */
export const LocaleSettingsSection = observer(({ store }: Props) => {
  const t = useT(store);
  const settings = store.settingsStore.settings;

  return (
    <div className="settings-section" data-testid="locale-settings">
      <label className="list-row" data-testid="setting-language">
        <div style={{ flex: 1, textAlign: "left" }}>
          <div className="list-row__title">{t("settings.language.title")}</div>
          <div className="list-row__subtitle">{t("settings.language.subtitle")}</div>
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
          <option value="en">{t("settings.language.en")}</option>
          <option value="fi">{t("settings.language.fi")}</option>
        </select>
      </label>

      <label className="list-row" data-testid="setting-distanceUnit">
        <div style={{ flex: 1, textAlign: "left" }}>
          <div className="list-row__title">{t("settings.distanceUnit.title")}</div>
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
