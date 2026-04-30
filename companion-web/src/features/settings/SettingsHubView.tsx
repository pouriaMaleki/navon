import { observer } from "mobx-react-lite";
import { useState } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { useT, type TFunction } from "../../i18n/useT.js";
import { ActivitySettingsSection } from "./ActivitySettingsSection.js";
import { ImportDiagnosticsView } from "./ImportDiagnosticsView.js";
import { LocaleSettingsSection } from "./LocaleSettingsSection.js";
import { RouteDetailView } from "./RouteDetailView.js";
import { RoutePlannerSettingsView } from "./RoutePlannerSettingsView.js";
import { RoutesSettingsView } from "./RoutesSettingsView.js";

type SubScreen = "hub" | "routes" | "planner" | "diagnostics" | "routeDetail";

type Props = { store: RootStore };

export const SettingsHubView = observer(({ store }: Props) => {
  const t = useT(store);
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
          <button type="button" className="icon-button" aria-label={t("common.close")} onClick={back}>
            ‹
          </button>
          <h1 style={{ margin: 0, fontSize: 18 }}>{titleFor(screen, t)}</h1>
          <button
            type="button"
            className="icon-button"
            aria-label={t("common.close")}
            onClick={() => store.goHome()}
          >
            ✕
          </button>
        </header>

        {screen === "hub" ? (
          <>
            {/* UX spec lines 128-145: prevent screen off, allow GPS in
                background, audio cues, and live activity must appear at the
                TOP of the settings page in this exact order. */}
            <ActivitySettingsSection store={store} />
            <LocaleSettingsSection store={store} />
            <div className="settings-section">
              <button type="button" className="list-row" onClick={() => setScreen("routes")}>
                <div style={{ flex: 1, textAlign: "start" }}>
                  <div className="list-row__title">{t("settings.hub.routes")}</div>
                  <div className="list-row__subtitle">{t("settings.hub.routes.subtitle")}</div>
                </div>
                <span aria-hidden>›</span>
              </button>
              <button type="button" className="list-row" onClick={() => setScreen("planner")}>
                <div style={{ flex: 1, textAlign: "start" }}>
                  <div className="list-row__title">{t("settings.hub.routePlanner")}</div>
                  <div className="list-row__subtitle">{t("settings.hub.routePlanner.subtitle")}</div>
                </div>
                <span aria-hidden>›</span>
              </button>
              <button type="button" className="list-row" onClick={() => setScreen("diagnostics")}>
                <div style={{ flex: 1, textAlign: "start" }}>
                  <div className="list-row__title">{t("settings.hub.importDiagnostics")}</div>
                  <div className="list-row__subtitle">{t("settings.hub.importDiagnostics.subtitle")}</div>
                </div>
                <span aria-hidden>›</span>
              </button>
            </div>
          </>
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

function titleFor(screen: SubScreen, t: TFunction): string {
  switch (screen) {
    case "hub":
      return t("settings.hub.title");
    case "routes":
      return t("settings.hub.routes");
    case "planner":
      return t("settings.hub.routePlanner");
    case "diagnostics":
      return t("settings.hub.importDiagnostics");
    case "routeDetail":
      return "Route Detail";
  }
}
