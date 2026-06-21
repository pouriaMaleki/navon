import { makeAutoObservable, runInAction } from "mobx";
import {
  type CoordinatePoint,
  type DestinationSearchResult,
  type PlaceSearchService,
  primaryProviderID,
  type RoutePlanRequest,
  type RoutePreviewModel,
  type RouteProviderID,
  type RouteSourceMode,
  type RoutingProvider,
  selectedAlternative,
} from "../domain/index.js";
import { isInFinland } from "../integrations/geo.js";
import {
  expandShortLink,
  extractCoordinateFromText,
  looksLikeUrl,
} from "../integrations/shareImport/UrlExpander.js";
import type { HistoryStore } from "./HistoryStore.js";
import type { LocationStore } from "./LocationStore.js";
import {
  decoratePreview,
  mergeMixedAlternatives,
  mixedNotice,
  presentAlternatives,
} from "./PlanningHelpers.js";
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
  isResolvingUrl = false;
  urlResolveError?: string;
  preview: RoutePreviewModel = { alternatives: [] };
  routeRequest: RoutePlanRequest = {
    origin: DEFAULT_ORIGIN,
    destination: { latitude: 60.1921, longitude: 24.9458 },
    providerID: "hsl",
  };
  currentSourceMode: RouteSourceMode = "mixed";
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
    // Reserved — settings may be re-read by future routing logic (e.g.
    // cyclingSpeedKph). Keep the parameter for stable constructor shape.
    _settings: SettingsStore,
    private readonly history?: HistoryStore,
  ) {
    const initialOrigin = this.location.bestKnownLocation();
    if (initialOrigin) this.routeRequest = { ...this.routeRequest, origin: initialOrigin };
    makeAutoObservable(this, {}, { autoBind: true });
  }

  private bestOrigin(): CoordinatePoint {
    return this.location.bestKnownLocation() ?? this.routeRequest.origin ?? DEFAULT_ORIGIN;
  }

  get isHslApplicableForRequest(): boolean {
    return isInFinland(this.routeRequest.origin) && isInFinland(this.routeRequest.destination);
  }

  get isHslAvailable(): boolean {
    return this.isHslApplicableForRequest;
  }

  get availableSourceModes(): RouteSourceMode[] {
    if (this.isHslAvailable) return ["mixed", "hsl", "osm"];
    return ["osm"];
  }

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
    this.markPickCompleted();
    void this.planRoute(suggestion.title);
  }

  /** Every destination-pick path goes through here — single source of truth. */
  markPickCompleted(label?: string): void {
    if (label !== undefined) this.query = label;
    this.isSearchOpen = false;
    this.postSelectionLatchUntilMs = Date.now() + this.postSelectionLatchMs;
  }

  setDestinationFromMap(coordinate: CoordinatePoint, fallbackTitle = "Dropped pin"): void {
    void this.resolveAndPlan(coordinate, fallbackTitle);
  }

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
    // Always run through `presentAlternatives` so every code path —
    // initial planning, GPX import, split-way reroute (Spec #11), and
    // any future "load preset" flow — produces the iOS-parity engine-
    // name titles ("BRouter fastbike" / "OSM Route" / "HSL Fastest")
    // with empty subtitles. Without this, `exploreAlternateRoutes`
    // surfaced the raw provider sourceReference as a second row of
    // details on each alternative card.
    this.preview = {
      ...preview,
      alternatives: presentAlternatives(preview.alternatives, this.currentSourceMode),
    };
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
    const effective = previews.filter((p) => p.alternatives.length > 0);
    const merged = mergeMixedAlternatives(effective.flatMap((p) => p.alternatives));
    return {
      alternatives: merged,
      selectedAlternativeID: merged[0]?.id,
      routeIdentifier: merged[0]?.normalizedPackage.routeIdentifier,
      routeRevision: merged[0]?.normalizedPackage.revision,
      planningNotice: mixedNotice(effective, racers.length, includeHsl),
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

export {
  decoratePreview,
  friendlyAlternativeLabel,
  mergeMixedAlternatives,
  mixedNotice,
  presentAlternatives,
} from "./PlanningHelpers.js";
