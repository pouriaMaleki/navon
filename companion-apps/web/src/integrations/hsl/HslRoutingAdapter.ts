import {
  type ActiveRouteSession,
  type CompanionSettings,
  type CoordinatePoint,
  CURRENT_ROUTE_PACKAGE_VERSION,
  type NormalizedRoutePackage,
  type RerouteContext,
  type RouteAlternative,
  type RouteManeuver,
  type RouteManeuverType,
  type RoutePlanRequest,
  type RoutePreviewModel,
  type RouteProviderID,
} from "../../domain/models.js";
import type { RoutingProvider } from "../../domain/providers.js";
import {
  approximateDistanceMeters,
  classifyTurn,
  cumulativeDistances,
  decodePolyline,
  deduplicateConsecutive,
  turnDeltaDegrees,
} from "../geo.js";
import { newAlternativeId, normalizedFromPreview } from "../routePackage.js";

export type HslSettingsProvider = () => CompanionSettings;

const ROUTE_PLAN_QUERY = `query RoutePlan($from: InputCoordinates!, $to: InputCoordinates!, $numItineraries: Int!, $transportModes: [TransportMode!]!, $optimize: OptimizeType!) {
  plan(from: $from, to: $to, numItineraries: $numItineraries, transportModes: $transportModes, optimize: $optimize) {
    itineraries { duration legs { mode distance from { lat lon name } to { lat lon name } legGeometry { points } } }
  }
}`;

type LivePlace = { lat: number; lon: number; name?: string };
type LiveLeg = {
  mode?: string;
  distance: number;
  from?: LivePlace;
  to?: LivePlace;
  legGeometry?: { points: string };
};
type LiveItinerary = { duration: number; legs: LiveLeg[] };
type DigitransitApiResponse = {
  data?: { plan?: { itineraries: LiveItinerary[] } };
  errors?: { message: string }[];
};

type Itinerary = {
  durationSeconds: number;
  systemNotice: string;
  legs: { distanceMeters: number; geometry: CoordinatePoint[] }[];
  startLabel: string;
  destinationLabel: string;
};

export class HslRoutingAdapter implements RoutingProvider {
  readonly providerID: RouteProviderID = "hsl";

  constructor(private readonly settingsProvider: HslSettingsProvider) {}

  async planRoute(request: RoutePlanRequest, signal?: AbortSignal): Promise<RoutePreviewModel> {
    return this.planPreview(request, undefined, signal);
  }

  async replanRoute(
    session: ActiveRouteSession,
    riderLocation: CoordinatePoint,
    rerouteContext?: RerouteContext,
    signal?: AbortSignal,
  ): Promise<RoutePreviewModel> {
    const rerouteOrigin = headingBiasedOrigin(riderLocation, rerouteContext, "hsl");
    const request: RoutePlanRequest = {
      origin: rerouteOrigin,
      destination: session.destinationCoordinate ?? riderLocation,
      providerID: session.providerID,
    };
    const revision = session.routeRevision === undefined ? undefined : session.routeRevision + 1;
    return this.planPreview(request, revision, signal);
  }

  normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
    return normalizedFromPreview(preview, request);
  }

  private async planPreview(
    request: RoutePlanRequest,
    revisionOverride: number | undefined,
    signal: AbortSignal | undefined,
  ): Promise<RoutePreviewModel> {
    const settings = this.settingsProvider();
    const cyclingSpeedKph = settings.cyclingSpeedKph;
    if (settings.preferLiveHslRouting) {
      const trimmedKey = settings.hslSubscriptionKey.trim();
      if (!trimmedKey) {
        return normalizeItineraries(
          sampleItineraries(request, "Fallback sample: missing HSL subscription key"),
          request,
          revisionOverride,
          "No HSL subscription key configured. Showing sample route instead.",
          cyclingSpeedKph,
        );
      }
      try {
        const live = await fetchLive(request, settings, signal);
        return normalizeItineraries(
          live,
          request,
          revisionOverride,
          "Live HSL Digitransit",
          cyclingSpeedKph,
        );
      } catch (err) {
        if ((err as Error)?.name === "AbortError") throw err;
        const message = err instanceof Error ? err.message : "Unknown error";
        return normalizeItineraries(
          sampleItineraries(request, "Fallback sample after live HSL failure"),
          request,
          revisionOverride,
          `Live HSL failed: ${message}. Showing sample route instead.`,
          cyclingSpeedKph,
        );
      }
    }
    return normalizeItineraries(
      sampleItineraries(request, "Sample HSL route"),
      request,
      revisionOverride,
      "Using sample HSL routes. Enable live HSL in Settings.",
      cyclingSpeedKph,
    );
  }
}

