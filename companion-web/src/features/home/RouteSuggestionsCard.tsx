import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import {
  ROUTE_SOURCE_MODE_DISPLAY_NAME,
  selectedAlternative,
  summaryLine,
} from "../../domain/models.js";
import { useT } from "../../i18n/useT.js";

type Props = { store: RootStore };

export const RouteSuggestionsCard = observer(({ store }: Props) => {
  const t = useT(store);
  const planning = store.planningStore;
  const guidance = store.guidanceStore;
  const exploring = guidance.isExploringAlternativesFromGuidance;
  const limit = store.settingsStore.plannerPreferences.suggestionMode === "bestOnly" ? 1 : 3;
  const alternatives = planning.preview.alternatives.slice(0, limit);
  const selectedId = selectedAlternative(planning.preview)?.id;
  const showSourceControl =
    !planning.isPreviewLockedToImportedRoute() && planning.availableSourceModes.length > 1;
  const sourceMode = planning.currentSourceMode;

  return (
    <div className="card suggestions-card">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div className="list-row__title">{t("home.routeOptions")}</div>
        <button
          type="button"
          onClick={() =>
            exploring ? guidance.cancelAlternativesExploration() : planning.clearPreview()
          }
          style={{ color: "var(--fg-soft)", fontWeight: 600 }}
        >
          {t("common.close")}
        </button>
      </div>
      {exploring ? (
        <button
          type="button"
          className="alternative alternative--selected"
          onClick={() => guidance.cancelAlternativesExploration()}
        >
          <div style={{ textAlign: "start" }}>
            <div className="list-row__title">{t("home.continueOnCurrentRoute")}</div>
          </div>
          <span aria-hidden>✓</span>
        </button>
      ) : null}
      {planning.preview.planningNotice ? (
        <div className="notice">{planning.preview.planningNotice}</div>
      ) : null}
      {showSourceControl ? (
        <div className="source-picker" role="tablist">
          {planning.availableSourceModes.map((mode) => (
            <button
              key={mode}
              type="button"
              role="tab"
              aria-selected={sourceMode === mode}
              className={sourceMode === mode ? "active" : ""}
              onClick={() => {
                planning.setSourceMode(mode);
                store.settingsStore.updatePlannerPreferences({ defaultSourceMode: mode });
                void planning.planRoute();
              }}
            >
              {ROUTE_SOURCE_MODE_DISPLAY_NAME[mode]}
            </button>
          ))}
        </div>
      ) : null}
      {alternatives.map((alt) => {
        const isSelected = alt.id === selectedId;
        return (
          <button
            key={alt.id}
            type="button"
            className={`alternative${isSelected ? " alternative--selected" : ""}`}
            onClick={() => planning.selectAlternative(alt.id)}
          >
            <div style={{ textAlign: "start" }}>
              <div className="list-row__title">{alt.title}</div>
              {/* iOS parity: drop the redundant subtitle (now empty
                  after the friendlyAlternativeLabel rename). The row
                  collapses to title + km/min summary so it's a tight
                  two-line block. */}
              {alt.subtitle ? <div className="list-row__subtitle">{alt.subtitle}</div> : null}
              <div className="list-row__subtitle">{summaryLine(alt.normalizedPackage)}</div>
            </div>
            {isSelected ? <span aria-hidden>✓</span> : null}
          </button>
        );
      })}
      <button
        type="button"
        className="primary-button"
        onClick={() => store.guidanceStore.startSelectedRoute()}
        disabled={alternatives.length === 0}
      >
        {t("home.start")}
      </button>
    </div>
  );
});
