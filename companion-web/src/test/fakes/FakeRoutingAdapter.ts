import type {
  ActiveRouteSession,
  CoordinatePoint,
  NormalizedRoutePackage,
  RoutePlanRequest,
  RoutePreviewModel,
  RouteProviderID,
} from "../../domain/models.js";
import { CURRENT_ROUTE_PACKAGE_VERSION } from "../../domain/models.js";
import type { RoutingProvider } from "../../domain/providers.js";

/**
 * Test double for `RoutingProvider`. Returns a baked preview without hitting
 * any network. Tests can seed `nextPreview` per call or pass a factory via
 * `planFactory` to vary the response.
 */
export class FakeRoutingAdapter implements RoutingProvider {
  readonly providerID: RouteProviderID;
  nextPreview?: RoutePreviewModel;
  planFactory?: (request: RoutePlanRequest) => RoutePreviewModel;
  planCalls: RoutePlanRequest[] = [];
  replanCalls: Array<{ session: ActiveRouteSession; rider: CoordinatePoint }> = [];

  constructor(providerID: RouteProviderID = "osm") {
    this.providerID = providerID;
  }

  async planRoute(request: RoutePlanRequest): Promise<RoutePreviewModel> {
    this.planCalls.push(request);
    if (this.planFactory) return this.planFactory(request);
    if (this.nextPreview) return this.nextPreview;
    return buildSimplePreview(this.providerID, request);
  }

  async replanRoute(
    session: ActiveRouteSession,
    rider: CoordinatePoint,
  ): Promise<RoutePreviewModel> {
    this.replanCalls.push({ session, rider });
    return buildSimplePreview(this.providerID, {
      origin: rider,
      destination: session.destinationCoordinate ?? rider,
      providerID: this.providerID,
    });
  }

  normalizePreview(preview: RoutePreviewModel, _request: RoutePlanRequest): NormalizedRoutePackage {
    return (
      preview.alternatives[0]?.normalizedPackage ??
      buildSimplePreview(this.providerID, _request).alternatives[0]!.normalizedPackage
    );
  }
}

export function buildSimplePreview(
  providerID: RouteProviderID,
  request: RoutePlanRequest,
): RoutePreviewModel {
  const label = providerID === "hsl" ? "HSL Route 1" : "Route 1";
  const distance = approxDistance(request.origin, request.destination);
  const durationSec = Math.max(60, Math.round(distance / 4.5));
  const normalized: NormalizedRoutePackage = {
    version: CURRENT_ROUTE_PACKAGE_VERSION,
    routeIdentifier: `${providerID}-fake-${distance.toFixed(0)}`,
    revision: 1,
    geometry: [request.origin, request.destination],
    maneuvers: [
      {
        id: `${providerID}-depart`,
        maneuverType: "depart",
        location: request.origin,
        distanceFromStartMeters: 0,
        distanceToNextMeters: distance,
        instructionText: "Head toward destination",
      },
      {
        id: `${providerID}-arrive`,
        maneuverType: "arrive",
        location: request.destination,
        distanceFromStartMeters: distance,
        instructionText: "Arrive at destination",
      },
    ],
    summary: {
      totalDistanceMeters: distance,
      estimatedDurationSeconds: durationSec,
    },
    provenance: {
      providerID,
      generatedAtUnixMs: 1_700_000_000_000,
    },
  };
  return {
    alternatives: [
      {
        id: `${providerID}-alt-1`,
        title: label,
        subtitle: `${Math.round(distance)} m • ${Math.round(durationSec / 60)} min`,
        distanceMeters: Math.round(distance),
        durationSeconds: durationSec,
        normalizedPackage: normalized,
      },
    ],
  };
}

function approxDistance(a: CoordinatePoint, b: CoordinatePoint): number {
  const metersPerDegreeLat = 111_320.0;
  const latMeters = (b.latitude - a.latitude) * metersPerDegreeLat;
  const meanLat = ((a.latitude + b.latitude) / 2) * (Math.PI / 180);
  const lonMeters = (b.longitude - a.longitude) * Math.cos(meanLat) * metersPerDegreeLat;
  return Math.sqrt(latMeters * latMeters + lonMeters * lonMeters);
}
