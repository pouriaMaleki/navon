import { observer } from "mobx-react-lite";
import type { ChangeEvent } from "react";
import type { AppStore } from "../stores/AppStore";
import styles from "./Controls.module.css";

type ControlsProps = {
  appStore: AppStore;
};

export const Controls = observer(({ appStore }: ControlsProps) => {
  const { bikeSimStore, emulatorStore, geoStore } = appStore;
  const disabled = !emulatorStore.isReady || emulatorStore.isLoading;
  const requestGpsDisabled = !emulatorStore.isReady || geoStore.isRequestInFlight;
  const perfText =
    emulatorStore.frameSamples > 0
      ? `${emulatorStore.fps.toFixed(1)} fps / ${emulatorStore.frameDtAvgMs.toFixed(1)} ms`
      : "warming up";

  return (
    <section className={styles["controls"]}>
      <div className={styles["actions"]}>
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
          disabled={requestGpsDisabled}
        >
          {geoStore.requestButtonLabel}
        </button>
        <span className={styles["status"]} data-tone={geoStore.statusTone}>
          {geoStore.statusText}
        </span>
        <span className={styles["metrics"]}>Frame: {perfText}</span>
      </div>

      <section className={styles["tuning"]} aria-label="Bike physics tuning">
        <header className={styles["tuningHeader"]}>
          <h2 className={styles["tuningTitle"]}>Bike Physics</h2>
          <button
            className={styles["button"]}
            type="button"
            onClick={bikeSimStore.resetPhysicsConfig}
          >
            Reset Tuning
          </button>
        </header>

        <RangeControl
          label="Max Speed"
          value={bikeSimStore.physicsConfig.maxSpeedKmh}
          min={5}
          max={60}
          step={1}
          unit="km/h"
          onChange={bikeSimStore.setMaxSpeedKmh}
        />
        <RangeControl
          label="Throttle Accel"
          value={bikeSimStore.physicsConfig.throttleAccelMps2}
          min={0.2}
          max={6}
          step={0.1}
          unit="m/s²"
          onChange={bikeSimStore.setThrottleAccelMps2}
        />
        <RangeControl
          label="Coast Decel"
          value={bikeSimStore.physicsConfig.coastDecelMps2}
          min={0.05}
          max={2.5}
          step={0.05}
          unit="m/s²"
          onChange={bikeSimStore.setCoastDecelMps2}
        />
        <RangeControl
          label="Brake Decel"
          value={bikeSimStore.physicsConfig.brakeDecelMps2}
          min={0.2}
          max={6}
          step={0.1}
          unit="m/s²"
          onChange={bikeSimStore.setBrakeDecelMps2}
        />
        <RangeControl
          label="Max Steer"
          value={bikeSimStore.physicsConfig.maxSteerDeg}
          min={5}
          max={35}
          step={1}
          unit="deg"
          onChange={bikeSimStore.setMaxSteerDeg}
        />
        <RangeControl
          label="Steer Response"
          value={bikeSimStore.physicsConfig.steerResponseDegPerSec}
          min={5}
          max={90}
          step={1}
          unit="deg/s"
          onChange={bikeSimStore.setSteerResponseDegPerSec}
        />
        <RangeControl
          label="Wheelbase"
          value={bikeSimStore.physicsConfig.wheelbaseM}
          min={0.2}
          max={2.5}
          step={0.01}
          unit="m"
          onChange={bikeSimStore.setWheelbaseM}
        />
      </section>
    </section>
  );
});

type RangeControlProps = {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit: string;
  onChange: (value: number) => void;
};

function RangeControl({ label, value, min, max, step, unit, onChange }: RangeControlProps) {
  return (
    <label className={styles["rangeRow"]}>
      <span className={styles["rangeLabel"]}>{label}</span>
      <input
        className={styles["slider"]}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event: ChangeEvent<HTMLInputElement>) => {
          onChange(Number(event.currentTarget.value));
        }}
      />
      <span className={styles["rangeValue"]}>
        {formatValue(value, step)} {unit}
      </span>
    </label>
  );
}

function formatValue(value: number, step: number): string {
  if (step >= 1) {
    return value.toFixed(0);
  }
  if (step >= 0.1) {
    return value.toFixed(1);
  }
  return value.toFixed(2);
}
