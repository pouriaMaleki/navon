// Audio cue trigger engine — pure function consumed by the wiring layer.
// Spec: docs/ux-specs.md lines 133-143.

export type ManeuverKind =
  | "left"
  | "right"
  | "slightLeft"
  | "slightRight"
  | "exitLeft"
  | "exitRight"
  | "uturn"
  | "roundabout"
  | "merge"
  | "ramp"
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
  /**
   * Immediate-action 10 m cue. `followUpKind` is set when the next maneuver
   * is within `BACK_TO_BACK_THRESHOLD_M` of this one — covers the
   * sparse-GPS / fast-cycling case where the 50 m combined cue was missed
   * because the first in-range tick already landed inside 15 m of M1. Without
   * this fold, the rider would hear only "turn <first>" with no mention of
   * the immediately-following turn.
   */
  | { kind: "turn10m"; turnKind: ManeuverKind; followUpKind?: ManeuverKind }
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
  /** Number of rerouting cues that have fired (counts rising edges).
   *  Persists across route id changes, since each successful reroute itself
   *  causes a route id change. Reset only when the rider is confirmed
   *  on-track for ON_TRACK_CONFIRM_SAMPLES consecutive ticks. */
  reroutingEpisodeCount: number;
  /** Number of consecutive off-route ticks. Resets on any on-route tick.
   *  offTrack only fires after OFF_ROUTE_HYSTERESIS_TICKS consecutive. */
  offRouteTickCount: number;
};

const APPROACH_50_M = 50;
const APPROACH_10_M = 15;
/** When the next turn is closer than this at the time of the nextTurnInAbout
 *  announcement, pre-latch the turn50m cue so it never fires. The rider has
 *  already been told the turn is near; a redundant "in 50 m" announcement
 *  before they can even hear the first one is jarring. */
const SKIP_50M_BELOW_DISTANCE_M = 100;
const PASSED_TURN_M = 10;
const ON_TRACK_CONFIRM_SAMPLES = 5;
const ON_TRACK_CORRIDOR_M = 22;
const OFF_ROUTE_HYSTERESIS_TICKS = 3;
/** When the rider is this far from the route, skip hysteresis — they're genuinely lost. */
const OFF_ROUTE_IMMEDIATE_DISTANCE_M = 50;
const REPEAT_OFFTRACK_SILENCE_THRESHOLD = 2;
/** Cap on rerouting audio cues per "off-route session". After this many fires,
 *  stay silent until the rider is confirmed on-track. */
const REROUTING_CUE_CAP = 2;
/** If the last cue maneuver sits within this distance of the route end,
 *  approaching it is indistinguishable from arriving: substitute arrivingInM
 *  for any nextTurnInAbout or approach cues so the rider hears "arriving in Xm"
 *  rather than a phantom turn command. */
