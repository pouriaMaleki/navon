import { reaction } from "mobx";
import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { formatSpeedLabel } from "../../integrations/speed.js";
import { ActiveGuidanceCard } from "./ActiveGuidanceCard.js";
import { MapSurface } from "./MapSurface.js";
import { RouteSuggestionsCard } from "./RouteSuggestionsCard.js";
import { refreshCameraForCurrentMode } from "./refreshCamera.js";
import { SearchPanel } from "./SearchPanel.js";

type Props = { store: RootStore };

export const HomeView = observer(({ store }: Props) => {
  const lastPreviewTickRef = useRef(0);
  const lastStartTickRef = useRef(0);

  // Camera follow for PLANNING transitions only (picking a different
  // alternative / first GPS fix). During phoneGuidance the GuidanceStore's
  // emit path owns the camera (spec line 101 requires the route-segment
  // bearing, which this reaction used to overwrite with GPS heading).
  useEffect(() => {
    return reaction(
      () => ({
        homeMode: store.guidanceStore.homeMode,
        selectedId: store.planningStore.preview.selectedAlternativeID,
        revision: store.planningStore.preview.routeRevision,
        location: store.locationStore.currentLocation,
      }),
      () => {
        if (store.guidanceStore.homeMode === "phoneGuidance") return;
        refreshCameraForCurrentMode(store);
      },
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

  // Measure the bottom overlay (routing card / alternatives card) and feed
  // its height to MapCameraStore so the camera reserves that space —
  // otherwise fitBounds and follow-rider both render content under the
  // opaque card. Spec 84 says rider sits in the bottom quarter; "bottom"
  // here means visible map area, not absolute viewport.
  useEffect(() => {
    const update = () => {
      const el = document.querySelector(".overlay-bottom") as HTMLElement | null;
      const h = el ? el.getBoundingClientRect().height : 0;
      // Add the bottom safe-area-inset slack so the card's visual edge,
      // not its DOM box, defines the reserved area.
      store.mapCameraStore.setBottomReservedPx(h + 12);
    };
    update();
    const observer = new ResizeObserver(update);
    const el = document.querySelector(".overlay-bottom");
    if (el) observer.observe(el);
    // The overlay element is conditional (different cards for different
    // modes), so re-attach on every mode/preview change.
    const mo = new MutationObserver(() => {
      const next = document.querySelector(".overlay-bottom");
      if (next && next !== el) {
        observer.disconnect();
        observer.observe(next);
      }
      update();
    });
    mo.observe(document.body, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
      mo.disconnect();
    };
  }, [store]);

  return (
    <>
      <MapSurface store={store} />
      <TopOverlay store={store} />
      <BottomOverlay store={store} />
      <RecenterButton store={store} />
      <ZoomControls store={store} />
      <SpeedBadge store={store} />
      <LocationBanner store={store} />
    </>
  );
});

// Spec: render speed whenever the rider is moving (with or without an active
// route). The "moving" signal is the heading-trail's `travelHeadingDegrees`
// — same threshold the camera uses to enter routing-anchor mode — so the
// badge appears/disappears in lock-step with the bottom-quarter anchor.
const SpeedBadge = observer(({ store }: { store: RootStore }) => {
  const moving = store.locationStore.travelHeadingDegrees !== undefined;
  const inGuidance = store.guidanceStore.homeMode === "phoneGuidance";
  if (!moving && !inGuidance) return null;
  const unit = store.settingsStore.settings.speedUnit;
  return (
    <div className="speed-badge" aria-label="Current speed" role="status">
      {formatSpeedLabel(store.locationStore.currentSpeedMps, unit)}
    </div>
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
    // Top card: next-turn line as the headline, destination + remaining bundled
    // as the subtitle (single source of truth — the bottom just floats a stop
    // button). See GuidanceStore.guidanceSubtitleLine.
    const headline = guidance.nextInstructionLine ?? guidance.activeNavigationTitle;
    return (
      <div className="overlay-top">
        <div className="card guidance-header">
          <div className="guidance-header__text">
            <div className="list-row__title">{headline}</div>
            <div className="list-row__subtitle">{guidance.guidanceSubtitleLine}</div>
          </div>
          <button
            type="button"
            className="compass-button"
            aria-label="Find alternate routes"
            title="Find alternate routes from here"
            onClick={() => void store.exploreAlternateRoutes()}
          >
            ⇄
          </button>
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
  if (guidance.arrivalNotice) {
    return (
      <div className="overlay-bottom">
        <div className="card" role="status">
          <div className="list-row__title">{guidance.arrivalNotice}</div>
          <div className="list-row__subtitle">
            Routing finished. Tap a destination to plan again.
          </div>
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

const ZoomControls = observer(({ store }: { store: RootStore }) => {
  // Hide the on-map zoom controls while a search dropdown / location banner
  // is active, since they would visually crowd the top of the screen.
  if (store.planningStore.isSearchOpen) return null;
  return (
    <div className="zoom-controls">
      <button
        type="button"
        className="icon-button zoom-controls__button"
        aria-label="Zoom in"
        onClick={() => store.mapCameraStore.requestZoomDelta(1)}
      >
        +
      </button>
      <button
        type="button"
        className="icon-button zoom-controls__button"
        aria-label="Zoom out"
        onClick={() => store.mapCameraStore.requestZoomDelta(-1)}
      >
        −
      </button>
    </div>
  );
});

const RecenterButton = observer(({ store }: { store: RootStore }) => {
  const loc = store.locationStore;

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