const MIN_HEADING_SPEED_MPS = 2.0;
const REROUTE_FORWARD_SHIFT_M = 15.0;

function headingBiasedOrigin(
  riderLocation: CoordinatePoint,
  rerouteContext: RerouteContext | undefined,
  providerLabel: string,
): CoordinatePoint {
  const heading = rerouteContext?.headingDegrees;
  const speedMps = rerouteContext?.speedMps;
  if (heading == null || !Number.isFinite(heading)) {
    console.debug(`[reroute_heading] provider=${providerLabel} reason=no_heading`);
    return riderLocation;
  }
  if (speedMps == null || !Number.isFinite(speedMps) || speedMps < MIN_HEADING_SPEED_MPS) {
    console.debug(`[reroute_heading] provider=${providerLabel} reason=low_speed speed=${speedMps ?? "nil"}`);
    return riderLocation;
  }
  const shifted = shiftPointByHeading(riderLocation, heading, REROUTE_FORWARD_SHIFT_M);
  if (
    !Number.isFinite(shifted.latitude) ||
    !Number.isFinite(shifted.longitude) ||
    (shifted.latitude === riderLocation.latitude && shifted.longitude === riderLocation.longitude)
  ) {
    console.debug(`[reroute_heading] provider=${providerLabel} reason=shift_failed`);
    return riderLocation;
  }
  console.debug(`[reroute_heading] provider=${providerLabel} reason=applied`);
  return shifted;
}

function shiftPointByHeading(
  point: CoordinatePoint,
  headingDegrees: number,
  distanceMeters: number,
): CoordinatePoint {
  const metersPerDegLat = 111_320.0;
  const rad = (headingDegrees * Math.PI) / 180;
  const northM = Math.cos(rad) * distanceMeters;
  const eastM = Math.sin(rad) * distanceMeters;
  const lat = point.latitude + northM / metersPerDegLat;
  const latRad = (point.latitude * Math.PI) / 180;
  const lonScale = metersPerDegLat * Math.cos(latRad);
  const lon = lonScale === 0 ? point.longitude : point.longitude + eastM / lonScale;
  return { latitude: lat, longitude: lon };
}

async function fetchLive(
  request: RoutePlanRequest,
  settings: CompanionSettings,
  signal?: AbortSignal,
): Promise<Itinerary[]> {
  const body = {
    query: ROUTE_PLAN_QUERY,
    variables: {
      from: { lat: request.origin.latitude, lon: request.origin.longitude },
      to: { lat: request.destination.latitude, lon: request.destination.longitude },
      numItineraries: 3,
      transportModes: [{ mode: "BICYCLE" }],
      optimize: "SAFE",
    },
  };
  const response = await fetch(settings.hslEndpointURL, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "digitransit-subscription-key": settings.hslSubscriptionKey,
    },
    body: JSON.stringify(body),
    signal,
  });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  const decoded = (await response.json()) as DigitransitApiResponse;
  if (decoded.errors && decoded.errors.length > 0) {
    throw new Error(decoded.errors.map((e) => e.message).join(" | "));
  }
  const itineraries = decoded.data?.plan?.itineraries ?? [];
  if (itineraries.length === 0) {
    throw new Error("No HSL route alternatives were returned.");
  }
  return itineraries.map((itinerary, index) => ({
    durationSeconds: Math.round(itinerary.duration),
    systemNotice:
      index === 0 ? "HSL Digitransit live / fastest" : "HSL Digitransit live / alternative",
    legs: itinerary.legs
      .map((leg) => mapLiveLeg(leg))
      .filter(
        (leg): leg is { distanceMeters: number; geometry: CoordinatePoint[] } => leg !== null,
      ),
    startLabel: itinerary.legs[0]?.from?.name ?? "Current location",
    destinationLabel: itinerary.legs[itinerary.legs.length - 1]?.to?.name ?? "Selected destination",
  }));
}