const CLOSE_TO_DESTINATION_M = 30;
/** Two maneuvers separated by less than this fold into a single "turn X
 *  then quickly Y" cue. 30 m at cycling speeds ≈ 4-7 s — the only window
 *  where coalescing two turns into one cue feels natural. */
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
    reroutingEpisodeCount: 0,
    offRouteTickCount: 0,
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
      ? {
          ...initialCueEngineState(),
          lastRouteId: snapshot.routeId,
          // Persist rerouting silence across route id changes — every
          // successful reroute issues a new route id, so resetting here
          // would defeat the cue cap.
          reroutingEpisodeCount: state.reroutingEpisodeCount,
        }
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
    const nextAnnounced10m = new Set(s.announced10m);
    const nextAnnouncedNextTurnAfter = new Set(s.announcedNextTurnAfter);
    if (firstNonDepart) {
      const distanceM = firstNonDepart.distanceFromStartM - snapshot.progressDistanceM;
      if (distanceM > APPROACH_50_M) {
        // Case A — orientation cue.
        // Bug 1: if firstNonDepart is the last cue maneuver AND very close
        // to the route end, announce "arriving" instead of a phantom turn.
        const firstIdx = snapshot.maneuvers.findIndex((m) => m.id === firstNonDepart.id);
        const isLastManeuver = firstIdx === snapshot.maneuvers.length - 1;
        const distToEnd = snapshot.routeTotalDistanceM - firstNonDepart.distanceFromStartM;
        if (isLastManeuver && distToEnd < CLOSE_TO_DESTINATION_M) {
          if (!s.approachingDestinationAnnounced) {
            events.push({
              kind: "arrivingInM",
              distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM,
            });
            s = {
              ...s,
              approachingDestinationAnnounced: true,
              announced50m: new Set([...s.announced50m, firstNonDepart.id]),
              announced10m: new Set([...s.announced10m, firstNonDepart.id]),
            };
          }
        } else {
          events.push({
            kind: "nextTurnInAbout",
            turnKind: firstNonDepart.kind,
            distanceM,
          });
          // Pre-latch turn50m when the first turn is already close: the rider
          // has the orientation cue; a redundant "in 50 m" a few seconds later
          // would be jarring before they can even react to the first.
          if (distanceM < SKIP_50M_BELOW_DISTANCE_M) {
            const set = new Set(nextAnnounced50m);
            set.add(firstNonDepart.id);
            nextAnnounced50m = set;
          }
        }
      } else {
        // Case B vs. C — peek at the follow-up gap.
        const upcomingIdx = snapshot.maneuvers.findIndex((m) => m.id === firstNonDepart.id);
        const follow = snapshot.maneuvers[upcomingIdx + 1];
        const gap = follow
          ? follow.distanceFromStartM - firstNonDepart.distanceFromStartM
          : Number.POSITIVE_INFINITY;
        if (follow && gap <= BACK_TO_BACK_THRESHOLD_M) {
          events.push({
            kind: "turn50m",
            turnKind: firstNonDepart.kind,
            distanceM,
            followUpKind: follow.kind,
          });
          const set50 = new Set(nextAnnounced50m);
          set50.add(firstNonDepart.id);
          set50.add(follow.id);
          nextAnnounced50m = set50;
          nextAnnounced10m.add(firstNonDepart.id);
          nextAnnounced10m.add(follow.id);
          nextAnnouncedNextTurnAfter.add(firstNonDepart.id);
        } else {
          // Case B: pre-latch the 50 m cue so only the 10 m action
          // cue fires for this maneuver.
          const set = new Set(nextAnnounced50m);
          set.add(firstNonDepart.id);
          nextAnnounced50m = set;
        }
      }
    }
    s = {
      ...s,
      routeStartedAnnounced: true,
      announced50m: nextAnnounced50m,
      announced10m: nextAnnounced10m,
      announcedNextTurnAfter: nextAnnouncedNextTurnAfter,
    };
  }

  // Off-route hysteresis: count consecutive off-route ticks. Fire offTrack
  // only after OFF_ROUTE_HYSTERESIS_TICKS consecutive, or immediately when
  // the rider is far from the route (genuinely lost, not a GPS blip).
  let offRouteTickCount = s.offRouteTickCount;
  if (snapshot.offRoute) {
    offRouteTickCount += 1;
  } else {
    offRouteTickCount = 0;
  }

  let offRouteEpisodeCount = s.offRouteEpisodeCount;

  const immediateOffTrack =
    snapshot.offRoute &&
    snapshot.distanceFromRouteM > OFF_ROUTE_IMMEDIATE_DISTANCE_M &&
    offRouteTickCount === 1;
  const hysteresisOffTrack = snapshot.offRoute && offRouteTickCount === OFF_ROUTE_HYSTERESIS_TICKS;
  const offTrackFired = immediateOffTrack || hysteresisOffTrack;

  if (offTrackFired) {
    offRouteEpisodeCount += 1;
  }

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

  let reroutingEpisodeCount = s.reroutingEpisodeCount;

  if (silenced && consecutiveOnRouteSamples >= ON_TRACK_CONFIRM_SAMPLES && !onTrackAnnounced) {
    events.push({ kind: "onTrack" });
    silenced = false;
    onTrackAnnounced = true;
    // Reset episode count so the rider can start fresh.
    offRouteEpisodeCount = 0;
  }
  // Independent reset of the rerouting cue counter: even without an off-route
  // silence event, a sustained on-track confirmation means the rider is back
  // on the route and the next reroute episode (if any) deserves a fresh count.
  if (consecutiveOnRouteSamples >= ON_TRACK_CONFIRM_SAMPLES) {
    reroutingEpisodeCount = 0;
  }

  if (offTrackFired && offRouteEpisodeCount > REPEAT_OFFTRACK_SILENCE_THRESHOLD && !silenced) {
    events.push({ kind: "repeatedOffTrackSilence" });
    silenced = true;
    onTrackAnnounced = false;
  } else if (offTrackFired && !silenced) {
    events.push({ kind: "offTrack" });
  }

  // Rerouting rising edge — capped at REROUTING_CUE_CAP per off-route session.
  const reroutingRose = !s.prevRerouting && snapshot.rerouting;
  if (reroutingRose) reroutingEpisodeCount += 1;
  if (reroutingRose && !silenced && reroutingEpisodeCount <= REROUTING_CUE_CAP) {
    events.push({ kind: "rerouting" });
  }

  // While silenced or arrived, suppress maneuver/arrival cues.
  if (!silenced && !snapshot.offRoute && !snapshot.rerouting && !snapshot.arrived) {
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

    // Cue 3: 10m approach (latched per maneuver id). Mirror the 50 m
    // branch's back-to-back peek so the rider still hears about a
    // follow-up turn when the 50 m combined cue was skipped (sparse GPS,
    // fast cycling, or a tick that landed already inside 15 m).
    if (
      upcoming &&
      upcomingDistance !== undefined &&
      upcomingDistance <= APPROACH_10_M &&
      !announced10m.has(upcoming.id)
    ) {
      const upcomingIdx = snapshot.maneuvers.findIndex((m) => m.id === upcoming.id);
      const followUp = snapshot.maneuvers[upcomingIdx + 1];
      const gapToFollowUp = followUp
        ? followUp.distanceFromStartM - upcoming.distanceFromStartM
        : Number.POSITIVE_INFINITY;
      if (followUp && gapToFollowUp <= BACK_TO_BACK_THRESHOLD_M) {
        events.push({
          kind: "turn10m",
          turnKind: upcoming.kind,
          followUpKind: followUp.kind,
        });
        announced10m.add(upcoming.id);
        announced10m.add(followUp.id);
        announced50m.add(followUp.id);
        announcedNextTurnAfter.add(upcoming.id);
      } else {
        events.push({ kind: "turn10m", turnKind: upcoming.kind });
        announced10m.add(upcoming.id);
      }
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
        // Bug 1 fix: if nextAfter is the last cue maneuver and sits within
        // CLOSE_TO_DESTINATION_M of the route end, the rider is effectively
        // arriving — emit arrivingInM and suppress all approach cues for that
        // maneuver so the phantom "turn X" never plays.
        const distanceToNext = nextAfter.distanceFromStartM - snapshot.progressDistanceM;
        const isLastManeuver = indexOfLast + 1 === snapshot.maneuvers.length - 1;
        const distNextToEnd = snapshot.routeTotalDistanceM - nextAfter.distanceFromStartM;
        if (distanceToNext <= 0) {
          // Rider already passed the next maneuver (GPS jump in one tick).
          // Only emit arrivingInM if this was the last maneuver before arrival.
          if (isLastManeuver && !s.approachingDestinationAnnounced && !snapshot.arrived) {
            events.push({
              kind: "arrivingInM",
              distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM,
            });
            s = { ...s, approachingDestinationAnnounced: true };
          }
          announcedNextTurnAfter.add(lastPassed.id);
        } else if (isLastManeuver && distNextToEnd < CLOSE_TO_DESTINATION_M) {
          // Bug 4: when the rider has already crossed the arrival
          // radius, the dedicated `arrived` cue at the bottom of this
          // function speaks instead — emitting `arrivingInM` here too
          // produces a same-tick double cue with disagreeing distances
          // ("Arriving in 5 m" → "You have arrived").
          if (!s.approachingDestinationAnnounced && !snapshot.arrived) {
            events.push({
              kind: "arrivingInM",
              distanceM: snapshot.routeTotalDistanceM - snapshot.progressDistanceM,
            });
            s = { ...s, approachingDestinationAnnounced: true };
            announcedNextTurnAfter.add(lastPassed.id);
            announced50m.add(nextAfter.id);
            announced10m.add(nextAfter.id);
          }
        } else if (distanceToNext > APPROACH_50_M) {
          events.push({
            kind: "nextTurnInAbout",
            turnKind: nextAfter.kind,
            distanceM: distanceToNext,
          });
          if (distanceToNext < SKIP_50M_BELOW_DISTANCE_M) {
            announced50m.add(nextAfter.id);
          }
          announcedNextTurnAfter.add(lastPassed.id);
        } else {
          announcedNextTurnAfter.add(lastPassed.id);
        }
      } else if (!s.approachingDestinationAnnounced && !snapshot.arrived) {
        // Bug 4: skip arrivingInM when the rider has already crossed the
        // arrival radius — the `arrived` cue at the bottom of this
        // function speaks instead.
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
      offRouteTickCount,
      silenced,
      consecutiveOnRouteSamples,
      onTrackAnnounced,
      reroutingEpisodeCount,
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
      if (event.followUpKind) {
        return {
          key: "cue.turn10mCombined",
          values: {
            first: event.turnKind,
            second: event.followUpKind,
          },
        };
      }
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

/** Collapse maneuver kinds into the slugs the `cue.nextTurnInAbout.*`
 *  catalog supports. Exit ramps fold into their parent direction; the
 *  dedicated kinds keep their own slug. */
function nextTurnDirection(kind: ManeuverKind): ManeuverKind {
  switch (kind) {
    case "left":
    case "exitLeft":
      return "left";
    case "right":
    case "exitRight":
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
    case "generic":
      return "generic";
  }
}
