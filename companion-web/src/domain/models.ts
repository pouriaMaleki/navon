export type CoordinatePoint = {
  latitude: number;
  longitude: number;
};

export type RouteProviderID = "hsl" | "osm" | "gpxImport" | "fitImport" | "tcxImport";

export const ROUTE_PROVIDER_DISPLAY_NAME: Record<RouteProviderID, string> = {
  hsl: "HSL",
  osm: "OSM",
  gpxImport: "GPX Import",
  fitImport: "FIT Import",
  tcxImport: "TCX Import",
};

export type RouteSourceMode = "mixed" | "hsl" | "osm";

export const ROUTE_SOURCE_MODE_DISPLAY_NAME: Record<RouteSourceMode, string> = {
  mixed: "Mixed",
  hsl: "HSL",
  osm: "OSM",
};

export const ROUTE_SOURCE_MODE_OPTIONS: RouteSourceMode[] = ["mixed", "hsl", "osm"];

export function primaryProviderID(mode: RouteSourceMode): RouteProviderID {
  return mode === "osm" ? "osm" : "hsl";
}

export function providerIDsForMode(mode: RouteSourceMode): RouteProviderID[] {
  if (mode === "mixed") return ["hsl", "osm"];
  return [mode];
}

export type RouteSuggestionKind = "fastest" | "quieter" | "simpler";

export const ROUTE_SUGGESTION_KIND_DISPLAY_NAME: Record<RouteSuggestionKind, string> = {
  fastest: "Fastest",
  quieter: "Quieter",
  simpler: "Simpler",
};

export type RouteSuggestionMode = "bestOnly" | "threeRoutes";

export const ROUTE_SUGGESTION_MODE_DISPLAY_NAME: Record<RouteSuggestionMode, string> = {
  bestOnly: "Best route",
  threeRoutes: "3 suggestions",
};

export type RouteStartBehavior = "explicit" | "automatic";

export const ROUTE_START_BEHAVIOR_DISPLAY_NAME: Record<RouteStartBehavior, string> = {
  explicit: "Ask before start",
  automatic: "Start automatically",
};

export type RouteManeuverType =
  | "depart"
  | "straight"
  | "slightLeft"
  | "left"
  | "sharpLeft"
  | "slightRight"
  | "right"
  | "sharpRight"
  | "uturn"
  | "roundabout"
  | "merge"
  | "ramp"
  | "arrive";

export type RouteManeuver = {
  id: string;
  maneuverType: RouteManeuverType;
  location: CoordinatePoint;
  distanceFromStartMeters: number;
  distanceToNextMeters?: number;
  instructionText?: string;
};

export type RouteSummary = {
  totalDistanceMeters: number;
  estimatedDurationSeconds: number;
  startLabel?: string;
  destinationLabel?: string;
};

export type RouteProvenance = {
  providerID: RouteProviderID;
  sourceReference?: string;
  generatedAtUnixMs: number;
};

export type RoutePackageVersion = { major: number; minor: number };

export const CURRENT_ROUTE_PACKAGE_VERSION: RoutePackageVersion = { major: 1, minor: 0 };

export type NormalizedRoutePackage = {
  version: RoutePackageVersion;
  routeIdentifier: string;
  revision: number;
  geometry: CoordinatePoint[];
  maneuvers: RouteManeuver[];
  summary: RouteSummary;
  provenance: RouteProvenance;
};

export function summaryLine(pkg: NormalizedRoutePackage): string {
  const minutes = Math.max(Math.floor(pkg.summary.estimatedDurationSeconds / 60), 1);
  return `${Math.round(pkg.summary.totalDistanceMeters)} m • ${minutes} min`;
}

export type RouteAlternative = {
  id: string;
  title: string;
  subtitle: string;
  distanceMeters: number;
  durationSeconds: number;
  normalizedPackage: NormalizedRoutePackage;
};

export type RoutePreviewModel = {
  alternatives: RouteAlternative[];
  selectedAlternativeID?: string;
  routeIdentifier?: string;
  routeRevision?: number;
  planningNotice?: string;
};

export function selectedAlternative(preview: RoutePreviewModel): RouteAlternative | undefined {
  if (preview.alternatives.length === 0) return undefined;
  if (preview.selectedAlternativeID) {
    return (
      preview.alternatives.find((a) => a.id === preview.selectedAlternativeID) ??
      preview.alternatives[0]
    );
  }
  return preview.alternatives[0];
}

export type RoutePlanRequest = {
  origin: CoordinatePoint;
  destination: CoordinatePoint;
  providerID: RouteProviderID;
};

export type ActiveRouteSession = {
  routeIdentifier?: string;
  routeRevision?: number;
  destinationLabel: string;
  destinationCoordinate?: CoordinatePoint;
  providerID: RouteProviderID;
  sourceMode: RouteSourceMode;
  lastRerouteReason?: string;
  lastRerouteTimestampMs?: number;
};

