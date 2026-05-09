import {
  type ActiveRouteSession,
  type CoordinatePoint,
  CURRENT_ROUTE_PACKAGE_VERSION,
  type NormalizedRoutePackage,
  ROUTE_PROVIDER_DISPLAY_NAME,
  type RouteAlternative,
  type RouteManeuver,
  type RoutePlanRequest,
  type RoutePreviewModel,
  type RouteProviderID,
} from "../../domain/models.js";
import type { RoutingProvider } from "../../domain/providers.js";
import {
  classifyTurn,
  cumulativeDistances,
  deduplicateConsecutive,
  totalDistanceMeters,
  turnDeltaDegrees,
} from "../geo.js";
import { newAlternativeId, normalizedFromPreview } from "../routePackage.js";

const PROVIDER_OFFSET: Record<RouteProviderID, number> = {
  hsl: 0.0018,
  osm: 0.002,
  gpxImport: 0.0014,
  fitImport: 0.0016,
  tcxImport: 0.0012,
};

const PROVIDER_AVG_MPS: Record<RouteProviderID, number> = {
  hsl: 5.3,
  osm: 5.2,
  gpxImport: 4.8,
  fitImport: 5.0,
  tcxImport: 4.7,
};

const PATTERNS: number[][] = [
  [0.3, 0.58, 0.18, -0.08, 0.26, 0.05],
  [-0.22, -0.46, -0.12, -0.34, -0.08, 0.03],
  [0.18, 0.06, 0.42, 0.2, 0.34, 0.09],
];

const FRACTIONS = [0.12, 0.24, 0.39, 0.56, 0.73, 0.88];

export class SampleRoutingAdapter implements RoutingProvider {
  constructor(public readonly providerID: RouteProviderID) {}

  async planRoute(request: RoutePlanRequest): Promise<RoutePreviewModel> {
    return buildSamplePreview(this.providerID, request, 1, samplePlanningNotice(this.providerID));
  }

  async replanRoute(
    session: ActiveRouteSession,
    riderLocation: CoordinatePoint,
  ): Promise<RoutePreviewModel> {
    const request: RoutePlanRequest = {
      origin: riderLocation,
      destination: session.destinationCoordinate ?? riderLocation,
      providerID: this.providerID,
    };
    const revision = (session.routeRevision ?? 0) + 1;
    return buildSamplePreview(
      this.providerID,
      request,
      revision,
      `Rerouted with ${ROUTE_PROVIDER_DISPLAY_NAME[this.providerID]} sample adapter`,
    );
  }

  normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
    return normalizedFromPreview(preview, request);
  }
}

export function samplePlanningNotice(provider: RouteProviderID): string {
  switch (provider) {
    case "osm":
      return "Using sample OSM fallback routes. Live OSRM bike routing is unavailable.";
    case "gpxImport":
      return "Using sample GPX import routes. File selection is not wired yet.";
    case "fitImport":
      return "Using sample FIT import routes. File selection is not wired yet.";
    case "tcxImport":
      return "Using sample TCX import routes. File selection is not wired yet.";
    case "hsl":
      return "Using sample HSL route";
  }
}

export function buildSamplePreview(
  providerID: RouteProviderID,
  request: RoutePlanRequest,
  revision: number,
  planningNotice: string,
): RoutePreviewModel {
  const alternatives = [0, 1, 2].map((index) =>
    buildAlternative(providerID, request, revision, index),
  );
  return {
    alternatives,
    selectedAlternativeID: alternatives[0]?.id,
    routeIdentifier: alternatives[0]?.normalizedPackage.routeIdentifier,
    routeRevision: alternatives[0]?.normalizedPackage.revision,
    planningNotice,
  };
}

