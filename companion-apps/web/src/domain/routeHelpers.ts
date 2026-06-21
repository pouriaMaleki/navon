import type {
  ActiveRouteSession,
  CompanionSettings,
  NormalizedRoutePackage,
  RouteAlternative,
  RoutePackageVersion,
  RoutePlannerPreferences,
  RoutePreviewModel,
  RouteProviderID,
  RouteSourceMode,
  RouteStartBehavior,
  RouteSuggestionKind,
  RouteSuggestionMode,
} from "./models.js";

export const ROUTE_PROVIDER_DISPLAY_NAME: Record<RouteProviderID, string> = {
  hsl: "HSL",
  osm: "OSM",
  gpxImport: "GPX Import",
  fitImport: "FIT Import",
  tcxImport: "TCX Import",
};

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

export const ROUTE_SUGGESTION_KIND_DISPLAY_NAME: Record<RouteSuggestionKind, string> = {
  fastest: "Fastest",
  quieter: "Quieter",
  simpler: "Simpler",
};

export const ROUTE_SUGGESTION_MODE_DISPLAY_NAME: Record<RouteSuggestionMode, string> = {
  bestOnly: "Best route",
  threeRoutes: "3 suggestions",
};

export const ROUTE_START_BEHAVIOR_DISPLAY_NAME: Record<RouteStartBehavior, string> = {
  explicit: "Ask before start",
  automatic: "Start automatically",
};

export const CURRENT_ROUTE_PACKAGE_VERSION: RoutePackageVersion = { major: 1, minor: 0 };

export function summaryLine(pkg: NormalizedRoutePackage): string {
  const minutes = Math.max(Math.floor(pkg.summary.estimatedDurationSeconds / 60), 1);
  return `${Math.round(pkg.summary.totalDistanceMeters)} m • ${minutes} min`;
}

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

export const EMPTY_ACTIVE_SESSION: ActiveRouteSession = {
  destinationLabel: "No destination",
  providerID: "hsl",
  sourceMode: "mixed",
};

export const DEFAULT_COMPANION_SETTINGS: CompanionSettings = {
  hslEndpointURL: "/api/hsl/routing",
  cyclingSpeedKph: 18,
  speedUnit: "kph",
  ridingZoom: null,
  keepScreenOn: false,
  allowBackgroundGps: false,
  audioCuesEnabled: true,
  audioCuesOnlyInBackground: true,
  liveActivityEnabled: false,
  routingDiagnosticsEnabled: false,
  language: "system",
  distanceUnit: "system",
};

export const DEFAULT_PLANNER_PREFERENCES: RoutePlannerPreferences = {
  defaultSourceMode: "mixed",
  suggestionMode: "threeRoutes",
  startBehavior: "explicit",
};
