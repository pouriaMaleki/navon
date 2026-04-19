import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import { summaryLine } from "../../domain/models.js";

type Props = { store: RootStore };

export const ActiveGuidanceCard = observer(({ store }: Props) => {
  const guidance = store.guidanceStore;
  const route = guidance.guidanceRoute;
  if (!route) return null;
  return (
    <div className="card">
      <div className="list-row__title">{route.summary.destinationLabel ?? "Guidance active"}</div>
      <div className="list-row__subtitle" style={{ marginBottom: 12 }}>
        {guidance.nextInstructionLine ?? summaryLine(route)}
      </div>
      <button type="button" className="danger-button" onClick={() => guidance.stopGuidance()}>
        Stop
      </button>
    </div>
  );
});
