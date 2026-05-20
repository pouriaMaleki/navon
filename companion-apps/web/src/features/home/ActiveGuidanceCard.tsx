import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { useT } from "../../i18n/useT.js";
import styles from "./ActiveGuidanceCard.module.css";

type Props = { store: RootStore };

// The destination + remaining + ETA all live in the top card now (see
// `guidanceSubtitleLine`). The bottom slot is intentionally minimal: an
// optional off-route alert pill stacked above a single floating Stop button.
// When the rerouting throttle is holding back an auto-reroute, surface a
// "Waiting … Reroute now" pill so the rider isn't stranded watching the
// app silently sit on stale guidance.
export const ActiveGuidanceCard = observer(({ store }: Props) => {
  const t = useT(store);
  const guidance = store.guidanceStore;
  const route = guidance.guidanceRoute;

  // Tick once a second so the countdown updates while waiting.
  const [, setTick] = useState(0);
  useEffect(() => {
    if (guidance.reroutingDelayedUntilMs === undefined) return;
    const id = setInterval(() => setTick((t) => t + 1), 250);
    return () => clearInterval(id);
  }, [guidance.reroutingDelayedUntilMs]);

  if (!route) return null;

  const offRouteLabel = guidance.offRouteLabel;
  const now = Date.now();
  const waitingForReroute = guidance.isWaitingToReroute(now);
  const secondsRemaining = waitingForReroute
    ? Math.max(0, Math.ceil(((guidance.reroutingDelayedUntilMs ?? now) - now) / 1000))
    : 0;

  return (
    <div className={styles.floating}>
      {waitingForReroute && (
        <div className={styles.alert} style={{ background: "#12A3A3" }}>
          <span className={styles.alertText}>Waiting to reroute • {secondsRemaining}s</span>
          <button
            type="button"
            className={styles.rerouteBtn}
            onClick={() => guidance.requestManualReroute()}
          >
            Reroute now
          </button>
        </div>
      )}
      {offRouteLabel && !waitingForReroute && (
        <div
          className={styles.offRouteAlert}
          style={{ background: guidance.rerouteRequested ? "#12A3A3" : "#D7FF3F" }}
        >
          {offRouteLabel}
        </div>
      )}
      <button
        type="button"
        className={styles.dangerBtnFloating}
        onClick={() => guidance.stopGuidance()}
      >
        {t("home.stop")}
      </button>
    </div>
  );
});
