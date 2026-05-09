import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { useT } from "../../i18n/useT.js";

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
    <div className="active-guidance-floating">
      {waitingForReroute && (
        <div
          className="guidance-alert"
          style={{
            background: "#12A3A3",
            color: "#050B12",
            fontWeight: 700,
            padding: "8px 12px",
            borderRadius: 8,
            fontSize: 14,
            display: "flex",
            alignItems: "center",
            gap: 12,
          }}
        >
          <span style={{ flex: 1 }}>Waiting to reroute • {secondsRemaining}s</span>
          <button
            type="button"
            onClick={() => guidance.requestManualReroute()}
            style={{
              background: "#050B12",
              color: "#D7FF3F",
              border: "none",
              padding: "6px 10px",
              borderRadius: 6,
              fontWeight: 700,
              cursor: "pointer",
            }}
          >
            Reroute now
          </button>
        </div>
      )}
      {offRouteLabel && !waitingForReroute && (
        <div
          className="guidance-alert"
          style={{
            background: guidance.rerouteRequested ? "#12A3A3" : "#D7FF3F",
            color: "#050B12",
            fontWeight: 700,
            textAlign: "center",
            padding: "6px 12px",
            borderRadius: 8,
            fontSize: 14,
          }}
        >
          {offRouteLabel}
        </div>
      )}
      <button
        type="button"
        className="danger-button danger-button--floating"
        onClick={() => guidance.stopGuidance()}
      >
        {t("home.stop")}
      </button>
    </div>
  );
});
