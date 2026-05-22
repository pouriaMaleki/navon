import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import styles from "./RoutePlannerSettingsView.module.css";

const DIGITRANSIT_PORTAL_URL = "https://portal-api.digitransit.fi/";

type Props = { store: RootStore };

export const HslSection = observer(({ store }: Props) => {
  const settings = store.settingsStore.settings;

  return (
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
        sign in at the portal, register an app, and copy the subscription key into the field below.
        Outside the Helsinki area, leave HSL off and the planner uses OSM routing globally.
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
  );
});
