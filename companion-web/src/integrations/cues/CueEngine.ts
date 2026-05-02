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
  /** Approaching a turn (within ~50m). `distanceM` is the rider's actual
   *  distance to the maneuver — the catalog uses it instead of hardcoding
   *  "50 meters", because the rider can enter this window with d much
   *  smaller (route starting close to the turn) and "in 50 meters turn
   *  left" while actually 15 m away is jarringly inaccurate. When the
   *  next maneuver is within ~30 m of this one (a back-to-back pair),
   *  `followUpKind` carries that maneuver's direction so the engine emits
   *  a single combined cue ("In 20 meters, turn right then quickly left")
   *  instead of two overlapping cue chains. */
  | { kind: "turn50m"; turnKind: ManeuverKind; distanceM: number; followUpKind?: ManeuverKind }
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
/** Two maneuvers separated by less than this fold into a single
 *  "turn X then quickly Y" cue. 80 m at 25 km/h ≈ 11 s — enough for the
 *  rider to take both turns smoothly, not long enough for two
 *  independent 50m/10m cue cycles to fit without colliding. */
// Two maneuvers separated by less than this fold into a single "turn X
// then quickly Y" cue. 30 m matches the spec phrase "then quickly" — at
// cycling speeds that's ~4-7 s apart, the only window where coalescing
// two turns into one cue actually feels natural. 50 m / 80 m both let
// genuinely separate maneuvers ride along on a combined cue, which the
// rider then misperceives as the routing engine inventing turns that
// aren't really there.
const BACK_TO_BACK_THRESHOLD_M = 30;

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

  // First-tick announcement (replaces "Route started"). User-feedback:
  // "Route started" was useless — replace with the actual next-turn
  // announcement so the first sound the rider hears is what they need
  // to plan for.
  //
  // Three sub-cases on this tick when the route just started:
  //   A) First turn is FAR (> 50 m): emit `nextTurnInAbout` as an
  //      orientation cue ("Next turn left in about 200 meters").
  //   B) First turn is IMMINENT and stands alone (no back-to-back
  //      follow-up within ~30 m): SKIP every announce; pre-latch the
  //      50 m cue so only the 10 m approach cue speaks when the rider
  //      actually reaches the turn. User feedback: a route starting
  //      15 m from a turn used to fire next-turn + 50 m + 10 m
  //      back-to-back — three cues for one turn, with disagreeing
  //      distances.
  //   C) First turn is IMMINENT and has a back-to-back companion
  //      within ~30 m: skip the orientation cue, let the 50 m block
  //      emit the combined "in X meters turn left then quickly right"
  //      cue with the ACTUAL distance. That's the only way to warn
  //      the rider about TWO close turns in one breath, so it stays.
  if (snapshot.routeId && !s.routeStartedAnnounced) {
    const firstNonDepart = snapshot.maneuvers.find(
      (m) => m.distanceFromStartM - snapshot.progressDistanceM >= 0,
    );
    let nextAnnounced50m = s.announced50m;
    if (firstNonDepart) {
      const distanceM = firstNonDepart.distanceFromStartM - snapshot.progressDistanceM;
      if (distanceM > APPROACH_50_M) {
        // Case A.
        events.push({
          kind: "nextTurnInAbout",
          turnKind: firstNonDepart.kind,
          distanceM,
        });
      } else {
        // Case B vs. C — peek at the follow-up gap.
        const upcomingIdx = snapshot.maneuvers.findIndex((m) => m.id === firstNonDepart.id);
        const follow = snapshot.maneuvers[upcomingIdx + 1];
        const gap = follow
          ? follow.distanceFromStartM - firstNonDepart.distanceFromStartM
          : Number.POSITIVE_INFINITY;
        if (!follow || gap > BACK_TO_BACK_THRESHOLD_M) {
          // Case B: pre-latch the 50 m cue so only the 10 m action
          // cue fires for this maneuver.
          const set = new Set(nextAnnounced50m);
          set.add(firstNonDepart.id);
          nextAnnounced50m = set;
        }
        // Case C: do nothing here — the 50 m block in this same tick
        // will detect the back-to-back pair and emit the combined cue
        // with actual distance.
      }
    }
    s = { ...s, routeStartedAnnounced: true, announced50m: nextAnnounced50m };
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

    // Cue 2: 50m approach (latched per maneuver id). When the maneuver
    // immediately AFTER `upcoming` is within 80m, fold both into a
    // single "turn X then quickly Y" cue and pre-latch the follow-up
    // so its own 50m / 10m / nextTurnInAbout cues stay silent — the
    // rider already heard about it.
    if (
      upcoming &&
      upcomingDistance !== undefined &&
      upcomingDistance <= APPROACH_50_M &&
      upcomingDistance > APPROACH_10_M &&
      !announced50m.has(upcoming.id)
    ) {
      const upcomingIdx = snapshot.maneuvers.findIndex((m) => m.id === upcoming.id);
      const followUp = snapshot.maneuvers[upcomingIdx + 1];
      const gapToFollowUp = followUp
        ? followUp.distanceFromStartM - upcoming.distanceFromStartM
        : Number.POSITIVE_INFINITY;
      if (followUp && gapToFollowUp <= BACK_TO_BACK_THRESHOLD_M) {
        events.push({
          kind: "turn50m",
          turnKind: upcoming.kind,
          distanceM: upcomingDistance,
          followUpKind: followUp.kind,
        });
        announced50m.add(upcoming.id);
        announced50m.add(followUp.id);
        announced10m.add(followUp.id);
        announcedNextTurnAfter.add(upcoming.id);
      } else {
        events.push({
          kind: "turn50m",
          turnKind: upcoming.kind,
          distanceM: upcomingDistance,
        });
        announced50m.add(upcoming.id);
      }
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
        const distanceToNext = nextAfter.distanceFromStartM - snapshot.progressDistanceM;
        events.push({
          kind: "nextTurnInAbout",
          turnKind: nextAfter.kind,
          distanceM: distanceToNext,
        });
        announcedNextTurnAfter.add(lastPassed.id);
        // If the next maneuver is already within the 50 m approach
        // window when we announce it, suppress the 50 m cue for it —
        // the rider was just told. Without this they'd hear "Next
        // turn left in about 30 m" and seconds later "In 50 m turn
        // left", which is repetitive and factually wrong.
        if (distanceToNext <= APPROACH_50_M) {
          announced50m.add(nextAfter.id);
        }
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

import { type DistanceMode, distanceCueValues } from "../../i18n/formatDistance.js";
import { tIn } from "../../i18n/index.js";
import type { MessageValues } from "../../i18n/messageFormat.js";

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
    case "turn50m":
      // Use the rider's actual distance to the maneuver, not a hardcoded
      // 50 m — at route start the rider can be 15 m away when the cue
      // first fires, and "in 50 meters turn left" is jarringly wrong.
      if (event.followUpKind) {
        return {
          key: "cue.turn50mCombined",
          values: {
            ...distanceCueValues(event.distanceM, distanceMode),
            first: event.turnKind,
            second: event.followUpKind,
          },
        };
      }
      return {
        key: `cue.turn50m.${event.turnKind}`,
        values: distanceCueValues(event.distanceM, distanceMode),
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
