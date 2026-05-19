import type { Annotation } from "../../domain/debuggerModels.js";
import type { CoordinatePoint } from "../../domain/models.js";
import type { RoutingDiagEvent } from "../../domain/routingDiagnosticsModels.js";

export function buildDebugRouteFeature(gpxGeometry?: CoordinatePoint[]): GeoJSON.Feature | null {
  if (!gpxGeometry || gpxGeometry.length < 2) return null;
  return {
    type: "Feature",
    properties: { kind: "route" },
    geometry: {
      type: "LineString",
      coordinates: gpxGeometry.map((p) => [p.longitude, p.latitude]),
    },
  };
}

export function buildGpsTrailFeatures(
  events: RoutingDiagEvent[],
  currentTimeMs: number,
): GeoJSON.Feature[] {
  const locationEvents = events.filter(
    (e) => e.data.kind === "locationUpdate" && e.timestampMs <= currentTimeMs,
  );
  if (locationEvents.length === 0) return [];

  const startTime = events.length > 0 ? events[0].timestampMs : 0;
  const duration = currentTimeMs - startTime || 1;

  const features: GeoJSON.Feature[] = [];
  for (const e of locationEvents) {
    if (e.data.kind !== "locationUpdate") continue;
    const t = (e.timestampMs - startTime) / duration;
    features.push({
      type: "Feature",
      properties: { kind: "gps-dot", timeFraction: t },
      geometry: { type: "Point", coordinates: [e.data.lon, e.data.lat] },
    });
  }
  return features;
}

export function buildCueMarkerFeatures(
  events: RoutingDiagEvent[],
  currentTimeMs: number,
): GeoJSON.Feature[] {
  const features: GeoJSON.Feature[] = [];
  for (const e of events) {
    if (e.data.kind !== "audioCueDispatched" || e.timestampMs > currentTimeMs) continue;
    const nearestGps = findNearestLocation(events, e.timestampMs);
    if (!nearestGps || nearestGps.data.kind !== "locationUpdate") continue;
    features.push({
      type: "Feature",
      properties: {
        kind: "cue",
        eventId: e.id,
        messageText: e.data.messageText,
        cueType: e.data.cueType,
      },
      geometry: { type: "Point", coordinates: [nearestGps.data.lon, nearestGps.data.lat] },
    });
  }
  return features;
}

export function buildOffRouteSegmentFeatures(
  events: RoutingDiagEvent[],
  currentTimeMs: number,
): GeoJSON.Feature[] {
  const features: GeoJSON.Feature[] = [];
  for (const e of events) {
    if (e.data.kind !== "offRouteDetected" || e.timestampMs > currentTimeMs) continue;
    const nearestGps = findNearestLocation(events, e.timestampMs);
    if (!nearestGps || nearestGps.data.kind !== "locationUpdate") continue;
    features.push({
      type: "Feature",
      properties: {
        kind: "offroute",
        eventId: e.id,
        distanceM: e.data.distanceM,
      },
      geometry: { type: "Point", coordinates: [nearestGps.data.lon, nearestGps.data.lat] },
    });
  }
  return features;
}

export function buildRiderFeature(riderPosition: CoordinatePoint | null): GeoJSON.Feature[] {
  if (!riderPosition) return [];
  return [
    {
      type: "Feature",
      properties: { kind: "rider" },
      geometry: {
        type: "Point",
        coordinates: [riderPosition.longitude, riderPosition.latitude],
      },
    },
  ];
}

export function buildAnnotationPinFeatures(annotations: Annotation[]): GeoJSON.Feature[] {
  return annotations
    .filter((a) => a.coordinate)
    .map((a) => ({
      type: "Feature" as const,
      properties: {
        kind: "annotation-pin",
        annotationId: a.id,
        tag: a.tag,
        severity: a.severity,
        note: a.note.slice(0, 60),
      },
      geometry: {
        type: "Point" as const,
        coordinates: [
          (a.coordinate as CoordinatePoint).longitude,
          (a.coordinate as CoordinatePoint).latitude,
        ],
      },
    }));
}

function findNearestLocation(
  events: RoutingDiagEvent[],
  timestampMs: number,
): RoutingDiagEvent | null {
  let best: RoutingDiagEvent | null = null;
  let bestDelta = Infinity;
  for (const e of events) {
    if (e.data.kind !== "locationUpdate") continue;
    const delta = Math.abs(e.timestampMs - timestampMs);
    if (delta < bestDelta) {
      bestDelta = delta;
      best = e;
    }
  }
  return best;
}
