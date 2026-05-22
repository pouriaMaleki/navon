import { observer } from "mobx-react-lite";
import type { RootStore } from "../../../app/RootStore.js";
import { formatSpeedLabel } from "../../../integrations/speed.js";
import styles from "./SpeedBadge.module.css";

type Props = { store: RootStore };

// Spec: render speed whenever the rider is moving (with or without an active
// route). The "moving" signal is the heading-trail's `travelHeadingDegrees`
// — same threshold the camera uses to enter routing-anchor mode — so the
// badge appears/disappears in lock-step with the bottom-quarter anchor.
export const SpeedBadge = observer(({ store }: Props) => {
  const moving = store.locationStore.travelHeadingDegrees !== undefined;
  const inGuidance = store.guidanceStore.homeMode === "phoneGuidance";
  if (!moving && !inGuidance) return null;
  // Hide whenever the route-suggestions card covers the bottom of the screen
  if (store.guidanceStore.isExploringAlternativesFromGuidance) return null;
  if (
    store.guidanceStore.homeMode === "planning" &&
    store.planningStore.preview.alternatives.length > 0
  )
    return null;
  const unit = store.settingsStore.settings.speedUnit;
  return (
    <output className={styles.speedBadge} aria-label="Current speed">
      {formatSpeedLabel(store.locationStore.currentSpeedMps, unit)}
    </output>
  );
});
