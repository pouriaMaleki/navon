import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import { useT } from "../../i18n/useT.js";

type Props = { store: RootStore };

// The destination + remaining + ETA all live in the top card now (see
// `guidanceSubtitleLine`). The bottom slot is intentionally minimal: an
// optional off-route alert pill stacked above a single floating Stop button.
export const ActiveGuidanceCard = observer(({ store }: Props) => {
  const t = useT(store);
  const guidance = store.guidanceStore;
  const route = guidance.guidanceRoute;
  if (!route) return null;

  const offRouteLabel = guidance.offRouteLabel;

  return (
    <div className="active-guidance-floating">
      {offRouteLabel && (
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
