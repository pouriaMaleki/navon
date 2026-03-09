import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { EmulatorStore } from "../stores/EmulatorStore";
import styles from "./EmulatorPanel.module.css";

type EmulatorPanelProps = {
  emulatorStore: EmulatorStore;
};

export const EmulatorPanel = observer(({ emulatorStore }: EmulatorPanelProps) => {
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
      {emulatorStore.errorMessage ? (
        <p className={styles["error"]}>Emulator error: {emulatorStore.errorMessage}</p>
      ) : null}
    </section>
  );
});
