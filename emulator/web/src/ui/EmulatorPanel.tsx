import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { AppStore } from "../stores/AppStore";
import { BikeControls } from "./BikeControls";
import styles from "./EmulatorPanel.module.css";

type EmulatorPanelProps = {
  appStore: AppStore;
};

export const EmulatorPanel = observer(({ appStore }: EmulatorPanelProps) => {
  const { bikeSimStore, emulatorStore } = appStore;
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    void emulatorStore.init(canvas);
  }, [emulatorStore]);

  return (
    <section className={styles["panel"]}>
      <canvas
        ref={canvasRef}
        className={styles["canvas"]}
        width={800}
        height={800}
        aria-label="Minimap canvas"
      />
      <BikeControls bikeSimStore={bikeSimStore} />
      {emulatorStore.errorMessage ? (
        <p className={styles["error"]}>Emulator error: {emulatorStore.errorMessage}</p>
      ) : null}
    </section>
  );
});
