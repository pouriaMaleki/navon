import {
  type ActiveRouteSession,
  type Annotation,
  type CompanionSettings,
  type CoordinatePoint,
  DEFAULT_COMPANION_SETTINGS,
  DEFAULT_PLANNER_PREFERENCES,
  EMPTY_ACTIVE_SESSION,
  type ImportDiagnosticsEntry,
  type PendingHomePresentation,
  type RouteHistoryItem,
  type RoutePlannerPreferences,
  ROUTING_DIAGNOSTICS_SESSION_LIMIT,
  type RoutingDiagSession,
} from "../../domain/index.js";
import { approximateDistanceMeters } from "../geo.js";

const KEY_RECENT = "companion.recentDestinations";
const KEY_HISTORY = "companion.routeHistory";
const KEY_DIAGNOSTICS = "companion.importDiagnostics";
const KEY_PENDING = "companion.pendingHomeImportPresentation";
const KEY_SESSION = "companion.lastSession";
const KEY_SETTINGS = "companion.settings";
const KEY_PLANNER = "companion.routePlannerPreferences";
const KEY_LAST_KNOWN_RIDER = "companion.lastKnownRider";
const KEY_LOCATION_PROMPT_SHOWN = "companion.locationPromptShown";
const KEY_ROUTING_DIAGNOSTICS = "companion.routingDiagnostics";

const RECENT_LIMIT = 30;
const HISTORY_LIMIT = 50;
const DIAGNOSTICS_LIMIT = 50;
const RECENT_DEDUP_METERS = 80;

export class LocalStoragePersistence {
  loadSettings(): CompanionSettings {
    // Spread the defaults so newly added fields (e.g. `ridingZoom` added when
    // zoom +/- buttons landed) default cleanly for users with older
    // localStorage payloads, instead of arriving as `undefined` and
    // surprising callers that expect the typed shape.
    return { ...DEFAULT_COMPANION_SETTINGS, ...readJson(KEY_SETTINGS, DEFAULT_COMPANION_SETTINGS) };
  }
  saveSettings(value: CompanionSettings): void {
    writeJson(KEY_SETTINGS, value);
  }

  loadPlannerPreferences(): RoutePlannerPreferences {
    return readJson(KEY_PLANNER, DEFAULT_PLANNER_PREFERENCES);
  }
  savePlannerPreferences(value: RoutePlannerPreferences): void {
    writeJson(KEY_PLANNER, value);
  }

  loadLastSession(): ActiveRouteSession {
    return readJson(KEY_SESSION, EMPTY_ACTIVE_SESSION);
  }
  saveLastSession(value: ActiveRouteSession): void {
    writeJson(KEY_SESSION, value);
  }

  loadRecentDestinations(): CoordinatePoint[] {
    return readJson<CoordinatePoint[]>(KEY_RECENT, []);
  }
  saveRecentDestination(point: CoordinatePoint): CoordinatePoint[] {
    const merged = mergeRecentDestinations(this.loadRecentDestinations(), point);
    writeJson(KEY_RECENT, merged);
    return merged;
  }

  loadRouteHistory(): RouteHistoryItem[] {
    return readJson<RouteHistoryItem[]>(KEY_HISTORY, []);
  }
  saveRouteHistory(items: RouteHistoryItem[]): void {
    writeJson(KEY_HISTORY, items.slice(0, HISTORY_LIMIT));
  }
  appendRouteHistoryItem(item: RouteHistoryItem): RouteHistoryItem[] {
    const merged = mergeRouteHistory(this.loadRouteHistory(), item);
    writeJson(KEY_HISTORY, merged);
    return merged;
  }
  removeRouteHistoryItem(id: string): RouteHistoryItem[] {
    const next = this.loadRouteHistory().filter((item) => item.id !== id);
    writeJson(KEY_HISTORY, next);
    return next;
  }

  loadImportDiagnostics(): ImportDiagnosticsEntry[] {
    return readJson<ImportDiagnosticsEntry[]>(KEY_DIAGNOSTICS, []);
  }
  saveImportDiagnostics(entries: ImportDiagnosticsEntry[]): void {
    writeJson(KEY_DIAGNOSTICS, entries.slice(0, DIAGNOSTICS_LIMIT));
  }
  appendImportDiagnostics(entry: ImportDiagnosticsEntry): ImportDiagnosticsEntry[] {
    const next = [entry, ...this.loadImportDiagnostics()].slice(0, DIAGNOSTICS_LIMIT);
    writeJson(KEY_DIAGNOSTICS, next);
    return next;
  }
  removeImportDiagnostics(id: string): ImportDiagnosticsEntry[] {
    const next = this.loadImportDiagnostics().filter((entry) => entry.id !== id);
    writeJson(KEY_DIAGNOSTICS, next);
    return next;
  }

