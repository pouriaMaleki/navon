import { observer } from "mobx-react-lite";
import type { AppStore } from "../stores/AppStore";
import styles from "./Controls.module.css";

type ControlsProps = {
  appStore: AppStore;
};

export const Controls = observer(({ appStore }: ControlsProps) => {
  const { emulatorStore, geoStore } = appStore;
  const toggleLabel = emulatorStore.isRunning ? "Pause" : "Resume";
  const disabled = !emulatorStore.isReady || emulatorStore.isLoading;

  return (
    <section className={styles["controls"]}>
      <button
        className={styles["button"]}
        type="button"
        onClick={emulatorStore.toggleRunning}
        disabled={disabled}
      >
        {toggleLabel}
      </button>
      <button
        className={styles["button"]}
        type="button"
        onClick={emulatorStore.reset}
        disabled={disabled}
      >
        Reset
      </button>
      <button
        className={styles["button"]}
        type="button"
        onClick={geoStore.requestLiveGps}
        disabled={!emulatorStore.isReady}
      >
        Request GPS
      </button>
      <span className={styles["status"]}>{geoStore.statusText}</span>
    </section>
  );
});