export const EMPTY_ACTIVE_SESSION: ActiveRouteSession = {
  destinationLabel: "No destination",
  providerID: "hsl",
  sourceMode: "mixed",
};

export type DestinationSearchResult = {
  id: string;
  title: string;
  subtitle: string;
  coordinate: CoordinatePoint;
};

export type RouteHistorySource =
  | "recentDestination"
  | "plannedRoute"
  | "gpxImport"
  | "googleMaps"
  | "shareImport";

export type RouteHistoryItem = {
  id: string;
  title: string;
  subtitle: string;
  source: RouteHistorySource;
  sourceLabel: string;
  createdAtMs: number;
  destination?: CoordinatePoint;
  routePackage?: NormalizedRoutePackage;
  occurrenceCount?: number;
};

export type PendingHomePresentation = {
  routeHistoryItemID: string;
  title: string;
  sourceLabel: string;
  destination?: CoordinatePoint;
  createdAtMs: number;
  debugTrail: string[];
};

export type SpeedUnit = "kph" | "mph";

export type CompanionSettings = {
  preferLiveHslRouting: boolean;
  hslSubscriptionKey: string;
  hslEndpointURL: string;
  /**
   * Cyclist's planning speed in km/h. Used to override route ETA so that
   * `estimatedDurationSeconds = totalDistanceMeters / (cyclingSpeedKph / 3.6)`.
   * Why: HSL Digitransit defaults a cautious bike speed and routinely returns
   * inflated ETAs; for a moving rider 14–22 kph is realistic. Applies to both
   * live and sample HSL itineraries.
   */
  cyclingSpeedKph: number;
  /** Display unit for live speed; persisted so all companions render the same. */
  speedUnit: SpeedUnit;
  /**
   * Persistent zoom level for riding-mode (phoneGuidance follow-rider).
   * The zoom +/- buttons in routing write here so the rider's preferred
   * "navigation zoom" survives across sessions. `null` falls back to the
   * built-in default of 16. Overview (north-preview / planning) zoom is
   * intentionally NOT persisted — see spec line 10: "in over view mode
   * user changes zoom, only keep it for moment".
   */
  ridingZoom: number | null;
  /** Prevents the screen from sleeping while a route is active (Wake Lock). */
  keepScreenOn: boolean;
  /**
   * Permission gate for background GPS. On the web this only requests the
   * geolocation prompt; iOS Safari does not actually deliver background fixes,
   * so the UI surfaces a platform-specific hint when this is on.
   */
  allowBackgroundGps: boolean;
  /**
   * Audio cues during routing. Stored default is true (see spec: "on by default"),
   * but the toggle is disabled in the UI and the cues are suppressed at runtime
   * unless allowBackgroundGps is also true.
   */
  audioCuesEnabled: boolean;
  /**
   * Lock-screen live activity. On web this surfaces a single self-updating
   * Notification. Disabled in the UI until allowBackgroundGps is on.
   */
  liveActivityEnabled: boolean;
};

export const DEFAULT_COMPANION_SETTINGS: CompanionSettings = {
  preferLiveHslRouting: false,
  hslSubscriptionKey: "",
  hslEndpointURL: "https://api.digitransit.fi/routing/v2/hsl/gtfs/v1",
  cyclingSpeedKph: 18,
  speedUnit: "kph",
  ridingZoom: null,
  keepScreenOn: false,
  allowBackgroundGps: false,
  audioCuesEnabled: true,
  liveActivityEnabled: false,
};

export type RoutePlannerPreferences = {
  defaultSourceMode: RouteSourceMode;
  suggestionMode: RouteSuggestionMode;
  startBehavior: RouteStartBehavior;
};

export const DEFAULT_PLANNER_PREFERENCES: RoutePlannerPreferences = {
  defaultSourceMode: "mixed",
  suggestionMode: "threeRoutes",
  startBehavior: "explicit",
};

export type ImportClassification =
  | "gpxFile"
  | "fitFile"
  | "tcxFile"
  | "googleMapsLocationLink"
  | "garminCourseLink"
  | "coordinatesText"
  | "genericUrl"
  | "unknown";

export type ImportDisposition = "directHomePreview" | "diagnosticsOnly";

export type SharedImportEnvelope = {
  id: string;
  receivedAtMs: number;
  rawKind: "url" | "plainText" | "file";
  fileName?: string;
  fileSizeBytes?: number;
  originalText?: string;
  originalURL?: string;
  classification: ImportClassification;
  disposition: ImportDisposition;
  note?: string;
  debugTrail: string[];
};

export type ImportDiagnosticsEntry = {
  id: string;
  envelope: SharedImportEnvelope;
  createdAtMs: number;
};

export type HomeMode = "planning" | "phoneGuidance";
export type HomeCompassMode = "autoFollow" | "northPreview" | "northLocked";

export type CompanionDiagnostics = {
  providerName: string;
  routeIdentifier: string;
  routeRevision: number;
  lastSyncResult: string;
  lastRerouteOutcome: string;
};