function mapLiveLeg(leg: LiveLeg): { distanceMeters: number; geometry: CoordinatePoint[] } | null {
  const polylineGeometry = decodePolyline(leg.legGeometry?.points ?? "");
  const fallback: CoordinatePoint[] = [];
  if (leg.from) fallback.push({ latitude: leg.from.lat, longitude: leg.from.lon });
  if (leg.to) {
    const point = { latitude: leg.to.lat, longitude: leg.to.lon };
    const last = fallback[fallback.length - 1];
    if (!last || last.latitude !== point.latitude || last.longitude !== point.longitude) {
      fallback.push(point);
    }
  }
  const geometry = polylineGeometry.length >= 2 ? polylineGeometry : fallback;
  if (geometry.length < 2) return null;
  return { distanceMeters: leg.distance, geometry };
}

function normalizeItineraries(
  itineraries: Itinerary[],
  request: RoutePlanRequest,
  revisionOverride: number | undefined,
  planningNotice: string | undefined,
  cyclingSpeedKph: number,
): RoutePreviewModel {
  const alternatives = itineraries.map((itinerary, index) =>
    normalizeItinerary(itinerary, request, index, revisionOverride ?? 1, cyclingSpeedKph),
  );
  return {
    alternatives,
    selectedAlternativeID: alternatives[0]?.id,
    routeIdentifier: alternatives[0]?.normalizedPackage.routeIdentifier,
    routeRevision: alternatives[0]?.normalizedPackage.revision,
    planningNotice,
  };
}

function normalizeItinerary(
  itinerary: Itinerary,
  request: RoutePlanRequest,
  alternativeIndex: number,
  revision: number,
  cyclingSpeedKph: number,
): RouteAlternative {
  const geometry = deduplicatedGeometryFromLegs(itinerary.legs);
  const totalDistance = itinerary.legs.reduce((sum, leg) => sum + leg.distanceMeters, 0);
  const maneuvers = buildManeuvers(geometry, totalDistance);
  // Digitransit's bike speed is conservative for actual riders; recompute the
  // ETA from the user-set cycling speed so listed times match real-world riding.
  const durationSeconds = overrideDurationSeconds(
    totalDistance,
    cyclingSpeedKph,
    itinerary.durationSeconds,
  );
  const package_: NormalizedRoutePackage = {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier: buildRouteIdentifier(request, alternativeIndex),
    revision,
    geometry,
    maneuvers,
    summary: {
      totalDistanceMeters: totalDistance,
      estimatedDurationSeconds: durationSeconds,
      startLabel: itinerary.startLabel,
      destinationLabel: itinerary.destinationLabel,
    },
    provenance: {
      providerID: "hsl",
      sourceReference: itinerary.systemNotice,
      generatedAtUnixMs: Date.now(),
    },
  };
  return {
    id: newAlternativeId(),
    title: alternativeIndex === 0 ? "Fastest bike route" : "Alternative bike route",
    subtitle: itinerary.systemNotice,
    distanceMeters: Math.round(totalDistance),
    durationSeconds,
    normalizedPackage: package_,
  };
}

function overrideDurationSeconds(
  totalDistanceMeters: number,
  cyclingSpeedKph: number,
  fallbackSeconds: number,
): number {
  if (!Number.isFinite(cyclingSpeedKph) || cyclingSpeedKph <= 0) return fallbackSeconds;
  const mps = cyclingSpeedKph / 3.6;
  return Math.max(1, Math.round(totalDistanceMeters / mps));
}

