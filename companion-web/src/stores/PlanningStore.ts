import { makeAutoObservable, runInAction } from "mobx";
import {
  type CoordinatePoint,
  type DestinationSearchResult,
  primaryProviderID,
  ROUTE_PROVIDER_DISPLAY_NAME,
  type RouteAlternative,
  type RoutePlanRequest,
  type RoutePreviewModel,
  type RouteProviderID,
  type RouteSourceMode,
  selectedAlternative,
} from "../domain/models.js";
import type { PlaceSearchService, RoutingProvider } from "../domain/providers.js";
import { isInUusimaa } from "../integrations/geo.js";
import {
  expandShortLink,
  extractCoordinateFromText,
  looksLikeUrl,
} from "../integrations/shareImport/UrlExpander.js";
import type { HistoryStore } from "./HistoryStore.js";
import type { LocationStore } from "./LocationStore.js";
import type { SettingsStore } from "./SettingsStore.js";

const SWITCHABLE_PROVIDERS: ReadonlySet<RouteProviderID> = new Set(["hsl", "osm"]);

export type ProvidersMap = Record<RouteProviderID, RoutingProvider>;

const DEFAULT_ORIGIN: CoordinatePoint = { latitude: 60.1699, longitude: 24.9384 };

export class PlanningStore {
  query = "";
  isSearchOpen = false;
  suggestions: DestinationSearchResult[] = [];
  visibleSuggestionCount = 10;
  visibleRecentCount = 10;
  planningStatus?: string;
  importActivityStatus?: string;
  /** True while we are following a pasted URL (e.g. maps.app.goo.gl) to a destination. */
  isResolvingUrl = false;
  /** Last URL-resolve error (e.g. proxy unreachable, no coords found). */
  urlResolveError?: string;
  preview: RoutePreviewModel = { alternatives: [] };
  routeRequest: RoutePlanRequest = {
    origin: DEFAULT_ORIGIN,
    destination: { latitude: 60.1921, longitude: 24.9458 },
    providerID: "hsl",
  };
  currentSourceMode: RouteSourceMode = "mixed";
  /**
   * True from the moment a keystroke starts a typeahead search until the
   * adapter responds. Spec line 34 requires a loading indicator for search.
   */
  isTypeaheadSearching = false;
  /**
   * True while `loadMoreRecentsIfNeeded` has in-flight async work. Today
   * recents grow synchronously so this stays `false` until pagination goes
   * async; kept as an observable so the UI can subscribe consistently.
   */
  isLoadingMoreRecents = false;

  private currentSearchAbort?: AbortController;
  private currentPlanAbort?: AbortController;
  private currentUrlAbort?: AbortController;
  private typeaheadDebounceTimer?: ReturnType<typeof setTimeout>;
  private readonly typeaheadDebounceMs = 250;

  constructor(
    private readonly providers: ProvidersMap,
    private readonly placeSearch: PlaceSearchService,
    private readonly location: LocationStore,
    private readonly settings: SettingsStore,
    private readonly history?: HistoryStore,
  ) {
    const initialOrigin = this.location.bestKnownLocation();
    if (initialOrigin) this.routeRequest = { ...this.routeRequest, origin: initialOrigin };
    makeAutoObservable(this, {}, { autoBind: true });
  }

  /** Resolve the best origin to plan from: live > last-known > default. */
  private bestOrigin(): CoordinatePoint {
    return this.location.bestKnownLocation() ?? this.routeRequest.origin ?? DEFAULT_ORIGIN;
  }

  /** True only when both endpoints are inside the Uusimaa region (HSL coverage area). */
  get isHslApplicableForRequest(): boolean {
    return isInUusimaa(this.routeRequest.origin) && isInUusimaa(this.routeRequest.destination);
  }

  /** True when HSL is both configured AND geographically usable for the current request. */
  get isHslAvailable(): boolean {
    return this.settings.isHslLiveConfigured && this.isHslApplicableForRequest;
  }

