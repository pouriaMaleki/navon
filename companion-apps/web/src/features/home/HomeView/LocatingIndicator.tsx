import { observer } from "mobx-react-lite";
import type { RootStore } from "../../../app/RootStore.js";
import styles from "./LocatingIndicator.module.css";

type Props = { store: RootStore };

/**
 * Planning-only status indicator: shows a spinner while we're waiting
 * for the first GPS fix and a "blocked" glyph when the user denied
 * location. Sits to the LEFT of the right rail so it doesn't clobber
 * the persistent settings/compass column. The recenter affordance is
 * folded into the compass icon itself.
 */
export const LocatingIndicator = observer(({ store }: Props) => {
  if (store.planningStore.isSearchOpen) return null;
  if (store.guidanceStore.homeMode !== "planning") return null;
  const loc = store.locationStore;
  if (loc.isWaitingForFirstFix) {
    return (
      <div
        className={[styles.railIcon, styles.locatingIndicator].join(" ")}
        role="status"
        aria-label="Locating"
        title="Finding your location…"
      >
        <span className={styles.spinner} aria-hidden />
      </div>
    );
  }
  if (loc.lastError === "denied") {
    return (
      <button
        type="button"
        className={[styles.railIcon, styles.locatingIndicator].join(" ")}
        aria-label="Location blocked"
        title="Location is blocked. Enable it in your browser settings to plan from your real position."
        onClick={() => store.dismissLocationBanner()}
      >
        📍
      </button>
    );
  }
  return null;
});
