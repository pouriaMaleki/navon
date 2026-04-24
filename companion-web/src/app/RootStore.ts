import { autorun, makeAutoObservable, reaction, runInAction } from "mobx";
import {
  type CoordinatePoint,
  type ImportClassification,
  primaryProviderID,
  type RouteHistoryItem,
  type RouteHistorySource,
} from "../domain/models.js";
import type { PlaceSearchService } from "../domain/providers.js";
import { GpxRoutingAdapter } from "../integrations/gpx/GpxRoutingAdapter.js";
import { HslRoutingAdapter } from "../integrations/hsl/HslRoutingAdapter.js";
import { BrowserLocationService } from "../integrations/location/BrowserLocationService.js";
import { OsrmRoutingAdapter } from "../integrations/osm/OsrmRoutingAdapter.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { SampleRoutingAdapter } from "../integrations/sample/SampleRoutingAdapter.js";
import { PhotonNominatimSearchService } from "../integrations/search/PhotonNominatimSearchService.js";
import {
  type ClassifiedImport,
  classifyImport,
  type ImportInput,
} from "../integrations/shareImport/UrlImportClassifier.js";
import { DiagnosticsStore } from "../stores/DiagnosticsStore.js";
import { GuidanceStore } from "../stores/GuidanceStore.js";
import { HistoryStore } from "../stores/HistoryStore.js";
import { LocationStore } from "../stores/LocationStore.js";
import { MapCameraStore } from "../stores/MapCameraStore.js";
import { PlanningStore, type ProvidersMap } from "../stores/PlanningStore.js";
import { SettingsStore } from "../stores/SettingsStore.js";

export type AppRoute = "home" | "settings";

export class RootStore {
  readonly persistence = new LocalStoragePersistence();
  readonly placeSearch: PlaceSearchService = new PhotonNominatimSearchService();
  readonly settingsStore = new SettingsStore(this.persistence);
  readonly historyStore = new HistoryStore(this.persistence);
  readonly mapCameraStore = new MapCameraStore();
  readonly diagnosticsStore = new DiagnosticsStore();
  readonly locationStore = new LocationStore(new BrowserLocationService(), this.persistence);

  private readonly gpxAdapter = new GpxRoutingAdapter();
  private readonly providers: ProvidersMap;
  readonly planningStore: PlanningStore;
  readonly guidanceStore: GuidanceStore;

  /** Current top-level route. The web "Settings" is shown as a full-screen overlay. */
  route: AppRoute = "home";
  /** Whether the user has dismissed or actioned the one-time location-permission banner. */
  locationBannerDismissed = false;
  private rerouteAbort?: AbortController;

