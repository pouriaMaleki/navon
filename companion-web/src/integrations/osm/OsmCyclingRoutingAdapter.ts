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
import { newAlternativeId, normalizedFromPreview } from "../routePackage.js";
import { buildSamplePreview } from "../sample/SampleRoutingAdapter.js";
import { type BrouterProfile, fetchBrouter } from "./brouter/BrouterClient.js";
import { mapBrouterToAlternative } from "./brouter/mapBrouterToRoute.js";

/**
 * OSM cycling routing orchestrator. Issues parallel requests to BRouter
 * (`fastbike` paths-preferred + `trekking` balanced) AND OSRM bike, then
 * exposes whichever succeed as 1-3 alternatives in the route preview.
 *
 * Why multi-source: no single backend is universally best. BRouter
 * profiles each have different cycle-infra biases; OSRM's bike profile
 * is more direct. Showing all three lets the rider pick the line that
 * looks right on the map for their trip. See
 * `docs/companion-app-architecture.md` "OSM cycling sources".
 */
const OSRM_BASE = "https://router.project-osrm.org/route/v1/bike";

type OsrmManeuver = { type: string; modifier?: string; location: [number, number] };
type OsrmStep = { distance: number; duration: number; name: string; maneuver: OsrmManeuver };
type OsrmLeg = { summary?: string; steps: OsrmStep[] };
type OsrmRoute = {
  distance: number;
  duration: number;
  geometry: { type: "LineString"; coordinates: number[][] };
  legs: OsrmLeg[];
};
type OsrmResponse = { code: string; message?: string; routes: OsrmRoute[] };

type SourceTask = {
  label: string;
  run: () => Promise<RouteAlternative | null>;
  /** Stable key used in dedupe and to spot which source produced the alt. */
  sourceKey: BrouterProfile | "osrm-bike";
};

export class OsmCyclingRoutingAdapter implements RoutingProvider {
  readonly providerID: RouteProviderID = "osm";

  async planRoute(request: RoutePlanRequest, signal?: AbortSignal): Promise<RoutePreviewModel> {
    return this.fanOutOrFallback(request, 1, signal);
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
    return this.fanOutOrFallback(request, (session.routeRevision ?? 0) + 1, signal);
  }

  normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
    return normalizedFromPreview(preview, request);
  }

  private async fanOutOrFallback(
    request: RoutePlanRequest,
    revision: number,
    signal?: AbortSignal,
  ): Promise<RoutePreviewModel> {
    const tasks: SourceTask[] = [
      {
        label: "Bike-paths first",
        sourceKey: "fastbike",
        run: async () => this.runBrouter("fastbike", "Bike-paths first", request, revision, signal),
      },
      {
        label: "Balanced cycling",
        sourceKey: "trekking",
        run: async () => this.runBrouter("trekking", "Balanced cycling", request, revision, signal),
      },
      {
        label: "Fastest",
        sourceKey: "osrm-bike",
        run: async () => this.runOsrm(request, revision, signal),
      },
    ];
    const settled = await Promise.allSettled(tasks.map((t) => t.run()));
    if (signal?.aborted) throw new DOMException("Aborted", "AbortError");
    const ok: { task: SourceTask; alt: RouteAlternative }[] = [];
    for (let i = 0; i < settled.length; i++) {
      const r = settled[i];
      if (r.status === "fulfilled" && r.value) ok.push({ task: tasks[i], alt: r.value });
    }
    const deduped = dedupeAlternatives(ok);
    if (deduped.length === 0) {
      return buildSamplePreview(
        "osm",
        request,
        revision,
        "Showing sample route — live routing failed",
      );
    }
    const failedCount = tasks.length - ok.length;
    return {
      alternatives: deduped.map((d) => d.alt),
      selectedAlternativeID: deduped[0].alt.id,
      routeIdentifier: deduped[0].alt.normalizedPackage.routeIdentifier,
      routeRevision: deduped[0].alt.normalizedPackage.revision,
      planningNotice:
        failedCount === 0
          ? "Cycling alternatives via BRouter + OSRM"
          : `Cycling alternatives — ${failedCount} source${failedCount === 1 ? "" : "s"} unavailable`,
    };
  }

  private async runBrouter(
    profile: BrouterProfile,
    title: string,
    request: RoutePlanRequest,
    revision: number,
    signal?: AbortSignal,
  ): Promise<RouteAlternative | null> {
    const feature = await fetchBrouter(profile, request.origin, request.destination, signal);
    return mapBrouterToAlternative(feature, request, revision, { title, profile });
  }

  private async runOsrm(
    request: RoutePlanRequest,
    revision: number,
    signal?: AbortSignal,
  ): Promise<RouteAlternative | null> {
    const coords = `${request.origin.longitude.toFixed(6)},${request.origin.latitude.toFixed(6)};${request.destination.longitude.toFixed(6)},${request.destination.latitude.toFixed(6)}`;
    const url = `${OSRM_BASE}/${coords}?alternatives=false&overview=full&steps=true&geometries=geojson`;
    const response = await fetch(url, { signal });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const data = (await response.json()) as OsrmResponse;
    if (data.code !== "Ok") throw new Error(data.message ?? `OSRM returned ${data.code}`);
    if (data.routes.length === 0) throw new Error("OSRM returned no routes");
    return mapOsrmToAlternative(data.routes[0], request, revision);
  }
}

