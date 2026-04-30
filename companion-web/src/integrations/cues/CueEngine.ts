// Audio cue trigger engine — pure function consumed by the wiring layer.
// Spec: docs/ux-specs.md lines 133-143.

export type ManeuverKind =
  | "left"
  | "right"
  | "keepLeft"
  | "keepRight"
  | "exitLeft"
  | "exitRight"
  | "uturn"
  | "generic";

export type CueManeuver = {
  id: string;
  kind: ManeuverKind;
  distanceFromStartM: number;
};

export type CueSnapshot = {
  routeId: string | undefined;
  pairedWithDevice: boolean;
  progressDistanceM: number;
  /** Excluding depart/arrive; ordered by distanceFromStartM ascending. */
  maneuvers: CueManeuver[];
  offRoute: boolean;
  rerouting: boolean;
  arrived: boolean;
  /** Straight-line distance from the rider to the projected route. */
  distanceFromRouteM: number;
  routeTotalDistanceM: number;
};

export type CueEvent =
  | { kind: "routeStarted" }
  | { kind: "turn50m"; turnKind: ManeuverKind }
  | { kind: "turn10m"; turnKind: ManeuverKind }
  | { kind: "nextTurnInAbout"; turnKind: ManeuverKind; distanceM: number }
  | { kind: "arrivingInM"; distanceM: number }
  | { kind: "arrived" }
  | { kind: "offTrack" }
  | { kind: "rerouting" }
  | { kind: "repeatedOffTrackSilence" }
  | { kind: "onTrack" };

export type CueEngineState = {
  lastRouteId: string | undefined;
  routeStartedAnnounced: boolean;
  announced50m: ReadonlySet<string>;
  announced10m: ReadonlySet<string>;
  announcedNextTurnAfter: ReadonlySet<string>;
  approachingDestinationAnnounced: boolean;
  arrivedAnnounced: boolean;
  offRouteEpisodeCount: number;
  prevOffRoute: boolean;
  prevRerouting: boolean;
  silenced: boolean;
  consecutiveOnRouteSamples: number;
  onTrackAnnounced: boolean;
};

const APPROACH_50_M = 50;
const APPROACH_10_M = 10;
const PASSED_TURN_M = 10;
const ON_TRACK_CONFIRM_SAMPLES = 5;
const ON_TRACK_CORRIDOR_M = 22;
const REPEAT_OFFTRACK_SILENCE_THRESHOLD = 2;

export function initialCueEngineState(): CueEngineState {
  return {
    lastRouteId: undefined,
    routeStartedAnnounced: false,
    announced50m: new Set(),
    announced10m: new Set(),
    announcedNextTurnAfter: new Set(),
    approachingDestinationAnnounced: false,
    arrivedAnnounced: false,
    offRouteEpisodeCount: 0,
    prevOffRoute: false,
    prevRerouting: false,
    silenced: false,
    consecutiveOnRouteSamples: 0,
    onTrackAnnounced: false,
  };
}

