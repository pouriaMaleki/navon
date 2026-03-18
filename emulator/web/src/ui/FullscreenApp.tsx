import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import type { AppStore } from "../stores/AppStore";
import { BikeControls } from "./BikeControls";
import { Controls } from "./Controls";
import { EmulatorPanel } from "./EmulatorPanel";
import styles from "./FullscreenApp.module.css";

type FullscreenAppProps = {
  appStore: AppStore;
};

export const FullscreenApp = observer(({ appStore }: FullscreenAppProps) => {
  const [drawerOpen, setDrawerOpen] = useState(false);

  useEffect(() => {
    return () => {
      appStore.dispose();
    };
  }, [appStore]);

  return (
    <main className={styles["shell"]}>
      <EmulatorPanel
        appStore={appStore}
        className={styles["mapFrame"]}
        variant="fullscreen"
        showBikeControls={false}
      />

      <button
        className={styles["toggle"]}
        type="button"
        aria-expanded={drawerOpen}
        aria-controls="fullscreen-control-drawer"
        onClick={() => {
          setDrawerOpen((open) => !open);
        }}
      >
        <span className={styles["toggleLabel"]}>Tools</span>
      </button>

      <aside
        id="fullscreen-control-drawer"
        className={styles["drawer"]}
        data-open={drawerOpen ? "1" : "0"}
      >
        <header className={styles["drawerHeader"]}>
          <div>
            <p className={styles["eyebrow"]}>Fullscreen Emulator</p>
            <h1 className={styles["title"]}>Minimap</h1>
          </div>
          <button
            className={styles["closeButton"]}
            type="button"
            onClick={() => {
              setDrawerOpen(false);
            }}
          >
            Close
          </button>
        </header>

        <div className={styles["drawerBody"]}>
          <BikeControls bikeSimStore={appStore.bikeSimStore} />
          <Controls appStore={appStore} />
        </div>
      </aside>

      <button
        className={styles["backdrop"]}
        type="button"
        aria-label="Close controls"
        data-open={drawerOpen ? "1" : "0"}
        onClick={() => {
          setDrawerOpen(false);
        }}
      />
    </main>
  );
});