  constructor() {
    const hsl = new HslRoutingAdapter(() => this.settingsStore.snapshotForAdapter());
    const osm = new OsrmRoutingAdapter();
    const fitSample = new SampleRoutingAdapter("fitImport");
    const tcxSample = new SampleRoutingAdapter("tcxImport");
    this.providers = {
      hsl,
      osm,
      gpxImport: this.gpxAdapter,
      fitImport: fitSample,
      tcxImport: tcxSample,
    } as ProvidersMap;
    this.planningStore = new PlanningStore(
      this.providers,
      this.placeSearch,
      this.locationStore,
      this.settingsStore,
      this.historyStore,
    );
    this.planningStore.setSourceMode(this.settingsStore.plannerPreferences.defaultSourceMode);
    this.guidanceStore = new GuidanceStore(
      this.planningStore,
      this.persistence,
      this.locationStore,
    );
    makeAutoObservable(
      this,
      {
        persistence: false,
        placeSearch: false,
      },
      { autoBind: true },
    );
    autorun(() => this.diagnosticsStore.updateFromSession(this.guidanceStore.activeSession));
    // Whenever HSL stops being usable (no key OR endpoints outside Uusimaa), switch the
    // active source mode to OSM. Mixed/HSL collapse to a single OSM tab in those cases.
    autorun(() => {
      if (!this.planningStore.isHslAvailable && this.planningStore.currentSourceMode !== "osm") {
        this.planningStore.setSourceMode("osm");
      }
    });
    // Advance route progress on every GPS fix during guidance.
    reaction(
      () => this.locationStore.currentLocation,
      (location) => {
        if (!location) return;
        if (this.guidanceStore.homeMode !== "phoneGuidance") return;
        this.guidanceStore.advanceProgress(location, Date.now());
      },
    );
    // Trigger rerouting when sustained off-route is detected.
    reaction(
      () => this.guidanceStore.rerouteRequested,
      (requested) => {
        if (requested) void this.performReroute();
      },
    );
    // Follow-rider intent: center on rider, zoom into routing view, rotate
    // to the current route-segment bearing. Fires on startSelectedRoute, on
    // every GPS tick during guidance, after map-interaction inactivity
    // timeout, and when compass returns to autoFollow from northLocked.
    const ROUTING_FOLLOW_ZOOM = 16;
    this.guidanceStore.onRecenterRequested(() => {
      const rider =
        this.locationStore.currentLocation ??
        this.locationStore.lastKnownLocation ??
        this.planningStore.routeRequest.origin;
      if (!rider) return;
      const inRouting = this.guidanceStore.homeMode === "phoneGuidance";
      // Routing uses a fixed "navigation zoom" (16) so Start always snaps
      // the camera in — regardless of what planning-mode overview zoom was.
      // Outside routing, preserve whatever the user was looking at.
      const zoom = inRouting
        ? ROUTING_FOLLOW_ZOOM
        : this.mapCameraStore.target.kind === "center"
          ? this.mapCameraStore.target.zoom
          : 14;
      // Spec line 110 (MOST IMPORTANT): when the rider is moving, the
      // camera rotates to the GPS-derived travel heading — overrides the
      // route-segment bearing. Spec line 101 is the fallback when the
      // rider is stationary ("even when stationary yet").
      const trailHeading = this.locationStore.travelHeadingDegrees;
      const routeBearing = inRouting ? (this.guidanceStore.routingBearingDegrees ?? 0) : 0;
      const bearing = inRouting ? (trailHeading ?? routeBearing) : (trailHeading ?? 0);
      this.mapCameraStore.setCenter(rider, zoom, bearing);
    });
    // Route-overview intent: fit the active route geometry, north-up. Fires
    // on compass single-tap (northPreview) and double-tap (northLocked).
    this.guidanceStore.onFitRouteRequested(() => {
      const selected =
        this.planningStore.preview.alternatives.find(
          (a) => a.id === this.planningStore.preview.selectedAlternativeID,
        ) ?? this.planningStore.preview.alternatives[0];
      const geometry = selected?.normalizedPackage.geometry;
      if (!geometry || geometry.length === 0) return;
      this.mapCameraStore.fitBounds(geometry, 120);
    });
    // Spec lines 40, 84, 108-118: the bottom-quarter rider anchor applies
    // when the rider is moving (with or without a route) — not just during
    // routing. The "rider is moving" signal is the heading-trail's
    // `travelHeadingDegrees` (defined ↔ ≥ 3 m displacement within the
    // window). Stationary planning falls back to the centered 0.5 anchor.
    autorun(() => {
      const inRouting = this.guidanceStore.homeMode === "phoneGuidance";
      const moving = this.locationStore.travelHeadingDegrees !== undefined;
      const anchor = inRouting || moving ? 0.72 : 0.5;
      this.mapCameraStore.setRiderAnchorNormalizedY(anchor);
    });
    // Auto-start the watcher only if the user has previously granted permission.
    // For the first-time prompt-or-denied case we let the banner ask explicitly.
    void this.maybeAutoStartLocation();
  }

  async maybeAutoStartLocation(): Promise<void> {
    if (this.locationStore.permission === "granted" || this.locationStore.promptShown) {
      this.locationStore.start();
    }
  }

