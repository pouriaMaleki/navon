import { makeAutoObservable } from "mobx";
import type {
  CoordinatePoint,
  ImportDiagnosticsEntry,
  PendingHomePresentation,
  RouteHistoryItem,
} from "../domain/models.js";
import type { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";

export class HistoryStore {
  routeHistoryItems: RouteHistoryItem[] = [];
  recentDestinations: CoordinatePoint[] = [];
  importDiagnosticsEntries: ImportDiagnosticsEntry[] = [];
  pendingHomePresentation?: PendingHomePresentation;
  /** Increments any time we want HomeView to re-evaluate the pending presentation. */
  homePreviewRequestTick = 0;
  /** Increments when HomeView should auto-start the selected route. */
  homeStartRequestTick = 0;

  constructor(private readonly persistence: LocalStoragePersistence) {
    this.routeHistoryItems = persistence.loadRouteHistory();
    this.recentDestinations = persistence.loadRecentDestinations();
    this.importDiagnosticsEntries = persistence.loadImportDiagnostics();
    this.pendingHomePresentation = persistence.loadPendingHomePresentation();
    makeAutoObservable(this, {}, { autoBind: true });
  }

  appendRouteHistoryItem(item: RouteHistoryItem): RouteHistoryItem {
    this.routeHistoryItems = this.persistence.appendRouteHistoryItem(item);
    return this.routeHistoryItems[0] ?? item;
  }

  removeRouteHistoryItem(id: string): void {
    this.routeHistoryItems = this.persistence.removeRouteHistoryItem(id);
  }

  recordRecentDestination(point: CoordinatePoint): void {
    this.recentDestinations = this.persistence.saveRecentDestination(point);
  }

  setPendingHomePresentation(value: PendingHomePresentation | undefined): void {
    this.pendingHomePresentation = value;
    this.persistence.savePendingHomePresentation(value);
  }

  requestHomePreviewReveal(): void {
    this.homePreviewRequestTick += 1;
  }

  requestHomeStart(): void {
    this.homeStartRequestTick += 1;
  }

  appendImportDiagnostics(entry: ImportDiagnosticsEntry): void {
    this.importDiagnosticsEntries = this.persistence.appendImportDiagnostics(entry);
  }

  removeImportDiagnostics(id: string): void {
    this.importDiagnosticsEntries = this.persistence.removeImportDiagnostics(id);
  }
}