function buildAlternative(
  providerID: RouteProviderID,
  request: RoutePlanRequest,
  revision: number,
  alternativeIndex: number,
): RouteAlternative {
  const geometry = sampleGeometry(
    request.origin,
    request.destination,
    providerID,
    alternativeIndex,
  );
  const maneuvers = buildSampleManeuvers(geometry);
  const distance = totalDistanceMeters(geometry);
  const durationSeconds = Math.max(Math.round(distance / PROVIDER_AVG_MPS[providerID]), 60);
  const package_: NormalizedRoutePackage = {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier: `${providerID}-sample-${alternativeIndex}`,
    revision,
    geometry,
    maneuvers,
    summary: {
      totalDistanceMeters: distance,
      estimatedDurationSeconds: durationSeconds,
      startLabel: "Current location",
      destinationLabel: `${ROUTE_PROVIDER_DISPLAY_NAME[providerID]} sample destination`,
    },
    provenance: {
      providerID,
      sourceReference: `${ROUTE_PROVIDER_DISPLAY_NAME[providerID]} sample adapter`,
      generatedAtUnixMs: Date.now(),
    },
  };
  return {
    id: newAlternativeId(),
    title: alternativeIndex === 0 ? "Sample primary route" : "Sample alternative route",
    subtitle: `${ROUTE_PROVIDER_DISPLAY_NAME[providerID]} sample ${alternativeIndex === 0 ? "primary" : "alternative"}`,
    distanceMeters: Math.round(distance),
    durationSeconds,
    normalizedPackage: package_,
  };
}

function sampleGeometry(
  origin: CoordinatePoint,
  destination: CoordinatePoint,
  providerID: RouteProviderID,
  alternativeIndex: number,
): CoordinatePoint[] {
  if (origin.latitude === destination.latitude && origin.longitude === destination.longitude) {
    return deduplicateConsecutive([
      origin,
      { latitude: origin.latitude + 0.0016, longitude: origin.longitude + 0.0008 },
      { latitude: origin.latitude + 0.0025, longitude: origin.longitude + 0.0017 },
      { latitude: origin.latitude + 0.0021, longitude: origin.longitude - 0.0006 },
      { latitude: origin.latitude + 0.0009, longitude: origin.longitude - 0.0018 },
      origin,
    ]);
  }
  const latDelta = destination.latitude - origin.latitude;
  const lonDelta = destination.longitude - origin.longitude;
  const length = Math.max(Math.sqrt(latDelta * latDelta + lonDelta * lonDelta), 0.0001);
  const perpendicularLat = -lonDelta / length;
  const perpendicularLon = latDelta / length;
  const baseOffset = PROVIDER_OFFSET[providerID] * (0.55 + alternativeIndex * 0.18);
  const pattern = PATTERNS[Math.min(alternativeIndex, PATTERNS.length - 1)];
  const geometry: CoordinatePoint[] = [origin];
  FRACTIONS.forEach((fraction, index) => {
    const lateral = baseOffset * pattern[index];
    const forwardBias = baseOffset * 0.12 * (index - 2.5);
    geometry.push({
      latitude:
        origin.latitude + latDelta * fraction + perpendicularLat * lateral + latDelta * forwardBias,
      longitude:
        origin.longitude +
        lonDelta * fraction +
        perpendicularLon * lateral +
        lonDelta * forwardBias,
    });
  });
  geometry.push(destination);
  return deduplicateConsecutive(geometry);
}

export function buildSampleManeuvers(geometry: CoordinatePoint[]): RouteManeuver[] {
  const cumulative = cumulativeDistances(geometry);
  const maneuvers: RouteManeuver[] = [
    {
      id: "depart",
      maneuverType: "depart",
      location: geometry[0] ?? { latitude: 0, longitude: 0 },
      distanceFromStartMeters: 0,
      distanceToNextMeters: cumulative[1],
      instructionText: "Start riding",
    },
  ];
  for (let i = 1; i < geometry.length - 1; i++) {
    const delta = turnDeltaDegrees(geometry[i - 1], geometry[i], geometry[i + 1]);
    const turn = classifyTurn(delta);
    if (!turn) continue;
    const distanceToNext =
      i + 1 < cumulative.length ? cumulative[i + 1] - cumulative[i] : undefined;
    maneuvers.push({
      id: `step-${i}`,
      maneuverType: turn.type,
      location: geometry[i],
      distanceFromStartMeters: cumulative[i],
      distanceToNextMeters: distanceToNext,
      instructionText: turn.instruction,
    });
  }
  maneuvers.push({
    id: "arrive",
    maneuverType: "arrive",
    location: geometry[geometry.length - 1] ?? { latitude: 0, longitude: 0 },
    distanceFromStartMeters: cumulative[cumulative.length - 1] ?? 0,
    instructionText: "Arrive at destination",
  });
  return maneuvers;
}
