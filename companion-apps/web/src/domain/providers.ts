import type {
  ActiveRouteSession,
  CoordinatePoint,
  DestinationSearchResult,
  NormalizedRoutePackage,
  RoutePlanRequest,
  RoutePreviewModel,
  RouteProviderID,
} from "./models.js";

export interface RoutingProvider {
  readonly providerID: RouteProviderID;
  planRoute(request: RoutePlanRequest, signal?: AbortSignal): Promise<RoutePreviewModel>;
  replanRoute(
    session: ActiveRouteSession,
    riderLocation: CoordinatePoint,
    signal?: AbortSignal,
  ): Promise<RoutePreviewModel>;
  normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage;
}

export interface PlaceSearchService {
  /**
   * Search for destinations matching `query`, optionally biased toward
   * `riderBias` so nearby results rank first. See `docs/ux-specs.md` line 75:
   * "it suggests locations in the drop down from the same city or area".
   */
  searchDestinations(
    query: string,
    limit: number,
    riderBias?: CoordinatePoint,
    signal?: AbortSignal,
  ): Promise<DestinationSearchResult[]>;
  resolveDestination(
    coordinate: CoordinatePoint,
    fallbackTitle: string,
    signal?: AbortSignal,
  ): Promise<DestinationSearchResult | null>;
}
