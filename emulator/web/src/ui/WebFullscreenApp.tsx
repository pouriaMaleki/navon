import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import type { AppStore } from "../stores/AppStore";
import { BikeControls } from "./BikeControls";
import { Controls } from "./Controls";
import { EmulatorPanel } from "./EmulatorPanel";
import styles from "./FullscreenApp.module.css";

type WebFullscreenAppProps = {
  appStore: AppStore;
};

export const WebFullscreenApp = observer(({ appStore }: WebFullscreenAppProps) => {
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
        variant="web_fullscreen"
        showBikeControls={false}
      />

      <button
        className={styles["toggle"]}
        type="button"
        aria-expanded={drawerOpen}
        aria-controls="web-fullscreen-control-drawer"
        onClick={() => {
          setDrawerOpen((open) => !open);
        }}
      >
        <span className={styles["toggleLabel"]}>Tools</span>
      </button>

      <aside
        id="web-fullscreen-control-drawer"
        className={styles["drawer"]}
        data-open={drawerOpen ? "1" : "0"}
      >
        <header className={styles["drawerHeader"]}>
          <div>
            <p className={styles["eyebrow"]}>Web Fullscreen</p>
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
