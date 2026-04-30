import { autorun, type IReactionDisposer } from "mobx";
import type { RootStore } from "../../app/RootStore.js";
import type { WebTtsService } from "../audio/WebTtsService.js";
import type { LiveNotificationService } from "../notifications/LiveNotificationService.js";
import type { WakeLockService } from "../screen/WakeLockService.js";
import {
  type CueEngineState,
  type CueManeuver,
  type CueSnapshot,
  formatCueEvent,
  initialCueEngineState,
  type ManeuverKind,
  tickCueEngine,
} from "./CueEngine.js";

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

  return autorun(() => {
    const settings = store.settingsStore.settings;
    const guidance = store.guidanceStore;
    const isRouting = guidance.homeMode === "phoneGuidance";
    const pairedWithDevice = false;

    void services.wakeLock.update({
      keepScreenOn: settings.keepScreenOn,
      isRouting,
    });

    const cuesActive =
      isRouting && settings.audioCuesEnabled && settings.allowBackgroundGps && !pairedWithDevice;

    if (cuesActive) {
      const snapshot = buildCueSnapshot(store, pairedWithDevice);
      const result = tickCueEngine(snapshot, cueState);
      cueState = result.nextState;
      for (const event of result.events) {
        services.tts.speak(formatCueEvent(event));
      }
    } else if (!isRouting) {
      cueState = initialCueEngineState();
    }

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
