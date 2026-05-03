import { reaction } from "mobx";
import { observer } from "mobx-react-lite";
import { useEffect, useRef } from "react";
import type { RootStore } from "../../app/RootStore.js";
import { useT } from "../../i18n/useT.js";
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

  // Camera-follow trigger for PLANNING transitions ONLY. User-reported:
  // depending on `currentLocation` here meant every GPS tick re-centred
  // the map even after the user panned, so they couldn't pan freely
  // while stationary. It also caused the first-fix to "jump" the camera
  // into a riding-mode-looking zoom because that was the first time
  // refreshCameraForCurrentMode had run with non-default state.
  //
  // Now: refresh only when mode / selected alternative / route revision
  // changes. Free-pan in stationary mode is preserved (spec line 87 —
  // "stays where user moved to until target user location icon is
  // pressed"), and there's a separate one-shot first-fix recenter below.
  useEffect(() => {
    return reaction(
      () => ({
        homeMode: store.guidanceStore.homeMode,
        selectedId: store.planningStore.preview.selectedAlternativeID,
        revision: store.planningStore.preview.routeRevision,
      }),
      () => {
        if (store.guidanceStore.homeMode === "phoneGuidance") return;
        refreshCameraForCurrentMode(store);
      },
      { fireImmediately: true },
    );
  }, [store]);

  // One-shot first-fix recenter (planning only). Mirrors the iOS
  // `lastKnownLocation`-driven recenter on cold launch — the camera
  // moves from the default Helsinki fallback to the rider's actual
  // position the moment GPS lands a fix, but does NOT re-recenter on
  // every subsequent fix (free-pan stays sticky).
  const firstFixDoneRef = useRef(false);
  useEffect(() => {
    return reaction(
      () => store.locationStore.currentLocation !== undefined,
      (haveFix) => {
        if (!haveFix) return;
        if (firstFixDoneRef.current) return;
        firstFixDoneRef.current = true;
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
      {/* iOS-parity persistent right + left side rails. Both anchored
          at a single Y offset (`--rail-top`) so icon positions don't
          reflow between planning ↔ routing. Conditional items (device
          chip, alternate-routes) sit at the BOTTOM of each rail. */}
      <LocatingIndicator store={store} />
      <RightSideRail store={store} />
      <LeftSideRail store={store} />
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
  // Hide whenever the route-suggestions card covers the bottom of the screen
  if (store.guidanceStore.isExploringAlternativesFromGuidance) return null;
  if (
    store.guidanceStore.homeMode === "planning" &&
    store.planningStore.preview.alternatives.length > 0
  )
    return null;
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
        <div className="overlay-top">
          <div className="search-bar">
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
      <div className="overlay-top">
        <div className="card guidance-header">
          <div className="guidance-header__text">
            <div className="list-row__title">{headline}</div>
            {guidance.distanceToDestinationLine && (
              <div className="list-row__subtitle">{guidance.distanceToDestinationLine}</div>
            )}
            {guidance.minutesRemainingLine && (
              <div className="list-row__subtitle">{guidance.minutesRemainingLine}</div>
            )}
          </div>
        </div>
      </div>
    );
  }
  // Planning: full-width where-to bar. Settings cog lives on the
  // right rail (iOS parity) — no longer inline with the search input.
  return (
    <div className="overlay-top" ref={containerRef}>
      <div className="search-bar">
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

const BottomOverlay = observer(({ store }: { store: RootStore }) => {
  const planning = store.planningStore;
  const guidance = store.guidanceStore;
  const status = planning.planningStatus ?? planning.importActivityStatus;
  if (guidance.homeMode === "phoneGuidance") {
    if (guidance.isExploringAlternativesFromGuidance) {
      return (
        <div className="overlay-bottom">
          <RouteSuggestionsCard store={store} />
        </div>
      );
    }
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

/**
 * iOS-parity right-side rail. Renders `topRightIconStack` items in a
 * vertical column at a fixed Y offset. Order, top → bottom: settings →
 * compass/north-up → device chip (only when paired). The compass tap
 * recentres the camera (single tap = north-up; double-tap = lock north-up,
 * matching the Rust impl).
 */
const RightSideRail = observer(({ store }: { store: RootStore }) => {
  if (store.planningStore.isSearchOpen) return null;
  const guidance = store.guidanceStore;
  return (
    <div className="rail rail--right">
      {guidance.topRightIconStack.map((item) => {
        if (item === "settings") {
          return (
            <button
              key="settings"
              type="button"
              className="rail__icon"
              aria-label="Settings"
              onClick={() => store.goSettings()}
            >
              ⚙
            </button>
          );
        }
        if (item === "compass") {
          return (
            <button
              key="compass"
              type="button"
              className="rail__icon"
              aria-label="Recenter map"
              title="Tap = recenter / north-up; double-tap = lock north-up"
              onClick={() => {
                if (guidance.homeMode === "phoneGuidance") {
                  guidance.handleCompassTap();
                } else {
                  refreshCameraForCurrentMode(store);
                }
              }}
              onDoubleClick={() => guidance.handleCompassDoubleTap()}
            >
              {guidance.compassMode === "northLocked"
                ? "🧭"
                : guidance.compassMode === "northPreview"
                  ? "↑"
                  : "◎"}
            </button>
          );
        }
        // `deviceChip` — web has no pairing flow yet, so this branch is
        // unreachable today but kept for shape parity with iOS.
        return null;
      })}
    </div>
  );
});

/**
 * iOS-parity left-side rail. Renders `topLeftIconStack` items.
 * Order, top → bottom: zoom-in → zoom-out → alternate-routes (only in
 * routing). The alternate-routes button appears at the BOTTOM so the
 * zoom column never shifts position when the rider presses Start.
 */
const LeftSideRail = observer(({ store }: { store: RootStore }) => {
  if (store.planningStore.isSearchOpen) return null;
  const guidance = store.guidanceStore;
  return (
    <div className="rail rail--left">
      {guidance.topLeftIconStack.map((item) => {
        if (item === "zoomIn") {
          return (
            <button
              key="zoomIn"
              type="button"
              className="rail__icon"
              aria-label="Zoom in"
              onClick={() => store.mapCameraStore.requestZoomDelta(1)}
            >
              +
            </button>
          );
        }
        if (item === "zoomOut") {
          return (
            <button
              key="zoomOut"
              type="button"
              className="rail__icon"
              aria-label="Zoom out"
              onClick={() => store.mapCameraStore.requestZoomDelta(-1)}
            >
              −
            </button>
          );
        }
        // alternateRoutes — split-way reroute: plan fresh alternatives
        // from the rider's current location to the same destination.
        return (
          <button
            key="alternateRoutes"
            type="button"
            className="rail__icon"
            aria-label="Find alternate routes"
            title="Find alternate routes from here"
            onClick={() => void store.exploreAlternateRoutes()}
          >
            ⇄
          </button>
        );
      })}
    </div>
  );
});

/**
 * Planning-only status indicator: shows a spinner while we're waiting
 * for the first GPS fix and a "blocked" glyph when the user denied
 * location. Sits to the LEFT of the right rail so it doesn't clobber
 * the persistent settings/compass column. The recenter affordance is
 * folded into the compass icon itself.
 */
const LocatingIndicator = observer(({ store }: { store: RootStore }) => {
  if (store.planningStore.isSearchOpen) return null;
  if (store.guidanceStore.homeMode !== "planning") return null;
  const loc = store.locationStore;
  if (loc.isWaitingForFirstFix) {
    return (
      <div
        className="rail__icon locating-indicator"
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
        className="rail__icon locating-indicator"
        aria-label="Location blocked"
        title="Location is blocked. Enable it in your browser settings to plan from your real position."
        onClick={() => store.dismissLocationBanner()}
      >
        📍
      </button>
    );
  }
  return null;
});
