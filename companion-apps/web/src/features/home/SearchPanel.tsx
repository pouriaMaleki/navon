import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { useT } from "../../i18n/useT.js";
import styles from "./SearchPanel.module.css";

type Props = { store: RootStore };

export const SearchPanel = observer(({ store }: Props) => {
  const t = useT(store);
  const planning = store.planningStore;
  const showRecents = planning.query.trim().length === 0;
  const recents = store.historyStore.routeHistoryItems.slice(0, planning.visibleRecentCount);
  const suggestions = planning.suggestions.slice(0, planning.visibleSuggestionCount);

  // Auto-close: if the input is empty and we have nothing to show, collapse the dropdown.
  useEffect(() => {
    if (showRecents && recents.length === 0 && !planning.isResolvingUrl) {
      planning.closeSearch();
    }
  }, [planning, showRecents, recents.length]);

  if (planning.isResolvingUrl) {
    return (
      <div className={styles.panel} data-testid="search-panel" role="status">
        <div className={styles.row} style={{ alignItems: "center", gap: 12 }}>
          <span className={styles.spinner} aria-hidden />
          <div style={{ flex: 1 }}>
            <div className={styles.title}>Resolving link…</div>
            <div className={styles.subtitle}>
              Following the URL to a destination. This can take a couple of seconds.
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (planning.urlResolveError) {
    return (
      <div className={styles.panel} data-testid="search-panel" role="alert">
        <div className={styles.row}>
          <div style={{ flex: 1 }}>
            <div className={styles.title}>Couldn't open that link</div>
            <div className={styles.subtitle}>{planning.urlResolveError}</div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={styles.panel} data-testid="search-panel" role="listbox">
      {showRecents ? (
        recents.length === 0 ? (
          <div className={styles.empty}>No recent routes yet. Try typing a destination.</div>
        ) : (
          recents.map((item) => (
            <button
              key={item.id}
              type="button"
              className={styles.row}
              onClick={() => void store.activateRouteHistoryItem(item, false)}
            >
              <div style={{ flex: 1, textAlign: "start" }}>
                <div className={styles.title}>{item.title}</div>
                {item.subtitle ? <div className={styles.subtitle}>{item.subtitle}</div> : null}
                <div className={styles.subtitle} style={{ opacity: 0.6 }}>
                  {item.sourceLabel}
                </div>
              </div>
            </button>
          ))
        )
      ) : suggestions.length === 0 ? (
        <div className={styles.empty}>{t("home.noSuggestions")}</div>
      ) : (
        suggestions.map((suggestion, idx) => (
          <button
            key={suggestion.id}
            type="button"
            className={styles.row}
            data-testid="search-row"
            onClick={() => {
              store.recordRecentDestination(suggestion.coordinate, suggestion.title);
              planning.selectSuggestion(suggestion);
            }}
            onMouseEnter={() => {
              if (idx === suggestions.length - 1) {
                planning.loadMoreSuggestionsIfNeeded(suggestion);
              }
            }}
          >
            <div style={{ flex: 1, textAlign: "start" }}>
              <div className={styles.title}>{suggestion.title}</div>
              {suggestion.subtitle ? (
                <div className={styles.subtitle}>{suggestion.subtitle}</div>
              ) : null}
            </div>
          </button>
        ))
      )}
    </div>
  );
});