export function tickCueEngine(
  snapshot: CueSnapshot,
  state: CueEngineState,
): { events: CueEvent[]; nextState: CueEngineState } {
  // Paired with device → ESP is the UI; suppress every cue.
  if (snapshot.pairedWithDevice) {
    return { events: [], nextState: state };
  }

  // Reset all latches on route id change (including reroute completion).
  let s: CueEngineState =
    snapshot.routeId !== state.lastRouteId
      ? { ...initialCueEngineState(), lastRouteId: snapshot.routeId }
      : state;

  const events: CueEvent[] = [];

  // Cue 1: route started — first tick of a non-empty route id.
  if (snapshot.routeId && !s.routeStartedAnnounced) {
    events.push({ kind: "routeStarted" });
    s = { ...s, routeStartedAnnounced: true };
  }

  // Off-route episode tracking.
  const offRouteRose = !s.prevOffRoute && snapshot.offRoute;
  let offRouteEpisodeCount = s.offRouteEpisodeCount;
  if (offRouteRose) offRouteEpisodeCount += 1;

  // Silence triggers when the rider's THIRD episode begins (offRouteEpisodeCount > 2).
  let silenced = s.silenced;
  let onTrackAnnounced = s.onTrackAnnounced;
  let consecutiveOnRouteSamples = s.consecutiveOnRouteSamples;

  // Confirm on-track when not off-route and inside the corridor.
  if (!snapshot.offRoute && snapshot.distanceFromRouteM < ON_TRACK_CORRIDOR_M) {
    consecutiveOnRouteSamples += 1;
  } else {
    consecutiveOnRouteSamples = 0;
  }

  if (silenced && consecutiveOnRouteSamples >= ON_TRACK_CONFIRM_SAMPLES && !onTrackAnnounced) {
    events.push({ kind: "onTrack" });
    silenced = false;
    onTrackAnnounced = true;
    // Reset episode count so the rider can start fresh.
    offRouteEpisodeCount = 0;
  }

  if (offRouteRose && offRouteEpisodeCount > REPEAT_OFFTRACK_SILENCE_THRESHOLD && !silenced) {
    events.push({ kind: "repeatedOffTrackSilence" });
    silenced = true;
    onTrackAnnounced = false;
  } else if (offRouteRose && !silenced) {
    events.push({ kind: "offTrack" });
  }

  // Rerouting rising edge.
  const reroutingRose = !s.prevRerouting && snapshot.rerouting;
  if (reroutingRose && !silenced) {
    events.push({ kind: "rerouting" });
  }

  // While silenced, suppress maneuver/arrival cues.
  if (!silenced && !snapshot.offRoute) {
    const announced50m = new Set(s.announced50m);
    const announced10m = new Set(s.announced10m);
    const announcedNextTurnAfter = new Set(s.announcedNextTurnAfter);

    // Find the next maneuver ahead of progress.
    const upcoming = snapshot.maneuvers.find(
      (m) => m.distanceFromStartM - snapshot.progressDistanceM >= 0,
    );
    const upcomingDistance = upcoming
      ? upcoming.distanceFromStartM - snapshot.progressDistanceM
      : undefined;

    // Cue 2: 50m approach (latched per maneuver id).
    if (
      upcoming &&
      upcomingDistance !== undefined &&
      upcomingDistance <= APPROACH_50_M &&
      upcomingDistance > APPROACH_10_M &&
      !announced50m.has(upcoming.id)
    ) {
      events.push({ kind: "turn50m", turnKind: upcoming.kind });
      announced50m.add(upcoming.id);
    }

    // Cue 3: 10m approach (latched per maneuver id).
    if (
      upcoming &&
      upcomingDistance !== undefined &&
      upcomingDistance <= APPROACH_10_M &&
      !announced10m.has(upcoming.id)
    ) {
      events.push({ kind: "turn10m", turnKind: upcoming.kind });
      announced10m.add(upcoming.id);
    }

    // Cue 4 + 5: passed last maneuver by 10m.
    // Find the maneuver immediately behind progress (largest distanceFromStartM
    // less than progressDistanceM) and check if rider is >= PASSED_TURN_M past it.
    const lastPassed = [...snapshot.maneuvers]
      .filter((m) => snapshot.progressDistanceM - m.distanceFromStartM >= PASSED_TURN_M)
      .sort((a, b) => b.distanceFromStartM - a.distanceFromStartM)[0];
    if (lastPassed && !announcedNextTurnAfter.has(lastPassed.id)) {
      const indexOfLast = snapshot.maneuvers.findIndex((m) => m.id === lastPassed.id);
      const nextAfter = snapshot.maneuvers[indexOfLast + 1];
      if (nextAfter) {
        // Cue 4: there's another maneuver — announce next turn.
        events.push({
          kind: "nextTurnInAbout",
          turnKind: nextAfter.kind,
          distanceM: nextAfter.distanceFromStartM - snapshot.progressDistanceM,
        });
        announcedNextTurnAfter.add(lastPassed.id);
      } else if (!s.approachingDestinationAnnounced) {
        // Cue 5: no further maneuvers — arriving at destination.
        events.push({
          kind: "arrivingInM",
          distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM,
        });
        announcedNextTurnAfter.add(lastPassed.id);
        s = { ...s, approachingDestinationAnnounced: true };
      }
    }

    s = {
      ...s,
      announced50m,
      announced10m,
      announcedNextTurnAfter,
    };
  }

  // Cue 6: arrived.
  if (snapshot.arrived && !s.arrivedAnnounced) {
    events.push({ kind: "arrived" });
    s = { ...s, arrivedAnnounced: true };
  }

  return {
    events,
    nextState: {
      ...s,
      prevOffRoute: snapshot.offRoute,
      prevRerouting: snapshot.rerouting,
      offRouteEpisodeCount,
      silenced,
      consecutiveOnRouteSamples,
      onTrackAnnounced,
    },
  };
}

