import { autorun, type IReactionDisposer, observable, runInAction } from "mobx";
import type { RootStore } from "../../app/RootStore.js";
import { resolveDistanceUnit, resolveLocale, setActiveLocale, t, tIn } from "../../i18n/index.js";
import { recordAudioCue } from "../../stores/RoutingDiagnosticsHooks.js";
import { hasVoiceForLocale } from "../audio/voiceAvailability.js";
import type { WebTtsService } from "../audio/WebTtsService.js";
import { collapseCloseManeuvers } from "../geo.js";
import type { LiveNotificationService } from "../notifications/LiveNotificationService.js";
import type { WakeLockService } from "../screen/WakeLockService.js";
import {
  type CueEngineState,
  type CueManeuver,
  type CueSnapshot,
  cueMessage,
  initialCueEngineState,
  type ManeuverKind,
  tickCueEngine,
} from "./CueEngine.js";
import { shouldDispatchCues } from "./cueGating.js";
import { filterGlitchClusters } from "./glitchTurnFilter.js";

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
    // Audio fallback: if the OS has no TTS voice for the active locale
    // (e.g. macOS Firefox + Persian), the synthesizer would otherwise
    // spell glyphs letter-by-letter via the default English voice.
    // Speak the EN-rendered cue under `lang=en` instead — at least the
    // rider gets intelligible turn directions while the UI stays in
    // their chosen language.
    const ttsLocale = hasVoiceForLocale(locale) ? locale : "en";
    services.tts.setLang(ttsLocale);
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
      isExploringAlternativesFromGuidance: guidance.isExploringAlternativesFromGuidance,
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
        // Render the spoken text in whatever locale matches the picked
        // TTS voice — `tIn("en", ...)` when we fell back to English so
        // the voice and the text agree, otherwise the active locale.
        const text =
          ttsLocale === locale ? t(msg.key, msg.values) : tIn(ttsLocale, msg.key, msg.values);
        services.tts.speak(text);
        recordAudioCue(store.routingDiagnosticsStore, event.kind, text);
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
      // Notification gate (user-feedback): only fire the platform card
      // when there's an upcoming turn within ~500m. Outside that window
      // the rider is mid-segment with nothing to act on, and a stale
      // "Continue" notification is just noise.
      const upcoming = guidance.upcomingTurnAlert;
      const closeUpcomingTurn = upcoming !== undefined && upcoming.distanceRemainingM <= 500;
      const content = { title, body, closeUpcomingTurn };
      if (!lastLiveActivityActive) {
        void services.liveNotification.start(content);
      } else {
        void services.liveNotification.update(content);
      }
    } else if (lastLiveActivityActive) {
      services.liveNotification.stop();
    }
    lastLiveActivityActive = liveActivityActive;
  });
}

/** Composite key used as CueSnapshot.routeId so a revision bump on the same
 *  route identifier is treated as a genuine route change by the CueEngine. */
export function buildRouteKey(
  routeIdentifier: string | undefined,
  routeRevision: number | undefined,
): string | undefined {
  if (!routeIdentifier) return undefined;
  return `${routeIdentifier}-rev${routeRevision ?? 0}`;
}

function buildCueSnapshot(store: RootStore, pairedWithDevice: boolean): CueSnapshot {
  const guidance = store.guidanceStore;
  const route = guidance.guidanceRoute;
  const filteredRouteManeuvers = route
    ? collapseCloseManeuvers(
        filterGlitchClusters(route.maneuvers, route.geometry) as unknown as {
          id: string;
          distanceFromStartM: number;
        }[],
        route.geometry,
      )
    : [];
  const maneuvers: CueManeuver[] = filteredRouteManeuvers.flatMap((m) => {
    const rt = (route?.maneuvers ?? []).find((rm) => rm.id === m.id);
    if (!rt) return [];
    const kind = maneuverKindFromType(rt.maneuverType);
    if (kind === undefined) return [];
    return [
      {
        id: rt.id,
        kind,
        distanceFromStartM: rt.distanceFromStartMeters,
      },
    ];
  });
  const routeTotalDistanceM = route?.summary.totalDistanceMeters ?? 0;
  return {
    routeId: buildRouteKey(
      guidance.activeSession.routeIdentifier,
      guidance.activeSession.routeRevision,
    ),
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

/** Exported for testing. Returns `undefined` for maneuver types that
 *  should not produce any audio cue:
 *   - `straight` — not a turn
 *   - `depart` / `arrive` — handled by dedicated arrived/arrivingInM events
 *  Unknown / future types fall through to the silenced default. */
export function maneuverKindFromType(type: string): ManeuverKind | undefined {
  switch (type) {
    case "left":
    case "sharpLeft":
      return "left";
    case "right":
    case "sharpRight":
      return "right";
    case "slightLeft":
      return "slightLeft";
    case "slightRight":
      return "slightRight";
    case "uturn":
      return "uturn";
    case "roundabout":
      return "roundabout";
    case "merge":
      return "merge";
    case "ramp":
      return "ramp";
    case "straight":
    case "depart":
    case "arrive":
      return undefined;
    default:
      return undefined;
  }
}
