import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import type { AppStore } from "../stores/AppStore";
import { EmulatorPanel } from "./EmulatorPanel";
import styles from "./FullscreenApp.module.css";
import { GpsControls } from "./GpsControls";
import { RouteAlertControls } from "./RouteAlertControls";

type WebFullscreenAppProps = {
  appStore: AppStore;
};

export const WebFullscreenApp = observer(({ appStore }: WebFullscreenAppProps) => {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [alertMenuOpen, setAlertMenuOpen] = useState(false);
  const gpsNeedsAttention = appStore.geoStore.needsUserAction;

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

      <div className={styles["floatingDock"]}>
        <button
          className={styles["toggle"]}
          type="button"
          aria-expanded={drawerOpen}
          aria-controls="web-fullscreen-control-drawer"
          data-gps-attention={gpsNeedsAttention ? "1" : "0"}
          data-placement="bottom"
          onClick={() => {
            setDrawerOpen((open) => !open);
            setAlertMenuOpen(false);
          }}
        >
          <span className={styles["toggleLabel"]}>GPS</span>
          {gpsNeedsAttention ? <span className={styles["toggleBadge"]}>Fix</span> : null}
        </button>

        <div className={styles["menuWrap"]}>
          <button
            className={styles["toggle"]}
            type="button"
            aria-expanded={alertMenuOpen}
            aria-controls="web-fullscreen-alert-menu"
            data-placement="bottom"
            onClick={() => {
              setAlertMenuOpen((open) => !open);
              setDrawerOpen(false);
            }}
          >
            <span className={styles["toggleLabel"]}>Alerts</span>
          </button>

          <div
            id="web-fullscreen-alert-menu"
            className={styles["menu"]}
            data-open={alertMenuOpen ? "1" : "0"}
          >
            <RouteAlertControls
              appStore={appStore}
              compact
              showHeader={false}
              onSelect={() => {
                setAlertMenuOpen(false);
              }}
            />
          </div>
        </div>
      </div>

      <aside
        id="web-fullscreen-control-drawer"
        className={styles["drawer"]}
        data-open={drawerOpen ? "1" : "0"}
        data-layout="bottom_sheet"
      >
        <header className={styles["drawerHeader"]}>
          <div>
            <p className={styles["eyebrow"]}>Web Fullscreen</p>
            <h1 className={styles["title"]}>GPS</h1>
            <p className={styles["gpsStatus"]} data-tone={appStore.geoStore.statusTone}>
              {appStore.geoStore.statusText}
            </p>
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
          <GpsControls appStore={appStore} />
        </div>
      </aside>

      <button
        className={styles["backdrop"]}
        type="button"
        aria-label="Close controls"
        data-open={drawerOpen || alertMenuOpen ? "1" : "0"}
        onClick={() => {
          setDrawerOpen(false);
          setAlertMenuOpen(false);
        }}
      />
    </main>
  );
});
