import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import {
  ROUTE_SOURCE_MODE_DISPLAY_NAME,
  ROUTE_START_BEHAVIOR_DISPLAY_NAME,
  ROUTE_SUGGESTION_MODE_DISPLAY_NAME,
  type RouteSourceMode,
  type RouteStartBehavior,
  type RouteSuggestionMode,
} from "../../domain/models.js";
import styles from "./RoutePlannerSettingsView.module.css";

const DIGITRANSIT_PORTAL_URL = "https://portal-api.digitransit.fi/";

type Props = { store: RootStore };

export const RoutePlannerSettingsView = observer(({ store }: Props) => {
  const settings = store.settingsStore.settings;
  const planner = store.settingsStore.plannerPreferences;

  return (
    <>
      <div className={styles.section}>
        <h2>Defaults</h2>
        {store.planningStore.availableSourceModes.length > 1 ? (
          <div className={styles.row}>
            <div>
              <div className={styles.label}>Default route source</div>
              <div className={styles.hint}>Used for new planning sessions.</div>
            </div>
            <select
              className={styles.textInput}
              style={{ maxWidth: 180 }}
              value={planner.defaultSourceMode}
              onChange={(event) =>
                store.settingsStore.updatePlannerPreferences({
                  defaultSourceMode: event.target.value as RouteSourceMode,
                })
              }
            >
              {store.planningStore.availableSourceModes.map((mode) => (
                <option key={mode} value={mode}>
                  {ROUTE_SOURCE_MODE_DISPLAY_NAME[mode]}
                </option>
              ))}
            </select>
          </div>
        ) : null}

        <div className={styles.row}>
          <div>
            <div className={styles.label}>Suggestions</div>
            <div className={styles.hint}>Best route only or up to three.</div>
          </div>
          <select
            className={styles.textInput}
            style={{ maxWidth: 180 }}
            value={planner.suggestionMode}
            onChange={(event) =>
              store.settingsStore.updatePlannerPreferences({
                suggestionMode: event.target.value as RouteSuggestionMode,
              })
            }
          >
            <option value="bestOnly">{ROUTE_SUGGESTION_MODE_DISPLAY_NAME.bestOnly}</option>
            <option value="threeRoutes">{ROUTE_SUGGESTION_MODE_DISPLAY_NAME.threeRoutes}</option>
          </select>
        </div>

        <div className={styles.row}>
          <div>
            <div className={styles.label}>Start behavior</div>
            <div className={styles.hint}>When picking a recent route or import.</div>
          </div>
          <select
            className={styles.textInput}
            style={{ maxWidth: 180 }}
            value={planner.startBehavior}
            onChange={(event) =>
              store.settingsStore.updatePlannerPreferences({
                startBehavior: event.target.value as RouteStartBehavior,
              })
            }
          >
            <option value="explicit">{ROUTE_START_BEHAVIOR_DISPLAY_NAME.explicit}</option>
            <option value="automatic">{ROUTE_START_BEHAVIOR_DISPLAY_NAME.automatic}</option>
          </select>
        </div>
      </div>

      <div className={styles.section}>
        <h2>Riding</h2>
        <div className={styles.row}>
          <div>
            <div className={styles.label}>Cycling speed</div>
            <div className={styles.hint}>
              Used to compute route ETAs. HSL defaults to a slow bike speed; bump this if its
              estimates feel too long.
            </div>
          </div>
          <label className={styles.toggleLabel}>
            <input
              type="number"
              min={1}
              max={50}
              step={1}
              className={styles.textInput}
              style={{ width: 80 }}
              value={settings.cyclingSpeedKph}
              onChange={(event) => {
                const next = Number(event.target.value);
                if (!Number.isFinite(next) || next <= 0) return;
                store.settingsStore.updateSettings({ cyclingSpeedKph: next });
              }}
            />
            <span className={styles.checkboxStatus}>kph</span>
          </label>
        </div>
        <div className={styles.row}>
          <div>
            <div className={styles.label}>Speed unit</div>
            <div className={styles.hint}>How live speed is shown on the map.</div>
          </div>
          <select
            className={styles.textInput}
            style={{ maxWidth: 120 }}
            value={settings.speedUnit}
            onChange={(event) =>
              store.settingsStore.updateSettings({
                speedUnit: event.target.value === "mph" ? "mph" : "kph",
              })
            }
          >
            <option value="kph">km/h</option>
            <option value="mph">mph</option>
          </select>
        </div>
      </div>

      <div className={styles.section}>
        <h2>HSL Digitransit</h2>
        <div className={[styles.hint, styles.hintBlock].join(" ")}>
          HSL is the Helsinki Region Transport authority. Their{" "}
          <a
            href={DIGITRANSIT_PORTAL_URL}
            target="_blank"
            rel="noreferrer noopener"
            className={styles.link}
          >
            Digitransit
          </a>{" "}
          API provides high-quality bike routing across the Helsinki metro area. The key is free —
          sign in at the portal, register an app, and copy the subscription key into the field
          below. Outside the Helsinki area, leave HSL off and the planner uses OSM routing globally.
        </div>
        <div className={styles.row}>
          <div>
            <div className={styles.label}>Prefer live HSL routing</div>
            <div className={styles.hint}>Falls back to sample if request fails.</div>
          </div>
          <label className={styles.checkboxLabel}>
            <input
              type="checkbox"
              checked={settings.preferLiveHslRouting}
              onChange={(event) =>
                store.settingsStore.updateSettings({ preferLiveHslRouting: event.target.checked })
              }
            />
            <span className={styles.checkboxStatus}>
              {settings.preferLiveHslRouting ? "On" : "Off"}
            </span>
          </label>
        </div>

        <div className={[styles.row, styles.columnRow].join(" ")}>
          <div className={styles.label}>Digitransit subscription key</div>
          <input
            type="password"
            className={styles.textInput}
            value={settings.hslSubscriptionKey}
            placeholder="Paste your Digitransit key"
            autoComplete="off"
            onChange={(event) =>
              store.settingsStore.updateSettings({ hslSubscriptionKey: event.target.value })
            }
          />
        </div>

        <div className={[styles.row, styles.columnRow].join(" ")}>
          <div className={styles.label}>Endpoint URL</div>
          <input
            type="url"
            className={styles.textInput}
            value={settings.hslEndpointURL}
            onChange={(event) =>
              store.settingsStore.updateSettings({ hslEndpointURL: event.target.value })
            }
          />
        </div>
      </div>
    </>
  );
});
