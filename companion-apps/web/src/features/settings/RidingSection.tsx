import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import styles from "./RoutePlannerSettingsView.module.css";

type Props = { store: RootStore };

export const RidingSection = observer(({ store }: Props) => {
  const settings = store.settingsStore.settings;

  return (
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
  );
});