  /**
   * Source-mode tabs visible in the UI. With no Digitransit key, or when either endpoint
   * is outside Uusimaa, the only working provider is OSM, so we collapse the picker to
   * a single OSM option (the UI hides it entirely when there is only one).
   */
  get availableSourceModes(): RouteSourceMode[] {
    if (this.isHslAvailable) return ["mixed", "hsl", "osm"];
    return ["osm"];
  }

  /** Resolve the effective source mode given current state: mixed/hsl → osm when HSL unavailable. */
  private effectiveSourceMode(mode: RouteSourceMode): RouteSourceMode {
    if (!this.isHslAvailable && mode !== "osm") return "osm";
    return mode;
  }

  setSourceMode(mode: RouteSourceMode): void {
    const effective = this.effectiveSourceMode(mode);
    this.currentSourceMode = effective;
    this.routeRequest = { ...this.routeRequest, providerID: primaryProviderID(effective) };
  }

  /**
   * Short window after `selectSuggestion` during which `openSearch()` is
   * absorbed. The user-reported bug is the dropdown re-opens visually after
   * a pick because the input still holds focus and React re-fires its
   * `onFocus` handler. This latch swallows that follow-up open while the
   * legitimate "user explicitly tapped the input again" intent fires after
   * the window closes.
   */
  private readonly postSelectionLatchMs = 350;
  private postSelectionLatchUntilMs = 0;

  openSearch(): void {
    if (Date.now() < this.postSelectionLatchUntilMs) return;
    this.isSearchOpen = true;
  }

  closeSearch(): void {
    this.isSearchOpen = false;
    this.visibleSuggestionCount = 10;
    this.visibleRecentCount = 10;
  }

  loadMoreSuggestionsIfNeeded(item: DestinationSearchResult): void {
    if (this.suggestions.length === 0) return;
    const last =
      this.suggestions[Math.min(this.visibleSuggestionCount, this.suggestions.length) - 1];
    if (last && last.id === item.id && this.visibleSuggestionCount < this.suggestions.length) {
      this.visibleSuggestionCount = Math.min(
        this.visibleSuggestionCount + 10,
        this.suggestions.length,
      );
    }
  }

  /**
   * Grow the visible recents slice, but only when the user has scrolled to
   * the last visible item. Spec lines 72-73: "only shows a few until user
   * scroll to the bottom of it then it loads more".
   */
  loadMoreRecentsIfNeeded(lastId: string): void {
    const items = this.history?.routeHistoryItems ?? [];
    if (items.length === 0) return;
    const lastVisibleId = items[Math.min(this.visibleRecentCount, items.length) - 1]?.id;
    if (lastVisibleId !== lastId) return;
    if (this.visibleRecentCount >= items.length) return;
    this.visibleRecentCount = Math.min(this.visibleRecentCount + 10, items.length);
  }

  async updateQuery(text: string): Promise<void> {
    this.query = text;
    const trimmed = text.trim();

    // Cancel any pending debounce + in-flight request on every keystroke.
    if (this.typeaheadDebounceTimer !== undefined) {
      clearTimeout(this.typeaheadDebounceTimer);
      this.typeaheadDebounceTimer = undefined;
    }
    this.currentSearchAbort?.abort();

    if (trimmed.length === 0) {
      this.suggestions = [];
      this.urlResolveError = undefined;
      this.isTypeaheadSearching = false;
      return;
    }
    if (looksLikeUrl(trimmed)) {
      this.suggestions = [];
      this.isTypeaheadSearching = false;
      void this.resolveUrlDestination(trimmed);
      return;
    }

    // Flip the loading flag synchronously so the UI can render a spinner
    // the moment the user types. Spec line 34.
    this.isTypeaheadSearching = true;
    this.typeaheadDebounceTimer = setTimeout(() => {
      this.typeaheadDebounceTimer = undefined;
      void this.runTypeaheadSearch(text);
    }, this.typeaheadDebounceMs);
  }

