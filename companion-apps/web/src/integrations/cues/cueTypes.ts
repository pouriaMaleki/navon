// Audio cue trigger engine types and constants.
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
  | { kind: "turn50m"; turnKind: ManeuverKind; distanceM: number; followUpKind?: ManeuverKind }
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
  reroutingEpisodeCount: number;
  offRouteTickCount: number;
};

export const APPROACH_50_M = 50;
export const APPROACH_10_M = 15;
export const SKIP_50M_BELOW_DISTANCE_M = 100;
export const PASSED_TURN_M = 10;
export const ON_TRACK_CONFIRM_SAMPLES = 5;
export const ON_TRACK_CORRIDOR_M = 22;
export const OFF_ROUTE_HYSTERESIS_TICKS = 3;
export const OFF_ROUTE_IMMEDIATE_DISTANCE_M = 50;
export const REPEAT_OFFTRACK_SILENCE_THRESHOLD = 2;
export const REROUTING_CUE_CAP = 2;
export const CLOSE_TO_DESTINATION_M = 30;
export const BACK_TO_BACK_THRESHOLD_M = 30;
