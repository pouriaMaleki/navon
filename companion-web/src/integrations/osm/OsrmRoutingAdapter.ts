import {
  type ActiveRouteSession,
  type CoordinatePoint,
  CURRENT_ROUTE_PACKAGE_VERSION,
  type NormalizedRoutePackage,
  type RouteAlternative,
  type RouteManeuver,
  type RouteManeuverType,
  type RoutePlanRequest,
  type RoutePreviewModel,
  type RouteProviderID,
} from "../../domain/models.js";
import type { RoutingProvider } from "../../domain/providers.js";
import { decodePolyline } from "../geo.js";
import { newAlternativeId, normalizedFromPreview } from "../routePackage.js";
import { buildSamplePreview } from "../sample/SampleRoutingAdapter.js";

const OSRM_BASE = "https://router.project-osrm.org/route/v1/bike";

type OsrmManeuver = { type: string; modifier?: string; location: [number, number] };
type OsrmStep = { distance: number; duration: number; name: string; maneuver: OsrmManeuver };
type OsrmLeg = { summary?: string; steps: OsrmStep[] };
type OsrmRoute = { distance: number; duration: number; geometry: string; legs: OsrmLeg[] };
type OsrmResponse = { code: string; message?: string; routes: OsrmRoute[] };

export class OsrmRoutingAdapter implements RoutingProvider {
  readonly providerID: RouteProviderID = "osm";

  async planRoute(request: RoutePlanRequest, signal?: AbortSignal): Promise<RoutePreviewModel> {
    return this.fetchOrFallback(request, 1, signal);
  }

  async replanRoute(
    session: ActiveRouteSession,
    riderLocation: CoordinatePoint,
    signal?: AbortSignal,
  ): Promise<RoutePreviewModel> {
    const request: RoutePlanRequest = {
      origin: riderLocation,
      destination: session.destinationCoordinate ?? riderLocation,
      providerID: this.providerID,
    };
    return this.fetchOrFallback(request, (session.routeRevision ?? 0) + 1, signal);
  }

  normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
    return normalizedFromPreview(preview, request);
  }

  private async fetchOrFallback(
    request: RoutePlanRequest,
    revision: number,
    signal?: AbortSignal,
  ): Promise<RoutePreviewModel> {
    try {
      return await this.fetchLive(request, revision, signal);
    } catch (err) {
      if ((err as Error)?.name === "AbortError") throw err;
      const message = err instanceof Error ? err.message : "Unknown error";
      return buildSamplePreview(
        "osm",
        request,
        revision,
        `Live OSM failed: ${message}. Showing sample route instead.`,
      );
    }
  }

  private async fetchLive(
    request: RoutePlanRequest,
    revision: number,
    signal?: AbortSignal,
  ): Promise<RoutePreviewModel> {
    const coords = `${request.origin.longitude.toFixed(6)},${request.origin.latitude.toFixed(6)};${request.destination.longitude.toFixed(6)},${request.destination.latitude.toFixed(6)}`;
    const url = `${OSRM_BASE}/${coords}?alternatives=3&overview=full&steps=true&geometries=polyline`;
    const response = await fetch(url, { signal });
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    const data = (await response.json()) as OsrmResponse;
    if (data.code !== "Ok") {
      throw new Error(data.message ?? `OSRM returned ${data.code}`);
    }
    if (data.routes.length === 0) {
      throw new Error("No alternatives available");
    }
    const alternatives = data.routes
      .slice(0, 3)
      .map((route, index) => mapAlternative(route, request, revision, index))
      .filter((a): a is RouteAlternative => a !== null);
    if (alternatives.length === 0) {
      throw new Error("No usable alternatives");
    }
    return {
      alternatives,
      selectedAlternativeID: alternatives[0].id,
      routeIdentifier: alternatives[0].normalizedPackage.routeIdentifier,
      routeRevision: alternatives[0].normalizedPackage.revision,
      planningNotice: "Live OSM bike routing via OSRM demo server",
    };
  }
}

