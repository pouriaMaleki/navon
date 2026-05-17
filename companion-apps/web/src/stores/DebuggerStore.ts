import { makeAutoObservable } from "mobx";
import {
  type Annotation,
  type AnnotationExport,
  type AnnotationSeverity,
  type AnnotationTag,
  type CoordinatePoint,
  type DebuggerSession,
  interpolateGps,
  sessionEndTime,
  sessionStartTime,
} from "../domain/debuggerModels.js";
import type {
  RoutingDiagDebugPackage,
  RoutingDiagEvent,
  RoutingDiagSession,
} from "../domain/routingDiagnosticsModels.js";
import type { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";

export type PlaybackState = "playing" | "paused" | "stopped";

export class DebuggerStore {
  session: DebuggerSession | null = null;
  playbackState: PlaybackState = "stopped";
  playbackSpeed: number = 1;
  currentTimeMs: number = 0;
  selectedEventId: string | null = null;
  selectedAnnotationId: string | null = null;
  pendingAnnotationTimeMs: number | null = null;
  pendingAnnotationCoordinate: CoordinatePoint | null = null;
  pendingAnnotationEventIds: string[] = [];
  mapFollowActive = false;
  private animFrameId: number | null = null;
  private lastTickRealMs: number = 0;

  constructor(private readonly persistence: LocalStoragePersistence) {
    makeAutoObservable(this, {}, { autoBind: true });
  }

  get sessionDurationMs(): number {
    if (!this.session) return 0;
    return sessionEndTime(this.session.diagSession) - sessionStartTime(this.session.diagSession);
  }

  get currentElapsedMs(): number {
    return this.currentTimeMs - (this.session ? sessionStartTime(this.session.diagSession) : 0);
  }

  get currentPosition(): CoordinatePoint | null {
    if (!this.session) return null;
    return interpolateGps(this.session.diagSession.events, this.currentElapsedMs);
  }

  get visibleEvents(): RoutingDiagEvent[] {
    if (!this.session) return [];
    return this.session.diagSession.events.filter((e) => e.timestampMs <= this.currentTimeMs);
  }

  get visibleCueEvents(): RoutingDiagEvent[] {
    return this.visibleEvents.filter((e) => e.data.kind === "audioCueDispatched");
  }

  get currentTurnAlert(): { instructionText: string; distanceRemainingM: number } | null {
    if (!this.session) return null;
    const turns = this.session.diagSession.events.filter(
      (e) => e.data.kind === "nextTurnAlerted" && e.timestampMs <= this.currentTimeMs,
    );
    const last = turns[turns.length - 1];
    if (!last || last.data.kind !== "nextTurnAlerted") return null;
    return {
      instructionText: last.data.instructionText,
      distanceRemainingM: last.data.distanceRemainingM,
    };
  }

  get annotations(): Annotation[] {
    return this.session?.annotations ?? [];
  }

  loadSessionFromPackage(pkg: RoutingDiagDebugPackage): void {
    this.stopPlayback();
    const diagSession: RoutingDiagSession = {
      id: pkg.sessionId,
      createdAtMs: pkg.createdAtMs,
      updatedAtMs:
        pkg.events.length > 0 ? pkg.events[pkg.events.length - 1].timestampMs : pkg.createdAtMs,
      events: pkg.events,
    };
    const savedAnnotations = this.persistence.loadDebuggerAnnotations(diagSession.id);
    this.session = {
      diagSession,
      annotations: savedAnnotations,
    };
    this.currentTimeMs = sessionStartTime(diagSession);
    this.selectedEventId = null;
    this.selectedAnnotationId = null;
  }

  loadSessionFromExisting(diagSession: RoutingDiagSession): void {
    this.stopPlayback();
    const savedAnnotations = this.persistence.loadDebuggerAnnotations(diagSession.id);
    this.session = {
      diagSession,
      annotations: savedAnnotations,
    };
    this.currentTimeMs = sessionStartTime(diagSession);
    this.selectedEventId = null;
    this.selectedAnnotationId = null;
  }

  loadGpxGeometry(geometry: CoordinatePoint[]): void {
    if (!this.session) return;
    this.session = { ...this.session, gpxGeometry: geometry };
  }

  play(): void {
    if (!this.session || this.playbackState === "playing") return;
    if (this.currentTimeMs >= sessionEndTime(this.session.diagSession)) {
      this.currentTimeMs = sessionStartTime(this.session.diagSession);
    }
    this.playbackState = "playing";
    this.lastTickRealMs = performance.now();
    this.scheduleTick();
  }

  pause(): void {
    this.playbackState = "paused";
    if (this.animFrameId !== null) {
      cancelAnimationFrame(this.animFrameId);
      this.animFrameId = null;
    }
  }

  stopPlayback(): void {
    this.pause();
    this.playbackState = "stopped";
    if (this.session) {
      this.currentTimeMs = sessionStartTime(this.session.diagSession);
    }
  }

  seekToElapsed(elapsedMs: number): void {
    if (!this.session) return;
    this.currentTimeMs = sessionStartTime(this.session.diagSession) + Math.max(0, elapsedMs);
    if (this.playbackState === "playing") {
      this.lastTickRealMs = performance.now();
    }
  }

  seekToEvent(eventId: string): void {
    if (!this.session) return;
    const event = this.session.diagSession.events.find((e) => e.id === eventId);
    if (event) {
      this.currentTimeMs = event.timestampMs;
      this.selectedEventId = eventId;
      if (this.playbackState === "playing") {
        this.lastTickRealMs = performance.now();
      }
    }
  }

  stepForward(): void {
    if (!this.session) return;
    const events = this.session.diagSession.events;
    const next = events.find((e) => e.timestampMs > this.currentTimeMs);
    if (next) {
      this.currentTimeMs = next.timestampMs;
      this.selectedEventId = next.id;
    }
  }

  stepBackward(): void {
    if (!this.session) return;
    const events = this.session.diagSession.events;
    let prev: RoutingDiagEvent | null = null;
    for (const e of events) {
      if (e.timestampMs >= this.currentTimeMs) break;
      prev = e;
    }
    if (prev) {
      this.currentTimeMs = prev.timestampMs;
      this.selectedEventId = prev.id;
    }
  }

  setPlaybackSpeed(speed: number): void {
    this.playbackSpeed = speed;
    if (this.playbackState === "playing") {
      this.lastTickRealMs = performance.now();
    }
  }

  selectEvent(eventId: string | null): void {
    this.selectedEventId = eventId;
    if (eventId) {
      this.seekToEvent(eventId);
    }
  }

  selectAnnotation(annotationId: string | null): void {
    this.selectedAnnotationId = annotationId;
    if (annotationId) {
      const ann = this.annotations.find((a) => a.id === annotationId);
      if (ann) {
        this.currentTimeMs =
          ann.timeRangeMs[0] + (this.session ? sessionStartTime(this.session.diagSession) : 0);
      }
    }
  }

  openAnnotationForm(timeMs: number, coordinate?: CoordinatePoint, eventIds?: string[]): void {
    this.pendingAnnotationTimeMs = timeMs;
    this.pendingAnnotationCoordinate = coordinate ?? null;
    this.pendingAnnotationEventIds = eventIds ?? [];
  }

  cancelAnnotationForm(): void {
    this.pendingAnnotationTimeMs = null;
    this.pendingAnnotationCoordinate = null;
    this.pendingAnnotationEventIds = [];
  }

  addAnnotation(
    tag: AnnotationTag,
    severity: AnnotationSeverity,
    timeRangeMs: [number, number],
    note: string,
  ): void {
    if (!this.session) return;
    const linkedEventIds = [...this.pendingAnnotationEventIds];
    const sessionStart = sessionStartTime(this.session.diagSession);
    const eventsInRange = this.session.diagSession.events.filter((e) => {
      const rel = e.timestampMs - sessionStart;
      return rel >= timeRangeMs[0] && rel <= timeRangeMs[1];
    });
    for (const e of eventsInRange) {
      if (!linkedEventIds.includes(e.id)) {
        linkedEventIds.push(e.id);
      }
    }
    const annotation: Annotation = {
      id: `ann-${Date.now()}`,
      tag,
      severity,
      timeRangeMs,
      linkedEventIds,
      coordinate: this.pendingAnnotationCoordinate ?? undefined,
      note,
      createdAtMs: Date.now(),
    };
    this.session = {
      ...this.session,
      annotations: [...this.session.annotations, annotation],
    };
    this.persistAnnotations();
    this.cancelAnnotationForm();
    this.selectedAnnotationId = annotation.id;
  }

  deleteAnnotation(id: string): void {
    if (!this.session) return;
    this.session = {
      ...this.session,
      annotations: this.session.annotations.filter((a) => a.id !== id),
    };
    if (this.selectedAnnotationId === id) {
      this.selectedAnnotationId = null;
    }
    this.persistAnnotations();
  }

  exportAnnotations(): AnnotationExport | null {
    if (!this.session) return null;
    const CONTEXT_WINDOW_MS = 10000;
    const sessionStart = sessionStartTime(this.session.diagSession);
    const eventContext: AnnotationExport["eventContext"] = {};
    for (const ann of this.session.annotations) {
      const absStart = sessionStart + ann.timeRangeMs[0];
      const absEnd = sessionStart + ann.timeRangeMs[1];
      eventContext[ann.id] = {
        before: this.session.diagSession.events.filter(
          (e) => e.timestampMs >= absStart - CONTEXT_WINDOW_MS && e.timestampMs < absStart,
        ),
        during: this.session.diagSession.events.filter(
          (e) => e.timestampMs >= absStart && e.timestampMs <= absEnd,
        ),
        after: this.session.diagSession.events.filter(
          (e) => e.timestampMs > absEnd && e.timestampMs <= absEnd + CONTEXT_WINDOW_MS,
        ),
      };
    }
    return {
      formatVersion: 1,
      sessionId: this.session.diagSession.id,
      exportedAtMs: Date.now(),
      annotations: this.session.annotations,
      eventContext,
    };
  }

  setMapFollowActive(active: boolean): void {
    this.mapFollowActive = active;
  }

  private scheduleTick(): void {
    if (this.playbackState !== "playing") return;
    this.animFrameId = requestAnimationFrame((_now) => {
      const now = performance.now();
      const realDeltaMs = now - this.lastTickRealMs;
      this.lastTickRealMs = now;
      const simDeltaMs = realDeltaMs * this.playbackSpeed;
      if (this.session) {
        const newTime = this.currentTimeMs + simDeltaMs;
        const end = sessionEndTime(this.session.diagSession);
        if (newTime >= end) {
          this.currentTimeMs = end;
          this.pause();
          return;
        }
        this.currentTimeMs = newTime;
      }
      this.scheduleTick();
    });
  }

  private persistAnnotations(): void {
    if (!this.session) return;
    this.persistence.saveDebuggerAnnotations(this.session.diagSession.id, this.session.annotations);
  }
}
