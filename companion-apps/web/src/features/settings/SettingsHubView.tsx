import { observer } from "mobx-react-lite";
import { useState } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { type TFunction, useT } from "../../i18n/useT.js";
import { ImportDiagnosticsView } from "./diagnostics/ImportDiagnosticsView.js";
import { RoutingDiagnosticsView } from "./diagnostics/RoutingDiagnosticsView.js";
import { RoutePlannerSettingsView } from "./RoutePlannerSettingsView.js";
import { RouteDetailView } from "./routes/RouteDetailView.js";
import { RoutesSettingsView } from "./routes/RoutesSettingsView.js";
import styles from "./SettingsHubView.module.css";
import { ActivitySettingsSection } from "./sections/ActivitySettingsSection.js";
import { LocaleSettingsSection } from "./sections/LocaleSettingsSection.js";

type SubScreen =
  | "hub"
  | "routes"
  | "planner"
  | "diagnostics"
  | "routingDiagnostics"
  | "routeDetail";

type Props = { store: RootStore };

export const SettingsHubView = observer(({ store }: Props) => {
  const t = useT(store);
  const [screen, setScreen] = useState<SubScreen>("hub");
  const [detailItemId, setDetailItemId] = useState<string | null>(null);

  const gitTimeStr = import.meta.env.VITE_APP_GIT_TIME;
  const formattedGitTime = gitTimeStr
    ? new Date(gitTimeStr).toLocaleString(store.settingsStore.settings.language, {
        dateStyle: "short",
        timeStyle: "short",
      })
    : undefined;

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
    <div className={styles.overlay}>
      <div className={styles.content}>
        <header className={styles.header}>
          <button
            type="button"
            className={styles.iconBtn}
            aria-label={t("common.close")}
            onClick={back}
          >
            ‹
          </button>
          <h1 style={{ margin: 0, fontSize: 18 }}>{titleFor(screen, t)}</h1>
          <button
            type="button"
            className={styles.iconBtn}
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
            <div className={styles.section}>
              <button type="button" className={styles.row} onClick={() => setScreen("routes")}>
                <div style={{ flex: 1, textAlign: "start" }}>
                  <div className={styles.title}>{t("settings.hub.routes")}</div>
                  <div className={styles.subtitle}>{t("settings.hub.routes.subtitle")}</div>
                </div>
                <span aria-hidden>›</span>
              </button>
              <button type="button" className={styles.row} onClick={() => setScreen("planner")}>
                <div style={{ flex: 1, textAlign: "start" }}>
                  <div className={styles.title}>{t("settings.hub.routePlanner")}</div>
                  <div className={styles.subtitle}>{t("settings.hub.routePlanner.subtitle")}</div>
                </div>
                <span aria-hidden>›</span>
              </button>
              <button type="button" className={styles.row} onClick={() => setScreen("diagnostics")}>
                <div style={{ flex: 1, textAlign: "start" }}>
                  <div className={styles.title}>{t("settings.hub.importDiagnostics")}</div>
                  <div className={styles.subtitle}>
                    {t("settings.hub.importDiagnostics.subtitle")}
                  </div>
                </div>
                <span aria-hidden>›</span>
              </button>
              <button
                type="button"
                className={styles.row}
                onClick={() => setScreen("routingDiagnostics")}
              >
                <div style={{ flex: 1, textAlign: "start" }}>
                  <div className={styles.title}>Routing Diagnostics</div>
                  <div className={styles.subtitle}>View recorded routing debug sessions</div>
                </div>
                <span aria-hidden>›</span>
              </button>
              <button type="button" className={styles.row} onClick={() => store.goDebugger()}>
                <div style={{ flex: 1, textAlign: "start" }}>
                  <div className={styles.title}>Diagnostics Debugger</div>
                  <div className={styles.subtitle}>
                    Import and visualize diagnostic files from any device
                  </div>
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
        {screen === "routingDiagnostics" ? <RoutingDiagnosticsView store={store} /> : null}
        {screen === "routeDetail" && detailItemId ? (
          <RouteDetailView
            store={store}
            itemId={detailItemId}
            onClose={() => setScreen("routes")}
          />
        ) : null}
        <div className={styles.version}>
          v{import.meta.env.VITE_APP_VERSION} ({import.meta.env.VITE_APP_GIT_HASH})
          {formattedGitTime && ` (${formattedGitTime})`}
        </div>
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
    case "routingDiagnostics":
      return "Routing Diagnostics";
    case "routeDetail":
      return "Route Detail";
  }
}