function mapAlternative(
  route: OsrmRoute,
  request: RoutePlanRequest,
  revision: number,
  alternativeIndex: number,
): RouteAlternative | null {
  const geometry = decodePolyline(route.geometry);
  if (geometry.length < 2) return null;
  const maneuvers = buildLiveManeuvers(route, geometry);
  const durationSeconds = Math.max(Math.round(route.duration), 60);
  const package_: NormalizedRoutePackage = {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier: liveRouteId(request, alternativeIndex),
    revision,
    geometry,
    maneuvers,
    summary: {
      totalDistanceMeters: route.distance,
      estimatedDurationSeconds: durationSeconds,
      startLabel: "Current location",
      destinationLabel: "Selected destination",
    },
    provenance: {
      providerID: "osm",
      sourceReference: "OSRM bike route",
      generatedAtUnixMs: Date.now(),
    },
  };
  return {
    id: newAlternativeId(),
    title: alternativeIndex === 0 ? "OSRM primary route" : "OSRM alternative route",
    subtitle: route.legs[0]?.summary || "OSRM bike route",
    distanceMeters: Math.round(route.distance),
    durationSeconds,
    normalizedPackage: package_,
  };
}

function buildLiveManeuvers(route: OsrmRoute, geometry: CoordinatePoint[]): RouteManeuver[] {
  const maneuvers: RouteManeuver[] = [
    {
      id: "depart",
      maneuverType: "depart",
      location: geometry[0],
      distanceFromStartMeters: 0,
      distanceToNextMeters: firstStepDistance(route.legs),
      instructionText: "Start riding",
    },
  ];
  let distanceFromStart = 0;
  for (const leg of route.legs) {
    for (const step of leg.steps) {
      const stepDistance = step.distance;
      const t = step.maneuver.type.toLowerCase();
      if (
        t === "depart" ||
        t === "notification" ||
        t === "new name" ||
        t === "continue" ||
        t === "arrive"
      ) {
        distanceFromStart += stepDistance;
        continue;
      }
      if (step.maneuver.location.length < 2) {
        distanceFromStart += stepDistance;
        continue;
      }
      maneuvers.push({
        id: `step-${maneuvers.length}${step.name ? `-${step.name}` : ""}`,
        maneuverType: maneuverType(step),
        location: { latitude: step.maneuver.location[1], longitude: step.maneuver.location[0] },
        distanceFromStartMeters: distanceFromStart,
        distanceToNextMeters: stepDistance,
        instructionText: instructionText(step),
      });
      distanceFromStart += stepDistance;
    }
  }
  maneuvers.push({
    id: "arrive",
    maneuverType: "arrive",
    location: geometry[geometry.length - 1],
    distanceFromStartMeters: route.distance,
    instructionText: "Arrive at destination",
  });
  return maneuvers;
}

function firstStepDistance(legs: OsrmLeg[]): number | undefined {
  for (const leg of legs) {
    for (const step of leg.steps) {
      if (step.maneuver.type.toLowerCase() !== "depart") return step.distance;
    }
  }
  return undefined;
}

function maneuverType(step: OsrmStep): RouteManeuverType {
  const t = step.maneuver.type.toLowerCase();
  const m = step.maneuver.modifier?.toLowerCase() ?? "";
  if (t === "roundabout" || t === "rotary") return "roundabout";
  if (t === "merge" || t === "fork" || t === "on ramp" || t === "off ramp") return "merge";
  if (t === "arrive") return "arrive";
  switch (m) {
    case "uturn":
      return "uturn";
    case "sharp right":
      return "sharpRight";
    case "right":
      return "right";
    case "slight right":
      return "slightRight";
    case "sharp left":
      return "sharpLeft";
    case "left":
      return "left";
    case "slight left":
      return "slightLeft";
    default:
      return "straight";
  }
}

function instructionText(step: OsrmStep): string {
  const t = step.maneuver.type.toLowerCase();
  const m = step.maneuver.modifier?.toLowerCase() ?? "";
  if (t === "roundabout" || t === "rotary") return "Enter roundabout";
  if (t === "arrive") return "Arrive at destination";
  if (t === "merge") return "Merge";
  if (t === "fork")
    return m.includes("left")
      ? "Keep left"
      : m.includes("right")
        ? "Keep right"
        : "Keep to the fork";
  switch (m) {
    case "uturn":
      return "Make a U-turn";
    case "sharp right":
      return "Turn sharply right";
    case "right":
      return "Turn right";
    case "slight right":
      return "Bear right";
    case "sharp left":
      return "Turn sharply left";
    case "left":
      return "Turn left";
    case "slight left":
      return "Bear left";
    default:
      return step.name ? `Continue on ${step.name}` : "Continue";
  }
}

function liveRouteId(request: RoutePlanRequest, alternativeIndex: number): string {
  const o = `${request.origin.latitude.toFixed(5)},${request.origin.longitude.toFixed(5)}`;
  const d = `${request.destination.latitude.toFixed(5)},${request.destination.longitude.toFixed(5)}`;
  return `osm-live:${o}->${d}:alt-${alternativeIndex}`;
}
