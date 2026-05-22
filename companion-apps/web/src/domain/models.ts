export type CoordinatePoint = {
  latitude: number;
  longitude: number;
};

export type RouteProviderID = "hsl" | "osm" | "gpxImport" | "fitImport" | "tcxImport";

export type RouteSourceMode = "mixed" | "hsl" | "osm";

export type RouteSuggestionKind = "fastest" | "quieter" | "simpler";

export type RouteSuggestionMode = "bestOnly" | "threeRoutes";

export type RouteStartBehavior = "explicit" | "automatic";

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

export type NormalizedRoutePackage = {
  version: RoutePackageVersion;
  routeIdentifier: string;
  revision: number;
  geometry: CoordinatePoint[];
  maneuvers: RouteManeuver[];
  summary: RouteSummary;
  provenance: RouteProvenance;
};

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

export type RerouteContext = {
  headingDegrees?: number;
  speedMps?: number;
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
/** Mirror of `AppLanguage` in src/i18n/index.ts. Listed here too because
 *  `models.ts` is the long-lived persisted-settings shape and shouldn't
 *  import from the i18n runtime. Add new locales to BOTH unions plus
 *  `i18n/catalog.config.json:locales`. */
export type AppLanguagePref =
  | "system"
  | "ar"
  | "bn"
  | "de"
  | "en"
  | "es"
  | "fa"
  | "fi"
  | "fr"
  | "hi"
  | "id"
  | "ja"
  | "mr"
  | "pcm"
  | "pt"
  | "ru"
  | "ur"
  | "zh";
export type DistanceUnitPref = "system" | "metric" | "imperial";

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
   * Spec line 144: when true (the default), audio cues are suppressed
   * while the rider has the app foregrounded — their map is already
   * visible — and only fire once the page goes hidden (tab switch /
   * screen lock). Toggle this off to hear cues even when the app is
   * open.
   */
  audioCuesOnlyInBackground: boolean;
  /**
   * Lock-screen live activity. On web this surfaces a single self-updating
   * Notification. Disabled in the UI until allowBackgroundGps is on.
   */
  liveActivityEnabled: boolean;
  /**
   * When enabled, routing activities are recorded into timestamped
   * diagnostics sessions. Sessions persist across restarts and can be
   * shared as JSON debug packages or deleted individually.
   */
  routingDiagnosticsEnabled: boolean;
  /**
   * App language. `"system"` follows `navigator.languages`. Concrete values
   * (`"en"`, `"fi"`) override the OS default. New shipped locales must be
   * added to both this union and `i18n/catalog.config.json`.
   */
  language: AppLanguagePref;
  /**
   * Distance unit used for both UI labels and spoken voice cues. `"system"`
   * derives from the resolved locale (en-US → imperial, otherwise metric).
   */
  distanceUnit: DistanceUnitPref;
};

export type RoutePlannerPreferences = {
  defaultSourceMode: RouteSourceMode;
  suggestionMode: RouteSuggestionMode;
  startBehavior: RouteStartBehavior;
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

// Re-exports from routeHelpers for backwards compatibility
export {
  CURRENT_ROUTE_PACKAGE_VERSION,
  DEFAULT_COMPANION_SETTINGS,
  DEFAULT_PLANNER_PREFERENCES,
  EMPTY_ACTIVE_SESSION,
  primaryProviderID,
  providerIDsForMode,
  ROUTE_PROVIDER_DISPLAY_NAME,
  ROUTE_SOURCE_MODE_DISPLAY_NAME,
  ROUTE_SOURCE_MODE_OPTIONS,
  ROUTE_START_BEHAVIOR_DISPLAY_NAME,
  ROUTE_SUGGESTION_KIND_DISPLAY_NAME,
  ROUTE_SUGGESTION_MODE_DISPLAY_NAME,
  selectedAlternative,
  summaryLine,
} from "./routeHelpers.js";
