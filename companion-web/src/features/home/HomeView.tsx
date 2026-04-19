import { reaction } from "mobx";
import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { selectedAlternative } from "../../domain/models.js";
import { ActiveGuidanceCard } from "./ActiveGuidanceCard.js";
import { MapSurface } from "./MapSurface.js";
import { RouteSuggestionsCard } from "./RouteSuggestionsCard.js";
import { SearchPanel } from "./SearchPanel.js";

type Props = { store: RootStore };

export const HomeView = observer(({ store }: Props) => {
  const lastPreviewTickRef = useRef(0);
  const lastStartTickRef = useRef(0);

  // Camera follow: re-frame whenever the visible route changes, mode changes,
  // or we get our first GPS fix (so the empty map snaps to the rider).
  useEffect(() => {
    return reaction(
      () => ({
        homeMode: store.guidanceStore.homeMode,
        selectedId: store.planningStore.preview.selectedAlternativeID,
        revision: store.planningStore.preview.routeRevision,
        location: store.locationStore.currentLocation,
      }),
      () => refreshCameraForCurrentMode(store),
      { fireImmediately: true },
    );
  }, [store]);

  // Pending-home-presentation reveal hook.
  useEffect(() => {
    return reaction(
      () => store.historyStore.homePreviewRequestTick,
      (tick) => {
        if (tick === lastPreviewTickRef.current) return;
        lastPreviewTickRef.current = tick;
        store.consumePendingPresentationOnReveal();
      },
    );
  }, [store]);

  // Auto-start on request.
  useEffect(() => {
    return reaction(
      () => store.historyStore.homeStartRequestTick,
      (tick) => {
        if (tick === lastStartTickRef.current) return;
        lastStartTickRef.current = tick;
        store.guidanceStore.startSelectedRoute();
      },
    );
  }, [store]);

  return (
    <>
      <MapSurface store={store} />
      <TopOverlay store={store} />
      <BottomOverlay store={store} />
      <RecenterButton store={store} />
      <LocationBanner store={store} />
    </>
  );
});

const LocationBanner = observer(({ store }: { store: RootStore }) => {
  const loc = store.locationStore;
  if (store.locationBannerDismissed) return null;
  if (loc.permission === "granted" || loc.promptShown) return null;
  return (
    <div className="overlay-top" style={{ top: "calc(env(safe-area-inset-top) + 78px)" }}>
      <div className="location-banner">
        <div className="location-banner__text">
          Allow location access so routes start from where you are.
        </div>
        <div className="location-banner__actions">
          <button
            type="button"
            className="location-banner__button location-banner__button--ghost"
            onClick={() => store.dismissLocationBanner()}
          >
            Not now
          </button>
          <button
            type="button"
            className="location-banner__button location-banner__button--primary"
            onClick={() => store.requestLocationFromBanner()}
          >
            Allow
          </button>
        </div>
      </div>
    </div>
  );
});

const TopOverlay = observer(({ store }: { store: RootStore }) => {
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
    return (
      <div className="overlay-top">
        <div className="card guidance-header">
          <div className="guidance-header__text">
            <div className="list-row__title">{guidance.activeNavigationTitle}</div>
            <div className="list-row__subtitle">{guidance.activeNavigationSubtitle}</div>
          </div>
          <button
            type="button"
            className="compass-button"
            aria-label="North indicator"
            onClick={() => guidance.handleCompassTap()}
            onDoubleClick={() => guidance.handleCompassDoubleTap()}
          >
            {guidance.compassMode === "northLocked"
              ? "🧭"
              : guidance.compassMode === "northPreview"
                ? "↑"
                : "◎"}
          </button>
        </div>
      </div>
    );
  }
  return (
    <div className="overlay-top" ref={containerRef}>
      <div style={{ display: "flex", gap: 10 }}>
        <div className="search-bar" style={{ flex: 1 }}>
          <span aria-hidden>🔍</span>
          <input
            type="text"
            placeholder="Where to?"
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
        <button
          type="button"
          className="icon-button"
          aria-label="Settings"
          onClick={() => store.goSettings()}
        >
          ⚙
        </button>
      </div>
      {planning.isSearchOpen ? <SearchPanel store={store} /> : null}
    </div>
  );
});

const BottomOverlay = observer(({ store }: { store: RootStore }) => {
  const planning = store.planningStore;
  const guidance = store.guidanceStore;
  const status = planning.planningStatus ?? planning.importActivityStatus;
  if (guidance.homeMode === "phoneGuidance") {
    return (
      <div className="overlay-bottom">
        <ActiveGuidanceCard store={store} />
      </div>
    );
  }
  if (status) {
    return (
      <div className="overlay-bottom">
        <div className="card">
          <div className="list-row__title">Working on route</div>
          <div className="list-row__subtitle">{status}</div>
        </div>
      </div>
    );
  }
  if (planning.preview.alternatives.length > 0) {
    return (
      <div className="overlay-bottom">
        <RouteSuggestionsCard store={store} />
      </div>
    );
  }
  return null;
});

const RecenterButton = observer(({ store }: { store: RootStore }) => {
  const loc = store.locationStore;
  if (store.guidanceStore.homeMode !== "planning") return null;

  // While we are waiting for the first GPS fix, show a spinner in the slot
  // instead of the recenter glyph — even if the camera has not been moved yet.
  if (loc.isWaitingForFirstFix) {
    return (
      <div
        className="icon-button recenter-button"
        role="status"
        aria-label="Locating"
        title="Finding your location…"
      >
        <span className="spinner" aria-hidden />
      </div>
    );
  }

  if (loc.lastError === "denied") {
    return (
      <button
        type="button"
        className="icon-button recenter-button"
        aria-label="Location blocked"
        title="Location is blocked. Enable it in your browser settings to plan from your real position."
        onClick={() => store.dismissLocationBanner()}
      >
        📍
      </button>
    );
  }

  if (!store.mapCameraStore.needsRecenter) return null;
  return (
    <button
      type="button"
      className="icon-button recenter-button"
      aria-label="Recenter map"
      onClick={() => refreshCameraForCurrentMode(store)}
    >
      ⌖
    </button>
  );
});

function refreshCameraForCurrentMode(store: RootStore): void {
  const selected = selectedAlternative(store.planningStore.preview);
  if (selected && selected.normalizedPackage.geometry.length > 0) {
    store.mapCameraStore.fitBounds(selected.normalizedPackage.geometry);
    return;
  }
  store.mapCameraStore.setCenter(store.guidanceStore.riderLocation, 12, 0);
}
