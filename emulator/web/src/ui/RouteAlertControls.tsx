import { observer } from "mobx-react-lite";
import type { AppStore } from "../stores/AppStore";
import type { RouteAlertVerbosity } from "../types";
import styles from "./RouteAlertControls.module.css";

type RouteAlertControlsProps = {
  appStore: AppStore;
  compact?: boolean;
  showHeader?: boolean;
  onSelect?: () => void;
};

const OPTIONS: Array<{
  value: RouteAlertVerbosity;
  label: string;
}> = [
  { value: "essential", label: "Essential" },
  { value: "standard", label: "Standard" },
  { value: "detailed", label: "Detailed" },
];

export const RouteAlertControls = observer(
  ({ appStore, compact = false, showHeader = true, onSelect }: RouteAlertControlsProps) => {
    const disabled = !appStore.emulatorStore.isReady || appStore.emulatorStore.isLoading;

    return (
      <section className={styles["panel"]} aria-label="Route alert controls">
        {showHeader ? (
          <header className={styles["header"]}>
            <p className={styles["eyebrow"]}>Navigation Alerts</p>
            <h2 className={styles["title"]}>Route guidance detail</h2>
          </header>
        ) : null}

        <div className={styles["segmented"]} data-compact={compact ? "1" : "0"}>
          {OPTIONS.map((option) => (
            <button
              key={option.value}
              className={styles["button"]}
              type="button"
              data-active={appStore.routeAlertVerbosity === option.value ? "1" : "0"}
              disabled={disabled}
              onClick={() => {
                appStore.setRouteAlertVerbosity(option.value);
                onSelect?.();
              }}
            >
              {option.label}
            </button>
          ))}
        </div>

        {showHeader ? (
          <p className={styles["help"]}>
            Essential hides turn banners. Standard shows turn distance. Detailed adds richer turn wording.
          </p>
        ) : null}
      </section>
    );
  },
);
