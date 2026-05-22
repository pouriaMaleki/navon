import { observer } from "mobx-react-lite";
import type { RootStore } from "../../../app/RootStore.js";
import styles from "./LocationBanner.module.css";

type Props = { store: RootStore };

export const LocationBanner = observer(({ store }: Props) => {
  const loc = store.locationStore;
  if (store.locationBannerDismissed) return null;
  if (loc.permission === "granted" || loc.promptShown) return null;
  return (
    <div className={styles.overlayTop} style={{ top: "calc(env(safe-area-inset-top) + 78px)" }}>
      <div className={styles.locBanner}>
        <div className={styles.locText}>
          Allow location access so routes start from where you are.
        </div>
        <div className={styles.locActions}>
          <button
            type="button"
            className={styles.locBtnGhost}
            onClick={() => store.dismissLocationBanner()}
          >
            Not now
          </button>
          <button
            type="button"
            className={styles.locBtnPrimary}
            onClick={() => store.requestLocationFromBanner()}
          >
            Allow
          </button>
        </div>
      </div>
    </div>
  );
});
