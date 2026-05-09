import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { AppStore } from "../stores/AppStore";
import { BikeControls } from "./BikeControls";
import styles from "./EmulatorPanel.module.css";

type EmulatorPanelProps = {
  appStore: AppStore;
  className?: string | undefined;
  variant?: "default" | "fullscreen" | "web_fullscreen";
  showBikeControls?: boolean;
};

function joinClassNames(...classNames: Array<string | undefined>): string {
  return classNames.filter(Boolean).join(" ");
}

export const EmulatorPanel = observer(
  ({ appStore, className, variant = "default", showBikeControls = true }: EmulatorPanelProps) => {
    const { bikeSimStore, emulatorStore } = appStore;
    const canvasRef = useRef<HTMLCanvasElement | null>(null);

    useEffect(() => {
      const canvas = canvasRef.current;
      if (!canvas) {
        return;
      }
      void emulatorStore.init(canvas);
    }, [emulatorStore]);

    useEffect(() => {
      const canvas = canvasRef.current;
      if (!canvas) {
        return;
      }

      let frameId = 0;
      const scheduleSync = () => {
        if (frameId !== 0) {
          return;
        }
        frameId = window.requestAnimationFrame(() => {
          frameId = 0;
          void emulatorStore.syncCanvasProfile(canvas);
        });
      };

      const resizeObserver = new ResizeObserver(() => {
        scheduleSync();
      });
      resizeObserver.observe(canvas);

      const viewport = window.visualViewport;
      viewport?.addEventListener("resize", scheduleSync);
      window.addEventListener("orientationchange", scheduleSync);

      return () => {
        if (frameId !== 0) {
          window.cancelAnimationFrame(frameId);
        }
        resizeObserver.disconnect();
        viewport?.removeEventListener("resize", scheduleSync);
        window.removeEventListener("orientationchange", scheduleSync);
      };
    }, [emulatorStore]);

    return (
      <section className={joinClassNames(styles["panel"], className)} data-variant={variant}>
        <div className={styles["screen"]}>
          <div className={styles["viewport"]}>
            <canvas
              ref={canvasRef}
              className={styles["canvas"]}
              width={800}
              height={800}
              aria-label="Minimap canvas"
            />
          </div>
        </div>
        {showBikeControls ? <BikeControls bikeSimStore={bikeSimStore} /> : null}
        {emulatorStore.errorMessage ? (
          <p className={styles["error"]}>Emulator error: {emulatorStore.errorMessage}</p>
        ) : null}
      </section>
    );
  },
);
