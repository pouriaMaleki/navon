import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";

type Props = { store: RootStore };

export const ActiveGuidanceCard = observer(({ store }: Props) => {
  const guidance = store.guidanceStore;
  const route = guidance.guidanceRoute;
  if (!route) return null;

  const offRouteLabel = guidance.offRouteLabel;

  return (
    <div className="card">
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
            marginBottom: 8,
            fontSize: 14,
          }}
        >
          {offRouteLabel}
        </div>
      )}
      {guidance.nextInstructionLine && (
        <div className="list-row__title" style={{ marginBottom: 4 }}>
          {guidance.nextInstructionLine}
        </div>
      )}
      <div className="list-row__subtitle" style={{ marginBottom: 12 }}>
        {guidance.activeNavigationSubtitle}
      </div>
      <button type="button" className="danger-button" onClick={() => guidance.stopGuidance()}>
        Stop
      </button>
    </div>
  );
});
