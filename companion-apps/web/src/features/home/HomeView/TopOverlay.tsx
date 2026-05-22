import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../../app/RootStore.js";
import { useT } from "../../../i18n/useT.js";
import { SearchPanel } from "../SearchPanel.js";
import styles from "./TopOverlay.module.css";

type Props = { store: RootStore };

export const TopOverlay = observer(({ store }: Props) => {
  const t = useT(store);
  const planning = store.planningStore;
  const guidance = store.guidanceStore;
  const containerRef = useRef<HTMLDivElement | null>(null);

  // Click outside the search container closes the dropdown.
  useEffect(() => {
    if (!planning.isSearchOpen || guidance.homeMode !== "planning") return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (!target || !containerRef.current) return;
      if (!containerRef.current.contains(target)) {
        planning.closeSearch();
      }
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    return () => window.removeEventListener("pointerdown", onPointerDown, true);
  }, [planning, guidance.homeMode, planning.isSearchOpen]);

  // Escape closes the dropdown when it's open.
  useEffect(() => {
    if (!planning.isSearchOpen) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") planning.closeSearch();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [planning, planning.isSearchOpen]);

  if (guidance.homeMode === "phoneGuidance") {
    if (guidance.isExploringAlternativesFromGuidance) {
      // Show a read-only "Where to" bar with the active destination so the
      // rider always sees where they're headed while comparing alternatives.
      const dest = planning.query || guidance.activeNavigationTitle;
      return (
        <div className={styles.overlayTop}>
          <div className={styles.searchBar}>
            <span aria-hidden>🔍</span>
            <input
              type="text"
              readOnly
              value={dest}
              onChange={() => {}}
              aria-label="Active destination"
            />
          </div>
        </div>
      );
    }
    // iOS-parity routing top card. Three text lines, no icons (every
    // icon lives in the persistent rails). The eye hits the metric
    // first on every line — distance-first next-turn, then
    // "X km to <destination>", then "Y min remaining".
    const headline = guidance.nextInstructionLine ?? guidance.activeNavigationTitle;
    return (
      <div className={styles.overlayTop}>
        <div className={[styles.card, styles.guidanceHeader].join(" ")}>
          <div className={styles.guidanceText}>
            <div className={styles.title}>{headline}</div>
            {guidance.distanceToDestinationLine && (
              <div className={styles.subtitle}>{guidance.distanceToDestinationLine}</div>
            )}
            {guidance.minutesRemainingLine && (
              <div className={styles.subtitle}>{guidance.minutesRemainingLine}</div>
            )}
          </div>
        </div>
      </div>
    );
  }
  // Planning: full-width where-to bar. Settings cog lives on the
  // right rail (iOS parity) — no longer inline with the search input.
  return (
    <div className={styles.overlayTop} ref={containerRef}>
      <div className={styles.searchBar}>
        <span aria-hidden>🔍</span>
        <input
          type="text"
          placeholder={t("home.whereTo")}
          value={planning.query}
          onChange={(event) => {
            planning.openSearch();
            void planning.updateQuery(event.target.value);
          }}
          onFocus={() => planning.openSearch()}
        />
        {(planning.query.length > 0 || planning.preview.alternatives.length > 0) && (
          <button
            type="button"
            aria-label="Clear destination"
            onClick={() => planning.clearPreview()}
          >
            ✕
          </button>
        )}
      </div>
      {planning.isSearchOpen ? <SearchPanel store={store} /> : null}
    </div>
  );
});
