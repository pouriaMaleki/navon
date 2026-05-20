import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import { ROUTE_SOURCE_MODE_DISPLAY_NAME, summaryLine } from "../../domain/models.js";
import { useT } from "../../i18n/useT.js";
import styles from "./RouteSuggestionsCard.module.css";

type Props = { store: RootStore };

export const RouteSuggestionsCard = observer(({ store }: Props) => {
  const t = useT(store);
  const planning = store.planningStore;
  const guidance = store.guidanceStore;
  const exploring = guidance.isExploringAlternativesFromGuidance;
  const limit = store.settingsStore.plannerPreferences.suggestionMode === "bestOnly" ? 1 : 3;
  const alternatives = planning.preview.alternatives.slice(0, limit);
  const selectedId = guidance.selectedAlternativeIDForDisplay;
  const showSourceControl =
    !planning.isPreviewLockedToImportedRoute() && planning.availableSourceModes.length > 1;
  const sourceMode = planning.currentSourceMode;

  return (
    <div className={styles.card}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div className={styles.title}>{t("home.routeOptions")}</div>
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
          className={[styles.alternative, selectedId === undefined && styles.selected]
            .filter(Boolean)
            .join(" ")}
          onClick={() => guidance.deselectForExploration()}
        >
          <div style={{ textAlign: "start" }}>
            <div className={styles.title}>{t("home.continueOnCurrentRoute")}</div>
          </div>
          {selectedId === undefined ? <span aria-hidden>✓</span> : null}
        </button>
      ) : null}
      {planning.preview.planningNotice ? (
        <div className={styles.notice}>{planning.preview.planningNotice}</div>
      ) : null}
      {showSourceControl ? (
        <div className={styles.sourcePicker} role="tablist">
          {planning.availableSourceModes.map((mode) => (
            <button
              key={mode}
              type="button"
              role="tab"
              aria-selected={sourceMode === mode}
              className={sourceMode === mode ? styles.sourceBtnActive : styles.sourceBtn}
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
            className={[styles.alternative, isSelected && styles.selected]
              .filter(Boolean)
              .join(" ")}
            onClick={() =>
              exploring
                ? guidance.selectAlternativeForExploration(alt.id)
                : planning.selectAlternative(alt.id)
            }
          >
            <div style={{ textAlign: "start" }}>
              <div className={styles.title}>{alt.title}</div>
              {/* iOS parity: drop the redundant subtitle (now empty
                  after the friendlyAlternativeLabel rename). The row
                  collapses to title + km/min summary so it's a tight
                  two-line block. */}
              {alt.subtitle ? <div className={styles.subtitle}>{alt.subtitle}</div> : null}
              <div className={styles.subtitle}>{summaryLine(alt.normalizedPackage)}</div>
            </div>
            {isSelected ? <span aria-hidden>✓</span> : null}
          </button>
        );
      })}
      <button
        type="button"
        className={styles.primaryBtn}
        onClick={() => {
          if (exploring && selectedId === undefined) {
            guidance.cancelAlternativesExploration();
          } else {
            store.guidanceStore.startSelectedRoute();
          }
        }}
        disabled={alternatives.length === 0 && !exploring}
      >
        {t("home.start")}
      </button>
    </div>
  );
});