  private async runTypeaheadSearch(text: string): Promise<void> {
    const controller = new AbortController();
    this.currentSearchAbort = controller;
    const bias = this.location.bestKnownLocation() ?? undefined;
    try {
      const results = await this.placeSearch.searchDestinations(text, 30, bias, controller.signal);
      runInAction(() => {
        if (this.currentSearchAbort === controller) {
          this.suggestions = results;
          this.visibleSuggestionCount = 10;
          this.isTypeaheadSearching = false;
        }
      });
    } catch (err) {
      if ((err as Error)?.name === "AbortError") return;
      runInAction(() => {
        if (this.currentSearchAbort === controller) {
          this.isTypeaheadSearching = false;
        }
      });
    }
  }

  /** Follow a pasted URL (Google Maps short link, plain coords URL, etc.) to a destination. */
  async resolveUrlDestination(url: string): Promise<void> {
    this.currentUrlAbort?.abort();
    const controller = new AbortController();
    this.currentUrlAbort = controller;
    this.isResolvingUrl = true;
    this.urlResolveError = undefined;
    try {
      // Inline coords often live in the URL itself — try those before hitting the network.
      let coordinate = extractCoordinateFromText(url);
      let title: string | undefined;
      if (!coordinate) {
        const expansion = await expandShortLink(url, controller.signal);
        if (this.currentUrlAbort !== controller) return;
        coordinate = expansion.coordinate;
        title = expansion.title;
      }
      if (!coordinate) {
        runInAction(() => {
          if (this.currentUrlAbort === controller) {
            this.urlResolveError = "Couldn't find a destination in that URL.";
          }
        });
        return;
      }
      const resolved = await this.placeSearch.resolveDestination(
        coordinate,
        title ?? "Imported destination",
      );
      const finalTitle = resolved?.title ?? title ?? "Imported destination";
      runInAction(() => {
        if (this.currentUrlAbort !== controller) return;
        this.query = finalTitle;
        this.routeRequest = {
          origin: this.bestOrigin(),
          destination: coordinate as CoordinatePoint,
          providerID: primaryProviderID(this.currentSourceMode),
        };
        this.isSearchOpen = false;
      });
      await this.planRoute(finalTitle);
    } catch (err) {
      if ((err as Error)?.name === "AbortError") return;
      const message = err instanceof Error ? err.message : "Unknown error";
      runInAction(() => {
        if (this.currentUrlAbort === controller) {
          this.urlResolveError = `URL expansion failed: ${message}`;
        }
      });
    } finally {
      runInAction(() => {
        if (this.currentUrlAbort === controller) {
          this.isResolvingUrl = false;
          this.currentUrlAbort = undefined;
        }
      });
    }
  }

  selectSuggestion(suggestion: DestinationSearchResult): void {
    this.query = suggestion.title;
    this.routeRequest = {
      origin: this.bestOrigin(),
      destination: suggestion.coordinate,
      providerID: primaryProviderID(this.currentSourceMode),
    };
    this.isSearchOpen = false;
    this.postSelectionLatchUntilMs = Date.now() + this.postSelectionLatchMs;
    void this.planRoute(suggestion.title);
  }

  setDestinationFromMap(coordinate: CoordinatePoint, fallbackTitle = "Dropped pin"): void {
    void this.resolveAndPlan(coordinate, fallbackTitle);
  }

  /** Update the routeRequest origin from the freshest known location. */
  refreshOriginFromLocation(): void {
    const origin = this.location.bestKnownLocation();
    if (!origin) return;
    if (
      origin.latitude !== this.routeRequest.origin.latitude ||
      origin.longitude !== this.routeRequest.origin.longitude
    ) {
      this.routeRequest = { ...this.routeRequest, origin };
    }
  }

  selectAlternative(id: string): void {
    if (!this.preview.alternatives.find((a) => a.id === id)) return;
    const next = { ...this.preview, selectedAlternativeID: id };
    const selected = selectedAlternative(next);
    if (selected) {
      next.routeIdentifier = selected.normalizedPackage.routeIdentifier;
      next.routeRevision = selected.normalizedPackage.revision;
    }
    this.preview = next;
  }

  clearPreview(): void {
    this.query = "";
    this.suggestions = [];
    this.preview = { alternatives: [] };
    this.isSearchOpen = false;
    this.urlResolveError = undefined;
    this.currentUrlAbort?.abort();
    this.currentUrlAbort = undefined;
    this.isResolvingUrl = false;
  }