  loadRoutingDiagnosticsSessions(): RoutingDiagSession[] {
    return readJson<RoutingDiagSession[]>(KEY_ROUTING_DIAGNOSTICS, []);
  }
  saveRoutingDiagnosticsSessions(sessions: RoutingDiagSession[]): void {
    writeJson(KEY_ROUTING_DIAGNOSTICS, sessions.slice(0, ROUTING_DIAGNOSTICS_SESSION_LIMIT));
  }
  appendRoutingDiagnosticsSession(session: RoutingDiagSession): RoutingDiagSession[] {
    const next = [session, ...this.loadRoutingDiagnosticsSessions()].slice(
      0,
      ROUTING_DIAGNOSTICS_SESSION_LIMIT,
    );
    writeJson(KEY_ROUTING_DIAGNOSTICS, next);
    return next;
  }
  removeRoutingDiagnosticsSession(id: string): RoutingDiagSession[] {
    const next = this.loadRoutingDiagnosticsSessions().filter((s) => s.id !== id);
    writeJson(KEY_ROUTING_DIAGNOSTICS, next);
    return next;
  }

  loadLastKnownRider(): CoordinatePoint | null {
    return readJsonOrUndefined<CoordinatePoint>(KEY_LAST_KNOWN_RIDER) ?? null;
  }
  saveLastKnownRider(value: CoordinatePoint): void {
    writeJson(KEY_LAST_KNOWN_RIDER, value);
  }

  loadLocationPromptShown(): boolean {
    return readJson(KEY_LOCATION_PROMPT_SHOWN, false);
  }
  saveLocationPromptShown(value: boolean): void {
    writeJson(KEY_LOCATION_PROMPT_SHOWN, value);
  }

  loadDebuggerAnnotations(sessionId: string): Annotation[] {
    const all = readJson<Record<string, Annotation[]>>("companion.debuggerAnnotations", {});
    return all[sessionId] ?? [];
  }
  saveDebuggerAnnotations(sessionId: string, annotations: Annotation[]): void {
    const all = readJson<Record<string, Annotation[]>>("companion.debuggerAnnotations", {});
    all[sessionId] = annotations;
    writeJson("companion.debuggerAnnotations", all);
  }

  loadPendingHomePresentation(): PendingHomePresentation | undefined {
    return readJsonOrUndefined<PendingHomePresentation>(KEY_PENDING);
  }
  savePendingHomePresentation(value: PendingHomePresentation | undefined): void {
    if (value) {
      writeJson(KEY_PENDING, value);
    } else {
      try {
        localStorage.removeItem(KEY_PENDING);
      } catch {
        /* ignore */
      }
    }
  }
}

export function mergeRecentDestinations(
  existing: CoordinatePoint[],
  incoming: CoordinatePoint,
): CoordinatePoint[] {
  const filtered = existing.filter(
    (point) => approximateDistanceMeters(point, incoming) > RECENT_DEDUP_METERS,
  );
  return [incoming, ...filtered].slice(0, RECENT_LIMIT);
}

export function mergeRouteHistory(
  existing: RouteHistoryItem[],
  incoming: RouteHistoryItem,
): RouteHistoryItem[] {
  if (incoming.source === "recentDestination" && incoming.destination) {
    const sameSpotIndex = existing.findIndex(
      (item) =>
        item.source === "recentDestination" &&
        item.destination &&
        approximateDistanceMeters(item.destination, incoming.destination as CoordinatePoint) <=
          RECENT_DEDUP_METERS,
    );
    if (sameSpotIndex >= 0) {
      const previous = existing[sameSpotIndex];
      const merged: RouteHistoryItem = {
        ...previous,
        title: chooseBetterTitle(previous.title, incoming.title),
        subtitle: incoming.subtitle || previous.subtitle,
        sourceLabel: incoming.sourceLabel,
        createdAtMs: incoming.createdAtMs,
        occurrenceCount: (previous.occurrenceCount ?? 1) + 1,
      };
      const without = existing.filter((_, idx) => idx !== sameSpotIndex);
      return [merged, ...without].slice(0, HISTORY_LIMIT);
    }
  }
  const without = existing.filter((item) => item.id !== incoming.id);
  return [incoming, ...without].slice(0, HISTORY_LIMIT);
}

function chooseBetterTitle(previous: string, incoming: string): string {
  const incomingTrim = incoming.trim();
  const previousTrim = previous.trim();
  if (!incomingTrim) return previousTrim;
  const generic = ["recent", "dropped pin", "selected destination", "no destination"];
  if (generic.some((g) => incomingTrim.toLowerCase() === g)) return previousTrim;
  return incomingTrim;
}

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as T;
    return parsed ?? fallback;
  } catch {
    return fallback;
  }
}

function readJsonOrUndefined<T>(key: string): T | undefined {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return undefined;
    return JSON.parse(raw) as T;
  } catch {
    return undefined;
  }
}

function writeJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* ignore quota or unavailable storage */
  }
}
