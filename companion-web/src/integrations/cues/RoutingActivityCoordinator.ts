import { autorun, observable, runInAction, type IReactionDisposer } from "mobx";
import type { RootStore } from "../../app/RootStore.js";
import {
  resolveDistanceUnit,
  resolveLocale,
  setActiveLocale,
  t,
} from "../../i18n/index.js";
import type { WebTtsService } from "../audio/WebTtsService.js";
import type { LiveNotificationService } from "../notifications/LiveNotificationService.js";
import type { WakeLockService } from "../screen/WakeLockService.js";
import {
  cueMessage,
  type CueEngineState,
  type CueManeuver,
  type CueSnapshot,
  initialCueEngineState,
  type ManeuverKind,
  tickCueEngine,
} from "./CueEngine.js";
import { shouldDispatchCues } from "./cueGating.js";

/**
 * Observable bridge over the Page Visibility API. The autorun below
 * reads this so MobX re-runs the cue dispatch when the user switches
 * tab / locks the screen / re-focuses the app. SSR-safe via the typeof
 * checks; the box stays `false` (treated as "foregrounded") when there
 * is no `document` in scope.
 */
const isAppInBackgroundBox = observable.box(
  typeof document !== "undefined" && document.visibilityState === "hidden",
);

if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    runInAction(() => {
      isAppInBackgroundBox.set(document.visibilityState === "hidden");
    });
  });
}

/**
 * Bridges MobX state (settings + GuidanceStore) to the four routing-time
 * side-effect services. Subscribes via autorun and disposes cleanly. The
 * wiring layer is the only place that consults the gating expressions:
 *  - WakeLock active iff `keepScreenOn && isRouting`
 *  - Cues active iff `audioCuesEnabled && allowBackgroundGps && !pairedWithDevice && isRouting`
 *  - Live notification active iff `liveActivityEnabled && allowBackgroundGps && isRouting`
 *
 * `pairedWithDevice` is currently always false on web — the ESP companion
 * pairing flow is tracked on the iOS/Android apps and not yet on web. When
 * web gains a pairing UI, surface a `pairedWithDevice` flag on RootStore
 * and feed it here.
 */
export function startRoutingActivityCoordinator(
  store: RootStore,
  services: {
    wakeLock: WakeLockService;
    tts: WebTtsService;
    liveNotification: LiveNotificationService;
  },
): IReactionDisposer {
  let cueState: CueEngineState = initialCueEngineState();
  let lastLiveActivityActive = false;
  // Tracks the previous tick's gating decision so we can detect a
  // false→true transition mid-route (e.g. rider unchecks "audio cues
  // only when in background" while the app is foregrounded). Without
  // this, all per-route latches would already be set from earlier
  // ticks and the rider would hear nothing until the next maneuver
  // crossing — looks like the toggle didn't apply, when really cues
  // are event-driven and there was just no event to fire. Resetting
  // `routeStartedAnnounced` on the transition gives an immediate
  // audible confirmation by re-announcing "Route started".
  let prevCuesActive = false;

  return autorun(() => {
    const settings = store.settingsStore.settings;
    const guidance = store.guidanceStore;
    const isRouting = guidance.homeMode === "phoneGuidance";
    const pairedWithDevice = false;

    // Apply the user's language + distance preferences to the runtime so
    // every `t(...)` call below renders correctly. resolveLocale falls
    // back to navigator.languages when preference is "system".
    const locale = resolveLocale(settings.language);
    setActiveLocale(locale);
    services.tts.setLang(locale);
    const distanceMode = resolveDistanceUnit(settings.distanceUnit, locale);

    void services.wakeLock.update({
      keepScreenOn: settings.keepScreenOn,
      isRouting,
    });

    const cuesActive = shouldDispatchCues({
      isRouting,
      audioCuesEnabled: settings.audioCuesEnabled,
      allowBackgroundGps: settings.allowBackgroundGps,
      pairedWithDevice,
      audioCuesOnlyInBackground: settings.audioCuesOnlyInBackground,
      isAppInBackground: isAppInBackgroundBox.get(),
    });

    if (cuesActive) {
      // Mid-route transition false→true: re-arm the route-started latch
      // so the next tick emits a confirmation cue. See the prevCuesActive
      // declaration above for the full rationale.
      if (!prevCuesActive && isRouting) {
        cueState = { ...cueState, routeStartedAnnounced: false };
      }
      const snapshot = buildCueSnapshot(store, pairedWithDevice);
      const result = tickCueEngine(snapshot, cueState);
      cueState = result.nextState;
      for (const event of result.events) {
        const msg = cueMessage(event, distanceMode);
        services.tts.speak(t(msg.key, msg.values));
      }
    } else if (!isRouting) {
      cueState = initialCueEngineState();
    }
    prevCuesActive = cuesActive;

    const liveActivityActive =
      isRouting && settings.liveActivityEnabled && settings.allowBackgroundGps;
    if (liveActivityActive) {
      const title = guidance.activeNavigationTitle || "Riding";
      // Body is direction-only on purpose — distance/time would shift on
      // every GPS tick and cause the platform Notification to re-chime
      // every few seconds. Only the upcoming maneuver itself drives a
      // re-post; LiveNotificationService dedupes identical bodies.
      const body = guidance.nextTurnDescriptionForNotification;
      if (!lastLiveActivityActive) {
        void services.liveNotification.start({ title, body });
      } else {
        void services.liveNotification.update({ title, body });
      }
    } else if (lastLiveActivityActive) {
      services.liveNotification.stop();
    }
    lastLiveActivityActive = liveActivityActive;
  });
}

function buildCueSnapshot(store: RootStore, pairedWithDevice: boolean): CueSnapshot {
  const guidance = store.guidanceStore;
  const route = guidance.guidanceRoute;
  const maneuvers: CueManeuver[] = (route?.maneuvers ?? [])
    .filter((m) => m.maneuverType !== "depart" && m.maneuverType !== "arrive")
    .map((m) => ({
      id: m.id,
      kind: maneuverKindFromType(m.maneuverType),
      distanceFromStartM: m.distanceFromStartMeters,
    }));
  const routeTotalDistanceM = route?.summary.totalDistanceMeters ?? 0;
  return {
    routeId: guidance.activeSession.routeIdentifier,
    pairedWithDevice,
    progressDistanceM: guidance.progressDistanceM,
    maneuvers,
    offRoute: guidance.offRoute,
    rerouting: guidance.rerouteRequested,
    arrived: guidance.arrivalNotice !== undefined,
    distanceFromRouteM: guidance.offRouteDistanceM,
    routeTotalDistanceM,
  };
}

function maneuverKindFromType(type: string): ManeuverKind {
  switch (type) {
    case "left":
    case "slightLeft":
    case "sharpLeft":
      return "left";
    case "right":
    case "slightRight":
    case "sharpRight":
      return "right";
    case "uturn":
      return "uturn";
    case "ramp":
    case "merge":
    case "roundabout":
      return "generic";
    default:
      return "generic";
  }
}
