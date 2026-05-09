import { observer } from "mobx-react-lite";
import type { ReactElement } from "react";
import type { BikeSimStore } from "../stores/BikeSimStore";
import styles from "./BikeControls.module.css";

type BikeControlsProps = {
  bikeSimStore: BikeSimStore;
};

export const BikeControls = observer(({ bikeSimStore }: BikeControlsProps) => {
  const { input } = bikeSimStore;

  return (
    <section className={styles["controls"]} aria-label="Simulated bike controls">
      <div className={styles["cluster"]}>
        <ControlButton
          label="↑"
          caption="Pedal"
          active={input.accelerate}
          onPress={() => bikeSimStore.setAccelerate(true)}
          onRelease={() => bikeSimStore.setAccelerate(false)}
        />
        <div className={styles["row"]}>
          <ControlButton
            label="←"
            caption="Turn"
            active={input.turnLeft}
            onPress={() => bikeSimStore.setTurnLeft(true)}
            onRelease={() => bikeSimStore.setTurnLeft(false)}
          />
          <ControlButton
            label="↓"
            caption="Brake"
            active={input.brake}
            onPress={() => bikeSimStore.setBrake(true)}
            onRelease={() => bikeSimStore.setBrake(false)}
          />
          <ControlButton
            label="→"
            caption="Turn"
            active={input.turnRight}
            onPress={() => bikeSimStore.setTurnRight(true)}
            onRelease={() => bikeSimStore.setTurnRight(false)}
          />
        </div>
      </div>
      <div className={styles["telemetry"]}>
        <span className={styles["speed"]}>{bikeSimStore.speedKmh.toFixed(1)} km/h</span>
        <span className={styles["hint"]}>Arrow keys drive the simulated bike GPS.</span>
      </div>
    </section>
  );
});

type ControlButtonProps = {
  label: string;
  caption: string;
  active: boolean;
  onPress: () => void;
  onRelease: () => void;
};

function ControlButton({
  label,
  caption,
  active,
  onPress,
  onRelease,
}: ControlButtonProps): ReactElement {
  return (
    <button
      className={styles["button"]}
      data-active={active ? "1" : "0"}
      type="button"
      onPointerDown={(event) => {
        event.preventDefault();
        onPress();
      }}
      onPointerUp={onRelease}
      onPointerLeave={onRelease}
      onPointerCancel={onRelease}
    >
      <span className={styles["label"]}>{label}</span>
      <span className={styles["caption"]}>{caption}</span>
    </button>
  );
}