import { distanceCueValues, type DistanceMode } from "../../i18n/formatDistance.js";
import type { MessageValues } from "../../i18n/messageFormat.js";
import { tIn } from "../../i18n/index.js";

/** Structured cue: an i18n catalog key + ICU placeholder values. The wiring
 *  layer feeds these to `t(key, values)` against the active locale; tests
 *  can resolve them against any locale. */
export type CueMessage = { key: string; values: MessageValues };

/**
 * Produce the structured (key, values) tuple for a `CueEvent`. This is the
 * locale-agnostic seam: the cue engine emits events; this maps them to
 * catalog keys + ICU placeholder bundles. The runtime renders via
 * `t(msg.key, msg.values)`; parity tests render via `tIn("en", ...)`.
 *
 * `distanceMode` selects metric/imperial for spoken distance values.
 */
export function cueMessage(event: CueEvent, distanceMode: DistanceMode = "metric"): CueMessage {
  switch (event.kind) {
    case "routeStarted":
      return { key: "cue.routeStarted", values: {} };
    case "turn50m":
      return {
        key: `cue.turn50m.${event.turnKind}`,
        values: distanceCueValues(50, distanceMode),
      };
    case "turn10m":
      return { key: `cue.turn10m.${event.turnKind}`, values: {} };
    case "nextTurnInAbout":
      return {
        key: `cue.nextTurnInAbout.${nextTurnDirection(event.turnKind)}`,
        values: distanceCueValues(event.distanceM, distanceMode),
      };
    case "arrivingInM":
      return {
        key: "cue.arrivingInM",
        values: distanceCueValues(event.distanceM, distanceMode),
      };
    case "arrived":
      return { key: "cue.arrived", values: {} };
    case "offTrack":
    case "repeatedOffTrackSilence":
      return { key: "cue.offTrack", values: {} };
    case "rerouting":
      return { key: "cue.rerouting", values: {} };
    case "onTrack":
      return { key: "cue.onTrack", values: {} };
  }
}

/**
 * Legacy English formatter — kept as the exact-byte path that existing
 * cue-engine tests assert against. New call sites should go through
 * `cueMessage(event)` + the active-locale `t(...)` instead.
 */
export function formatCueEvent(event: CueEvent): string {
  const { key, values } = cueMessage(event, "metric");
  return tIn("en", key, values);
}

/** Collapse the 8 maneuver kinds into the 4 directions the
 *  `cue.nextTurnInAbout.*` catalog supports. */
function nextTurnDirection(kind: ManeuverKind): "left" | "right" | "uturn" | "generic" {
  switch (kind) {
    case "left":
    case "keepLeft":
    case "exitLeft":
      return "left";
    case "right":
    case "keepRight":
    case "exitRight":
      return "right";
    case "uturn":
      return "uturn";
    case "generic":
      return "generic";
  }
}
