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
  searchDestinations(
    query: string,
    limit: number,
    signal?: AbortSignal,
  ): Promise<DestinationSearchResult[]>;
  resolveDestination(
    coordinate: CoordinatePoint,
    fallbackTitle: string,
    signal?: AbortSignal,
  ): Promise<DestinationSearchResult | null>;
}
