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

const DIGITRANSIT_PORTAL_URL = "https://portal-api.digitransit.fi/";

type Props = { store: RootStore };

export const RoutePlannerSettingsView = observer(({ store }: Props) => {
  const settings = store.settingsStore.settings;
  const planner = store.settingsStore.plannerPreferences;

  return (
    <>
      <div className="settings-section">
        <h2>Defaults</h2>
        {store.planningStore.availableSourceModes.length > 1 ? (
          <div className="settings-row">
            <div>
              <div className="settings-row__label">Default route source</div>
              <div className="settings-row__hint">Used for new planning sessions.</div>
            </div>
            <select
              className="text-input"
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

        <div className="settings-row">
          <div>
            <div className="settings-row__label">Suggestions</div>
            <div className="settings-row__hint">Best route only or up to three.</div>
          </div>
          <select
            className="text-input"
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

        <div className="settings-row">
          <div>
            <div className="settings-row__label">Start behavior</div>
            <div className="settings-row__hint">When picking a recent route or import.</div>
          </div>
          <select
            className="text-input"
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

      <div className="settings-section">
        <h2>HSL Digitransit</h2>
        <div
          className="settings-row__hint"
          style={{ paddingBottom: 8, borderBottom: "1px solid var(--card-border)" }}
        >
          HSL is the Helsinki Region Transport authority. Their{" "}
          <a
            href={DIGITRANSIT_PORTAL_URL}
            target="_blank"
            rel="noreferrer noopener"
            style={{ color: "var(--accent)" }}
          >
            Digitransit
          </a>{" "}
          API provides high-quality bike routing across the Helsinki metro area. The key is free —
          sign in at the portal, register an app, and copy the subscription key into the field
          below. Outside the Helsinki area, leave HSL off and the planner uses OSM routing globally.
        </div>
        <div className="settings-row">
          <div>
            <div className="settings-row__label">Prefer live HSL routing</div>
            <div className="settings-row__hint">Falls back to sample if request fails.</div>
          </div>
          <label style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={settings.preferLiveHslRouting}
              onChange={(event) =>
                store.settingsStore.updateSettings({ preferLiveHslRouting: event.target.checked })
              }
            />
            <span style={{ fontSize: 13, color: "var(--fg-soft)" }}>
              {settings.preferLiveHslRouting ? "On" : "Off"}
            </span>
          </label>
        </div>

        <div className="settings-row" style={{ flexDirection: "column", alignItems: "stretch" }}>
          <div className="settings-row__label">Digitransit subscription key</div>
          <input
            type="password"
            className="text-input"
            value={settings.hslSubscriptionKey}
            placeholder="Paste your Digitransit key"
            autoComplete="off"
            onChange={(event) =>
              store.settingsStore.updateSettings({ hslSubscriptionKey: event.target.value })
            }
          />
        </div>

        <div className="settings-row" style={{ flexDirection: "column", alignItems: "stretch" }}>
          <div className="settings-row__label">Endpoint URL</div>
          <input
            type="url"
            className="text-input"
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
