import { makeAutoObservable } from "mobx";
import {
  type RoutingDiagEventData,
  type RoutingDiagSession,
  newEventId,
  newSessionId,
  sessionDebugPackage,
} from "../domain/routingDiagnosticsModels.js";
import type { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";

export class RoutingDiagnosticsStore {
  sessions: RoutingDiagSession[] = [];
  currentSession: RoutingDiagSession | null = null;

  constructor(private readonly persistence: LocalStoragePersistence) {
    this.sessions = persistence.loadRoutingDiagnosticsSessions();
    makeAutoObservable(this, {}, { autoBind: true });
  }

  get isRecording(): boolean {
    return this.currentSession !== null;
  }

  startRecording(): void {
    if (this.currentSession) return;
    this.currentSession = {
      id: newSessionId(),
      createdAtMs: Date.now(),
      updatedAtMs: Date.now(),
      events: [],
    };
  }

  stopRecording(): void {
    if (!this.currentSession) return;
    const session = { ...this.currentSession, updatedAtMs: Date.now() };
    this.currentSession = null;
    this.sessions = this.persistence.appendRoutingDiagnosticsSession(session);
  }

  recordEvent(data: RoutingDiagEventData): void {
    if (!this.currentSession) return;
    this.currentSession.events.push({
      id: newEventId(),
      timestampMs: Date.now(),
      data,
    });
    this.currentSession.updatedAtMs = Date.now();
  }

  deleteSession(id: string): void {
    this.sessions = this.persistence.removeRoutingDiagnosticsSession(id);
  }

  sessionDebugPackage(id: string): string | undefined {
    const session = this.sessions.find((s) => s.id === id);
    if (!session) return undefined;
    return sessionDebugPackage(session);
  }
}