  requestLocationFromBanner(): void {
    this.locationBannerDismissed = true;
    this.locationStore.start();
  }

  dismissLocationBanner(): void {
    this.locationBannerDismissed = true;
    this.persistence.saveLocationPromptShown(true);
  }

  /** Reroute from the rider's current location to the active destination. */
  private async performReroute(): Promise<void> {
    const session = this.guidanceStore.activeSession;
    if (!session.destinationCoordinate || !session.routeIdentifier) return;
    const riderLocation = this.locationStore.currentLocation;
    if (!riderLocation) return;

    this.rerouteAbort?.abort();
    const controller = new AbortController();
    this.rerouteAbort = controller;

    try {
      const provider = this.providers[session.providerID];
      const preview = await provider.replanRoute(session, riderLocation, controller.signal);
      if (controller.signal.aborted) return;
      const selected = preview.alternatives[0];
      if (!selected) return;
      runInAction(() => {
        this.planningStore.setPreview({
          ...preview,
          selectedAlternativeID: selected.id,
          routeIdentifier: selected.normalizedPackage.routeIdentifier,
          routeRevision: selected.normalizedPackage.revision,
        });
        this.guidanceStore.activeSession = {
          ...session,
          routeIdentifier: selected.normalizedPackage.routeIdentifier,
          routeRevision: selected.normalizedPackage.revision,
          lastRerouteReason: "off-route",
          lastRerouteTimestampMs: Date.now(),
        };
        this.guidanceStore.resetProgress(selected.normalizedPackage);
        this.diagnosticsStore.updateFromSession(this.guidanceStore.activeSession);
      });
    } catch (err) {
      if ((err as Error)?.name === "AbortError") return;
      // Reroute failed — rider stays on current route with off-route state visible
    } finally {
      if (this.rerouteAbort === controller) this.rerouteAbort = undefined;
    }
  }

  dispose(): void {
    this.rerouteAbort?.abort();
    this.locationStore.stop();
  }

  goHome(): void {
    this.route = "home";
  }

  goSettings(): void {
    this.route = "settings";
  }

  /** Import a dropped/picked GPX file. Routes through the planning preview directly. */
  async importGpxFile(fileName: string, content: string): Promise<void> {
    this.planningStore.beginImportActivity(`Importing ${fileName}…`);
    try {
      const preview = this.gpxAdapter.importFile(fileName, content);
      runInAction(() => {
        this.planningStore.setSourceMode("hsl");
        this.planningStore.setPreview(preview);
      });
      const item = this.recordPlannedPreview(`Imported ${fileName}`, "gpxImport", "GPX");
      this.historyStore.setPendingHomePresentation({
        routeHistoryItemID: item.id,
        title: item.title,
        sourceLabel: item.sourceLabel,
        destination: item.destination,
        createdAtMs: Date.now(),
        debugTrail: ["gpx-import", `name=${fileName}`],
      });
      this.historyStore.requestHomePreviewReveal();
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unknown error";
      this.planningStore.setPreview({
        alternatives: [],
        planningNotice: `GPX import failed: ${message}`,
      });
    } finally {
      this.planningStore.endImportActivity();
    }
  }

  /** Classify and consume any drop/share input. */
  async ingestSharedImport(input: ImportInput): Promise<void> {
    const classified = classifyImport(input);
    if (classified.classification === "gpxFile" && classified.fileContent && classified.fileName) {
      await this.importGpxFile(classified.fileName, classified.fileContent);
      return;
    }
    if (
      (classified.classification === "googleMapsLocationLink" ||
        classified.classification === "coordinatesText") &&
      classified.coordinate
    ) {
      this.planningStore.setDestinationFromMap(classified.coordinate, "Imported destination");
      return;
    }
    this.recordImportDiagnostics(classified, input);
  }

