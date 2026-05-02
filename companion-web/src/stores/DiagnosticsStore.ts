import { makeAutoObservable } from "mobx";
import {
  type ActiveRouteSession,
  type CompanionDiagnostics,
  ROUTE_PROVIDER_DISPLAY_NAME,
} from "../domain/models.js";

const EMPTY: CompanionDiagnostics = {
  providerName: "—",
  routeIdentifier: "—",
  routeRevision: 0,
  lastSyncResult: "Phone guidance only (no device sync on web)",
  lastRerouteOutcome: "—",
};

export class DiagnosticsStore {
  snapshot: CompanionDiagnostics = EMPTY;

  constructor() {
    makeAutoObservable(this, {}, { autoBind: true });
  }

  updateFromSession(session: ActiveRouteSession | undefined): void {
    if (!session?.routeIdentifier) {
      this.snapshot = EMPTY;
      return;
    }
    this.snapshot = {
      providerName: ROUTE_PROVIDER_DISPLAY_NAME[session.providerID],
      routeIdentifier: session.routeIdentifier,
      routeRevision: session.routeRevision ?? 0,
      lastSyncResult: "Phone guidance only (no device sync on web)",
      lastRerouteOutcome: session.lastRerouteReason ?? "—",
    };
  }
}
