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
    // Apply the persisted language preference to the i18n runtime before any
    // components render, so the very first paint uses the right locale.
    setActiveLocale(resolveLocale(this.settings.language));
    makeAutoObservable(this, { snapshotForAdapter: false }, { autoBind: true });
  }

  updateSettings(patch: Partial<CompanionSettings>): void {
    this.settings = { ...this.settings, ...patch };
    if ("language" in patch) {
      setActiveLocale(resolveLocale(this.settings.language));
    }
    this.persistence.saveSettings(this.settings);
    // If HSL just became unavailable, normalize the default-source preference to OSM
    // (mixed/hsl only make sense when HSL is configured).
    if (!this.isHslLiveConfigured && this.plannerPreferences.defaultSourceMode !== "osm") {
      this.plannerPreferences = { ...this.plannerPreferences, defaultSourceMode: "osm" };
      this.persistence.savePlannerPreferences(this.plannerPreferences);
    }
  }

  updatePlannerPreferences(patch: Partial<RoutePlannerPreferences>): void {
    this.plannerPreferences = { ...this.plannerPreferences, ...patch };
    // Reject mixed/hsl as a default when no Digitransit key is configured.
    if (!this.isHslLiveConfigured && this.plannerPreferences.defaultSourceMode !== "osm") {
      this.plannerPreferences = { ...this.plannerPreferences, defaultSourceMode: "osm" };
    }
    this.persistence.savePlannerPreferences(this.plannerPreferences);
  }

  /** Snapshot used by integration adapters that need a sync settings provider. Never observable. */
  snapshotForAdapter(): CompanionSettings {
    return this.settings;
  }

  /** True when live HSL Digitransit routing is actually usable: toggle on AND key present. */
  get isHslLiveConfigured(): boolean {
    return this.settings.preferLiveHslRouting && this.settings.hslSubscriptionKey.trim().length > 0;
  }
}