  setPreview(preview: RoutePreviewModel): void {
    this.preview = preview;
  }

  /** Plan a route through the current source mode. Mirrors AppModel.buildPreview/buildMixedPreview.
   *
   * `_preferredTitle` is preserved as a parameter (callers pass the
   * destination name they typed) so the call signature stays compatible
   * with route-history and suggestion-pick paths. The value itself is
   * unused now: alternatives are labelled "<Provider> Route N" by
   * `presentAlternatives`. The destination name is shown in the top
   * overlay's "Selected destination" line instead.
   */
  async planRoute(_preferredTitle?: string): Promise<void> {
    this.currentPlanAbort?.abort();
    const controller = new AbortController();
    this.currentPlanAbort = controller;
    this.planningStatus = "Planning route…";
    this.refreshOriginFromLocation();
    // Collapse HSL → OSM if a route is requested under HSL mode but no key is configured.
    const sourceMode = this.effectiveSourceMode(this.currentSourceMode);
    if (sourceMode !== this.currentSourceMode) this.currentSourceMode = sourceMode;
    try {
      const request = this.routeRequest;
      const preview =
        sourceMode === "mixed"
          ? await this.buildMixedPreview(request, controller.signal)
          : await this.buildSinglePreview(sourceMode, request, controller.signal);
      runInAction(() => {
        if (this.currentPlanAbort === controller) {
          // Note: a `preferredTitle` (typically the destination name) used
          // to override the first alternative's title here. The displayed
          // alternatives now use a numbered "<Provider> Route N" scheme,
          // so the override is dropped — the destination is shown in the
          // top "Selected destination" overlay instead, and route-history
          // items keep their own title at recordPlannedPreview time.
          this.preview = preview;
        }
      });
    } catch (err) {
      if ((err as Error)?.name === "AbortError") return;
      const message = err instanceof Error ? err.message : "Unknown error";
      runInAction(() => {
        this.preview = { alternatives: [], planningNotice: `Planning failed: ${message}` };
      });
    } finally {
      runInAction(() => {
        if (this.currentPlanAbort === controller) {
          this.planningStatus = undefined;
          this.currentPlanAbort = undefined;
        }
      });
    }
  }

  isPreviewLockedToImportedRoute(): boolean {
    const provider = selectedAlternative(this.preview)?.normalizedPackage.provenance.providerID;
    if (!provider) return false;
    return !SWITCHABLE_PROVIDERS.has(provider);
  }

  beginImportActivity(message: string): void {
    this.importActivityStatus = message;
  }

  endImportActivity(): void {
    this.importActivityStatus = undefined;
  }

  cancelInFlight(): void {
    this.currentSearchAbort?.abort();
    this.currentPlanAbort?.abort();
    this.currentUrlAbort?.abort();
    this.currentSearchAbort = undefined;
    this.currentPlanAbort = undefined;
    this.currentUrlAbort = undefined;
    this.isResolvingUrl = false;
  }

  private async buildSinglePreview(
    mode: RouteSourceMode,
    request: RoutePlanRequest,
    signal: AbortSignal,
  ): Promise<RoutePreviewModel> {
    const provider = this.providers[primaryProviderID(mode)];
    const preview = await provider.planRoute(request, signal);
    return decoratePreview(preview, mode);
  }

  private async buildMixedPreview(
    request: RoutePlanRequest,
    signal: AbortSignal,
  ): Promise<RoutePreviewModel> {
    const includeHsl = this.isHslAvailable;
    const racers: Promise<RoutePreviewModel>[] = [
      this.providers.osm.planRoute({ ...request, providerID: "osm" }, signal),
    ];
    if (includeHsl) {
      racers.push(this.providers.hsl.planRoute({ ...request, providerID: "hsl" }, signal));
    }
    const settled = await Promise.allSettled(racers);
    const previews: RoutePreviewModel[] = [];
    for (const r of settled) {
      if (r.status === "fulfilled") previews.push(r.value);
    }
    const live = previews.filter((p) => !isSamplePreview(p) && p.alternatives.length > 0);
    const effective = live.length > 0 ? live : previews.filter((p) => p.alternatives.length > 0);
    const merged = mergeMixedAlternatives(effective.flatMap((p) => p.alternatives));
    return {
      alternatives: merged,
      selectedAlternativeID: merged[0]?.id,
      routeIdentifier: merged[0]?.normalizedPackage.routeIdentifier,
      routeRevision: merged[0]?.normalizedPackage.revision,
      planningNotice: mixedNotice(previews, effective, includeHsl),
    };
  }

