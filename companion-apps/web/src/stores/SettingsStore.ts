import { makeAutoObservable } from "mobx";
import {
  type CompanionSettings,
  DEFAULT_COMPANION_SETTINGS,
  DEFAULT_PLANNER_PREFERENCES,
  type RoutePlannerPreferences,
} from "../domain/models.js";
import { resolveLocale, setActiveLocale } from "../i18n/index.js";
import type { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";

export class SettingsStore {
  settings: CompanionSettings = DEFAULT_COMPANION_SETTINGS;
  plannerPreferences: RoutePlannerPreferences = DEFAULT_PLANNER_PREFERENCES;

  constructor(private readonly persistence: LocalStoragePersistence) {
    this.settings = persistence.loadSettings();
    this.plannerPreferences = persistence.loadPlannerPreferences();
    setActiveLocale(resolveLocale(this.settings.language));
    makeAutoObservable(this, { snapshotForAdapter: false }, { autoBind: true });
  }

  updateSettings(patch: Partial<CompanionSettings>): void {
    this.settings = { ...this.settings, ...patch };
    if ("language" in patch) {
      setActiveLocale(resolveLocale(this.settings.language));
    }
    this.persistence.saveSettings(this.settings);
  }

  updatePlannerPreferences(patch: Partial<RoutePlannerPreferences>): void {
    this.plannerPreferences = { ...this.plannerPreferences, ...patch };
    this.persistence.savePlannerPreferences(this.plannerPreferences);
  }

  snapshotForAdapter(): CompanionSettings {
    return this.settings;
  }
}