function dedupeAlternatives(
  candidates: { task: SourceTask; alt: RouteAlternative }[],
): { task: SourceTask; alt: RouteAlternative }[] {
  const kept: { task: SourceTask; alt: RouteAlternative }[] = [];
  for (const c of candidates) {
    const dup = kept.find((k) => alternativesAreNearIdentical(k.alt, c.alt));
    if (!dup) kept.push(c);
  }
  return kept;
}

function alternativesAreNearIdentical(a: RouteAlternative, b: RouteAlternative): boolean {
  const aLen = a.normalizedPackage.summary.totalDistanceMeters;
  const bLen = b.normalizedPackage.summary.totalDistanceMeters;
  if (aLen <= 0 || bLen <= 0) return false;
  const lengthDelta = Math.abs(aLen - bLen) / Math.max(aLen, bLen);
  if (lengthDelta > 0.03) return false;
  const ag = a.normalizedPackage.geometry;
  const bg = b.normalizedPackage.geometry;
  if (ag.length === 0 || bg.length === 0) return false;
  return (
    samePoint(ag[0], bg[0]) &&
    samePoint(ag[ag.length - 1], bg[bg.length - 1]) &&
    samePoint(ag[Math.floor(ag.length / 2)], bg[Math.floor(bg.length / 2)])
  );
}

function samePoint(a: CoordinatePoint, b: CoordinatePoint): boolean {
  return Math.abs(a.latitude - b.latitude) < 1e-4 && Math.abs(a.longitude - b.longitude) < 1e-4;
}

function mapOsrmToAlternative(
  route: OsrmRoute,
  request: RoutePlanRequest,
  revision: number,
): RouteAlternative | null {
  const geometry: CoordinatePoint[] = (route.geometry?.coordinates ?? []).map((c) => ({
    longitude: c[0],
    latitude: c[1],
  }));
  if (geometry.length < 2) return null;
  const maneuvers = buildOsrmManeuvers(route, geometry);
  const durationSeconds = Math.max(Math.round(route.duration), 60);
  const package_: NormalizedRoutePackage = {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier: osrmRouteId(request),
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
      sourceReference: "OSRM bike",
      generatedAtUnixMs: Date.now(),
    },
  };
  const km = (route.distance / 1000).toFixed(1);
  const min = Math.max(Math.round(durationSeconds / 60), 1);
  return {
    id: newAlternativeId(),
    title: "Fastest",
    subtitle: `${km} km • ${min} min`,
    distanceMeters: Math.round(route.distance),
    durationSeconds,
    normalizedPackage: package_,
  };
}

function buildOsrmManeuvers(route: OsrmRoute, geometry: CoordinatePoint[]): RouteManeuver[] {
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
        maneuverType: osrmManeuverType(step),
        location: { latitude: step.maneuver.location[1], longitude: step.maneuver.location[0] },
        distanceFromStartMeters: distanceFromStart,
        distanceToNextMeters: stepDistance,
        instructionText: osrmInstructionText(step),
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

function osrmManeuverType(step: OsrmStep): RouteManeuverType {
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

function osrmInstructionText(step: OsrmStep): string {
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

function osrmRouteId(request: RoutePlanRequest): string {
  const o = `${request.origin.latitude.toFixed(5)},${request.origin.longitude.toFixed(5)}`;
  const d = `${request.destination.latitude.toFixed(5)},${request.destination.longitude.toFixed(5)}`;
  return `osm-osrm:${o}->${d}`;
}