  /** Record the currently selected preview as a route-history entry. */
  recordPlannedPreview(
    title: string,
    source: RouteHistorySource,
    sourceLabel: string,
  ): RouteHistoryItem {
    const selected = this.planningStore.preview.alternatives[0];
    const package_ = selected?.normalizedPackage;
    const summaryLine = package_
      ? `${Math.round(package_.summary.totalDistanceMeters)} m • ${Math.max(
          Math.floor(package_.summary.estimatedDurationSeconds / 60),
          1,
        )} min`
      : "";
    const item: RouteHistoryItem = {
      id: `${source}-${Date.now()}`,
      title,
      subtitle: summaryLine,
      source,
      sourceLabel,
      createdAtMs: Date.now(),
      destination: package_?.geometry[package_.geometry.length - 1],
      routePackage: package_,
    };
    return this.historyStore.appendRouteHistoryItem(item);
  }

  /** Record a destination the user just typed/selected so it shows up in recents. */
  recordRecentDestination(point: CoordinatePoint, title: string): void {
    this.historyStore.recordRecentDestination(point);
    const item: RouteHistoryItem = {
      id: `recent-${Date.now()}`,
      title,
      subtitle: `${point.latitude.toFixed(4)}, ${point.longitude.toFixed(4)}`,
      source: "recentDestination",
      sourceLabel: "Recent",
      createdAtMs: Date.now(),
      destination: point,
    };
    this.historyStore.appendRouteHistoryItem(item);
  }

  /** Apply a history item back into the planner. Optionally start guidance. */
  async activateRouteHistoryItem(item: RouteHistoryItem, startImmediately: boolean): Promise<void> {
    if (item.routePackage) {
      const package_ = item.routePackage;
      const provider = package_.provenance.providerID;
      const sourceMode = provider === "osm" ? "osm" : provider === "hsl" ? "hsl" : "mixed";
      runInAction(() => {
        this.planningStore.setSourceMode(sourceMode);
        this.planningStore.setPreview({
          alternatives: [
            {
              id: `history-${item.id}`,
              title: item.title,
              subtitle: item.subtitle,
              distanceMeters: Math.round(package_.summary.totalDistanceMeters),
              durationSeconds: package_.summary.estimatedDurationSeconds,
              normalizedPackage: package_,
            },
          ],
          selectedAlternativeID: `history-${item.id}`,
          routeIdentifier: package_.routeIdentifier,
          routeRevision: package_.revision,
          planningNotice: item.sourceLabel,
        });
      });
    } else if (item.destination) {
      runInAction(() => {
        this.planningStore.routeRequest = {
          ...this.planningStore.routeRequest,
          destination: item.destination as CoordinatePoint,
          providerID: primaryProviderID(this.planningStore.currentSourceMode),
        };
      });
      await this.planningStore.planRoute(item.title);
    }
    this.goHome();
    if (startImmediately) this.guidanceStore.startSelectedRoute();
  }

  consumePendingPresentationOnReveal(): void {
    const pending = this.historyStore.pendingHomePresentation;
    if (!pending) return;
    this.historyStore.setPendingHomePresentation(undefined);
    if (this.settingsStore.plannerPreferences.startBehavior === "automatic") {
      this.guidanceStore.startSelectedRoute();
    }
  }

  private recordImportDiagnostics(classified: ClassifiedImport, input: ImportInput): void {
    const id = `diag-${Date.now()}`;
    const envelope = {
      id,
      receivedAtMs: Date.now(),
      rawKind:
        input.kind === "url"
          ? ("url" as const)
          : input.kind === "text"
            ? ("plainText" as const)
            : ("file" as const),
      fileName: input.kind === "file" ? input.fileName : undefined,
      originalText: input.kind === "text" ? input.text : undefined,
      originalURL: input.kind === "url" ? input.url : undefined,
      classification: classified.classification as ImportClassification,
      disposition: "diagnosticsOnly" as const,
      note: classified.note,
      debugTrail: classified.debugTrail,
    };
    this.historyStore.appendImportDiagnostics({ id, envelope, createdAtMs: Date.now() });
  }
}