  private async resolveAndPlan(coordinate: CoordinatePoint, fallbackTitle: string): Promise<void> {
    this.routeRequest = {
      origin: this.bestOrigin(),
      destination: coordinate,
      providerID: primaryProviderID(this.currentSourceMode),
    };
    const resolved = await this.placeSearch.resolveDestination(coordinate, fallbackTitle);
    const title = resolved?.title ?? fallbackTitle;
    runInAction(() => {
      this.query = title;
    });
    await this.planRoute(title);
  }
}

function decoratePreview(preview: RoutePreviewModel, mode: RouteSourceMode): RoutePreviewModel {
  return {
    ...preview,
    alternatives: presentAlternatives(preview.alternatives, mode),
    selectedAlternativeID: preview.alternatives[0]?.id,
  };
}

function isSamplePreview(preview: RoutePreviewModel): boolean {
  return (preview.planningNotice ?? "").toLowerCase().includes("sample");
}

function mixedNotice(
  previews: RoutePreviewModel[],
  effective: RoutePreviewModel[],
  includeHsl: boolean,
): string {
  if (effective.length === 1 && effective[0].planningNotice) return effective[0].planningNotice;
  if (effective.length < previews.length) {
    return "Showing live routes while sample fallback providers are hidden.";
  }
  return includeHsl ? "Mixed routes from HSL and OSM" : "OSM bike routes";
}

export function mergeMixedAlternatives(alternatives: RouteAlternative[]): RouteAlternative[] {
  if (alternatives.length === 0) return [];
  const sorted = [...alternatives].sort((a, b) => {
    if (a.durationSeconds === b.durationSeconds) return a.distanceMeters - b.distanceMeters;
    return a.durationSeconds - b.durationSeconds;
  });
  const remaining = [...sorted];
  const chosen: RouteAlternative[] = [];
  const takeFirstMatching = (predicate: (a: RouteAlternative) => boolean): void => {
    const idx = remaining.findIndex(predicate);
    if (idx >= 0) {
      chosen.push(remaining[idx]);
      remaining.splice(idx, 1);
    } else if (remaining.length > 0) {
      chosen.push(remaining.shift() as RouteAlternative);
    }
  };
  if (remaining.length > 0) {
    chosen.push(remaining.shift() as RouteAlternative);
  }
  takeFirstMatching((a) => a.normalizedPackage.provenance.providerID === "osm");
  takeFirstMatching(() => true);
  while (chosen.length < 3 && remaining.length > 0) {
    chosen.push(remaining.shift() as RouteAlternative);
  }
  return presentAlternatives(chosen, "mixed");
}

/**
 * Label every visible alternative as "<Provider> Route N", where N is a
 * per-provider counter (so OSM Route 1, OSM Route 2, HSL Route 1, …).
 * This replaces the prior "Fastest / Quieter / Simpler" scheme which
 * implied semantics the routing backends don't actually deliver — the
 * order is just whatever the provider returned.
 */
export function presentAlternatives(
  alternatives: RouteAlternative[],
  _mode: RouteSourceMode,
): RouteAlternative[] {
  const counters = new Map<string, number>();
  return alternatives.slice(0, 3).map((alt) => {
    const providerID = alt.normalizedPackage.provenance.providerID;
    const providerLabel = ROUTE_PROVIDER_DISPLAY_NAME[providerID];
    const next = (counters.get(providerID) ?? 0) + 1;
    counters.set(providerID, next);
    return {
      ...alt,
      title: `${providerLabel} Route ${next}`,
      subtitle: alt.normalizedPackage.provenance.sourceReference ?? `via ${providerLabel}`,
    };
  });
}
