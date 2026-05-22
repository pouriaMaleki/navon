import { observer } from "mobx-react-lite";
import type { RootStore } from "../../../app/RootStore.js";
import { refreshCameraForCurrentMode } from "../refreshCamera.js";
import styles from "./RightSideRail.module.css";

type Props = { store: RootStore };

/**
 * iOS-parity right-side rail. Renders `topRightIconStack` items in a
 * vertical column at a fixed Y offset. Order, top → bottom: settings →
 * compass/north-up → device chip (only when paired). The compass tap
 * recentres the camera (single tap = north-up; double-tap = lock north-up,
 * matching the Rust impl).
 */
export const RightSideRail = observer(({ store }: Props) => {
  if (store.planningStore.isSearchOpen) return null;
  const guidance = store.guidanceStore;
  return (
    <div className={styles.railRight}>
      {guidance.topRightIconStack.map((item) => {
        if (item === "settings") {
          return (
            <button
              key="settings"
              type="button"
              className={styles.railIcon}
              aria-label="Settings"
              onClick={() => store.goSettings()}
            >
              ⚙
            </button>
          );
        }
        if (item === "compass") {
          return (
            <button
              key="compass"
              type="button"
              className={styles.railIcon}
              aria-label="Recenter map"
              title="Tap = recenter / north-up; double-tap = lock north-up"
              onClick={() => {
                if (guidance.homeMode === "phoneGuidance") {
                  guidance.handleCompassTap();
                } else {
                  refreshCameraForCurrentMode(store);
                }
              }}
              onDoubleClick={() => guidance.handleCompassDoubleTap()}
            >
              {guidance.compassMode === "northLocked"
                ? "🧭"
                : guidance.compassMode === "northPreview"
                  ? "↑"
                  : "◎"}
            </button>
          );
        }
        // `deviceChip` — web has no pairing flow yet, so this branch is
        // unreachable today but kept for shape parity with iOS.
        return null;
      })}
    </div>
  );
});
