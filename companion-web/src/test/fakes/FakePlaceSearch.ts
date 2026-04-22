import type {
  CoordinatePoint,
  DestinationSearchResult,
} from "../../domain/models.js";
import type { PlaceSearchService } from "../../domain/providers.js";

/**
 * Test double for `PlaceSearchService`. `nextResults` is returned from
 * `searchDestinations`, `nextResolve` from `resolveDestination`.
 */
export class FakePlaceSearch implements PlaceSearchService {
  nextResults: DestinationSearchResult[] = [];
  nextResolve: DestinationSearchResult | null = null;
  searchCalls: Array<{
    query: string;
    limit: number;
    bias?: CoordinatePoint;
  }> = [];
  /**
   * Last rider-location bias seen by `searchDestinations`. The
   * `PlaceSearchService` interface currently has no bias argument, so this
   * stays undefined until the interface grows one. The area-bias flow test
   * asserts on this field.
   */
  lastQueryBias: CoordinatePoint | undefined;
  resolveCalls: Array<{ coordinate: CoordinatePoint; fallbackTitle: string }> = [];
  searchDelayMs = 0;

  async searchDestinations(
    query: string,
    limit: number,
    // When the interface grows a bias/options argument, capture it here via
    // `(this as { acceptBias?: (...) })` — until then any extra arg will be
    // ignored by TS and the observable field stays undefined, which is the
    // whole point of the RED baseline for flow #27.
    ..._rest: unknown[]
  ): Promise<DestinationSearchResult[]> {
    this.searchCalls.push({ query, limit });
    if (this.searchDelayMs > 0) {
      await new Promise((resolve) => setTimeout(resolve, this.searchDelayMs));
    }
    return this.nextResults;
  }

  async resolveDestination(
    coordinate: CoordinatePoint,
    fallbackTitle: string,
  ): Promise<DestinationSearchResult | null> {
    this.resolveCalls.push({ coordinate, fallbackTitle });
    return this.nextResolve;
  }
}
