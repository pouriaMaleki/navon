import { observer } from "mobx-react-lite";
import type { AppStore } from "../stores/AppStore";
import styles from "./GpsControls.module.css";

type GpsControlsProps = {
  appStore: AppStore;
};

export const GpsControls = observer(({ appStore }: GpsControlsProps) => {
  const { emulatorStore, geoStore } = appStore;
  const requestGpsDisabled = !emulatorStore.isReady || geoStore.isRequestInFlight;

  return (
    <section className={styles["panel"]} aria-label="GPS controls">
      <header className={styles["header"]}>
        <p className={styles["eyebrow"]}>Live GPS</p>
        <h2 className={styles["title"]}>Browser location</h2>
      </header>

      <p className={styles["status"]} data-tone={geoStore.statusTone}>
        {geoStore.statusText}
      </p>

      {geoStore.guidanceText ? <p className={styles["guidance"]}>{geoStore.guidanceText}</p> : null}

      <button
        className={styles["button"]}
        type="button"
        onClick={geoStore.requestLiveGps}
        disabled={requestGpsDisabled}
      >
        {geoStore.requestButtonLabel}
      </button>
    </section>
  );
});
