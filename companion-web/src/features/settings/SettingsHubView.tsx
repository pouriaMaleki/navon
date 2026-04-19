import { observer } from "mobx-react-lite";
import { useState } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { ImportDiagnosticsView } from "./ImportDiagnosticsView.js";
import { RouteDetailView } from "./RouteDetailView.js";
import { RoutePlannerSettingsView } from "./RoutePlannerSettingsView.js";
import { RoutesSettingsView } from "./RoutesSettingsView.js";

type SubScreen = "hub" | "routes" | "planner" | "diagnostics" | "routeDetail";

type Props = { store: RootStore };

export const SettingsHubView = observer(({ store }: Props) => {
  const [screen, setScreen] = useState<SubScreen>("hub");
  const [detailItemId, setDetailItemId] = useState<string | null>(null);

  const back = () => {
    if (screen === "routeDetail") {
      setScreen("routes");
      return;
    }
    if (screen !== "hub") {
      setScreen("hub");
      return;
    }
    store.goHome();
  };

  return (
    <div className="settings-overlay">
      <div className="settings-overlay__content">
        <header className="settings-overlay__header">
          <button type="button" className="icon-button" aria-label="Back" onClick={back}>
            ‹
          </button>
          <h1 style={{ margin: 0, fontSize: 18 }}>{titleFor(screen)}</h1>
          <button
            type="button"
            className="icon-button"
            aria-label="Close settings"
            onClick={() => store.goHome()}
          >
            ✕
          </button>
        </header>

        {screen === "hub" ? (
          <div className="settings-section">
            <button type="button" className="list-row" onClick={() => setScreen("routes")}>
              <div style={{ flex: 1, textAlign: "left" }}>
                <div className="list-row__title">Routes</div>
                <div className="list-row__subtitle">Recent routes and GPX import</div>
              </div>
              <span aria-hidden>›</span>
            </button>
            <button type="button" className="list-row" onClick={() => setScreen("planner")}>
              <div style={{ flex: 1, textAlign: "left" }}>
                <div className="list-row__title">Route Planner</div>
                <div className="list-row__subtitle">Default source, suggestions, HSL key</div>
              </div>
              <span aria-hidden>›</span>
            </button>
            <button type="button" className="list-row" onClick={() => setScreen("diagnostics")}>
              <div style={{ flex: 1, textAlign: "left" }}>
                <div className="list-row__title">Import Diagnostics</div>
                <div className="list-row__subtitle">Failed shared imports</div>
              </div>
              <span aria-hidden>›</span>
            </button>
          </div>
        ) : null}

        {screen === "routes" ? (
          <RoutesSettingsView
            store={store}
            onOpenDetail={(id) => {
              setDetailItemId(id);
              setScreen("routeDetail");
            }}
          />
        ) : null}
        {screen === "planner" ? <RoutePlannerSettingsView store={store} /> : null}
        {screen === "diagnostics" ? <ImportDiagnosticsView store={store} /> : null}
        {screen === "routeDetail" && detailItemId ? (
          <RouteDetailView
            store={store}
            itemId={detailItemId}
            onClose={() => setScreen("routes")}
          />
        ) : null}
      </div>
    </div>
  );
});

function titleFor(screen: SubScreen): string {
  switch (screen) {
    case "hub":
      return "Settings";
    case "routes":
      return "Routes";
    case "planner":
      return "Route Planner";
    case "diagnostics":
      return "Import Diagnostics";
    case "routeDetail":
      return "Route Detail";
  }
}