function deduplicatedGeometryFromLegs(legs: { geometry: CoordinatePoint[] }[]): CoordinatePoint[] {
  const points: CoordinatePoint[] = [];
  for (const leg of legs) {
    for (const point of leg.geometry) {
      const last = points[points.length - 1];
      if (!last || last.latitude !== point.latitude || last.longitude !== point.longitude) {
        points.push(point);
      }
    }
  }
  return points;
}

function buildManeuvers(geometry: CoordinatePoint[], totalDistance: number): RouteManeuver[] {
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
      id: `step-${maneuvers.length}`,
      maneuverType: turn.type as RouteManeuverType,
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
    distanceFromStartMeters: totalDistance,
    instructionText: "Arrive at destination",
  });
  return maneuvers;
}

function buildRouteIdentifier(request: RoutePlanRequest, alternativeIndex: number): string {
  const o = `${request.origin.latitude.toFixed(5)},${request.origin.longitude.toFixed(5)}`;
  const d = `${request.destination.latitude.toFixed(5)},${request.destination.longitude.toFixed(5)}`;
  return `hsl:${o}->${d}:alt-${alternativeIndex}`;
}

function sampleItineraries(request: RoutePlanRequest, liveDescriptor: string): Itinerary[] {
  const variants: { tag: string; offsetScale: number; pattern: number[] }[] = [
    {
      tag: "fastest",
      offsetScale: 0.0013,
      pattern: [0.26, 0.52, 0.22, 0.0, 0.16, 0.04],
    },
    {
      tag: "quieter",
      offsetScale: 0.0016,
      pattern: [-0.2, -0.42, -0.18, -0.28, -0.1, 0.02],
    },
    {
      tag: "simpler",
      offsetScale: 0.001,
      pattern: [0.12, 0.06, 0.28, 0.14, 0.24, 0.08],
    },
  ];
  return variants.map(({ tag, offsetScale, pattern }) => {
    const geometry = sampleGeometry(request.origin, request.destination, offsetScale, pattern);
    const distance = geometry
      .slice(1)
      .reduce((sum, p, i) => sum + approximateDistanceMeters(geometry[i], p), 0);
    return {
      durationSeconds: Math.round(distance / 4.2),
      systemNotice: `${liveDescriptor} / ${tag}`,
      legs: [{ distanceMeters: distance, geometry }],
      startLabel: "Current location",
      destinationLabel: "Selected destination",
    };
  });
}

function sampleGeometry(
  origin: CoordinatePoint,
  destination: CoordinatePoint,
  offsetScale: number,
  pattern: number[],
): CoordinatePoint[] {
  if (origin.latitude === destination.latitude && origin.longitude === destination.longitude) {
    return deduplicateConsecutive([
      origin,
      { latitude: origin.latitude + 0.0015, longitude: origin.longitude + 0.0009 },
      { latitude: origin.latitude + 0.0024, longitude: origin.longitude + 0.0016 },
      { latitude: origin.latitude + 0.0019, longitude: origin.longitude - 0.0004 },
      { latitude: origin.latitude + 0.0008, longitude: origin.longitude - 0.0016 },
      origin,
    ]);
  }
  const latDelta = destination.latitude - origin.latitude;
  const lonDelta = destination.longitude - origin.longitude;
  const length = Math.max(Math.sqrt(latDelta * latDelta + lonDelta * lonDelta), 0.0001);
  const perpendicularLat = -lonDelta / length;
  const perpendicularLon = latDelta / length;
  const fractions = [0.1, 0.22, 0.38, 0.54, 0.72, 0.88];
  const geometry: CoordinatePoint[] = [origin];
  fractions.forEach((fraction, index) => {
    const lateral = offsetScale * pattern[index];
    const forwardBias = offsetScale * 0.1 * (index - 2.5);
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
  return geometry;
}
