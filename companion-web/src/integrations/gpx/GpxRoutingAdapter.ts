import {
  type ActiveRouteSession,
  type CoordinatePoint,
  CURRENT_ROUTE_PACKAGE_VERSION,
  type NormalizedRoutePackage,
  type RouteAlternative,
  type RouteManeuver,
  type RoutePlanRequest,
  type RoutePreviewModel,
  type RouteProviderID,
} from "../../domain/models.js";
import type { RoutingProvider } from "../../domain/providers.js";
import { classifyTurn, cumulativeDistances, turnDeltaDegrees } from "../geo.js";
import { newAlternativeId, normalizedFromPreview } from "../routePackage.js";

export class GpxRoutingAdapter implements RoutingProvider {
  readonly providerID: RouteProviderID = "gpxImport";

  async planRoute(_request: RoutePlanRequest): Promise<RoutePreviewModel> {
    throw new Error("Select a GPX file instead of using coordinate planning.");
  }

  async replanRoute(
    _session: ActiveRouteSession,
    _riderLocation: CoordinatePoint,
  ): Promise<RoutePreviewModel> {
    throw new Error("Reroute is not supported for imported GPX routes yet.");
  }

  normalizePreview(preview: RoutePreviewModel, request: RoutePlanRequest): NormalizedRoutePackage {
    return normalizedFromPreview(preview, request);
  }

  importFile(fileName: string, xmlText: string, revision = 1): RoutePreviewModel {
    const parsed = parseGpx(xmlText);
    const routeName = parsed.routeName ?? fileName.replace(/\.gpx$/i, "");
    const routeID = slugify(routeName);
    const geometry = parsed.points.map((p) => p.point);
    const cumulative = cumulativeDistances(geometry);
    const totalDistance = cumulative[cumulative.length - 1] ?? 0;
    const maneuvers = buildGpxManeuvers(parsed.points, cumulative, parsed.preferPointLabels);
    const package_: NormalizedRoutePackage = {
      version: CURRENT_ROUTE_PACKAGE_VERSION,
      routeIdentifier: routeID,
      revision,
      geometry,
      maneuvers,
      summary: {
        totalDistanceMeters: totalDistance,
        estimatedDurationSeconds: Math.max(Math.round(totalDistance / 5.0), 60),
        startLabel: parsed.points[0]?.label,
        destinationLabel: parsed.points[parsed.points.length - 1]?.label ?? routeName,
      },
      provenance: {
        providerID: "gpxImport",
        sourceReference: fileName,
        generatedAtUnixMs: Date.now(),
      },
    };
    const alternative: RouteAlternative = {
      id: newAlternativeId(),
      title: routeName,
      subtitle: "Imported GPX route",
      distanceMeters: Math.round(totalDistance),
      durationSeconds: package_.summary.estimatedDurationSeconds,
      normalizedPackage: package_,
    };
    return {
      alternatives: [alternative],
      selectedAlternativeID: alternative.id,
      routeIdentifier: package_.routeIdentifier,
      routeRevision: package_.revision,
      planningNotice: `Imported ${fileName}`,
    };
  }
}

type GpxPoint = { point: CoordinatePoint; label?: string };
type ParsedGpx = { routeName?: string; points: GpxPoint[]; preferPointLabels: boolean };

export function parseGpx(xmlText: string): ParsedGpx {
  const parser = new DOMParser();
  const doc = parser.parseFromString(xmlText, "application/xml");
  const parserErrors = doc.getElementsByTagName("parsererror");
  if (parserErrors.length > 0) {
    throw new Error(`GPX import failed: ${parserErrors[0].textContent ?? "parser error"}`);
  }
  const metadataName = textContent(doc.querySelector("metadata > name"));
  const routeNodes = Array.from(doc.getElementsByTagName("rte"));
  const trackNodes = Array.from(doc.getElementsByTagName("trk"));

  const routePoints: GpxPoint[] = [];
  for (const rte of routeNodes) {
    for (const pt of Array.from(rte.getElementsByTagName("rtept"))) {
      const point = pointFromAttrs(pt);
      if (point) routePoints.push(point);
    }
  }
  const trackPoints: GpxPoint[] = [];
  for (const trk of trackNodes) {
    for (const pt of Array.from(trk.getElementsByTagName("trkpt"))) {
      const point = pointFromAttrs(pt);
      if (point) trackPoints.push(point);
    }
  }
  const dedupedRoute = dedupeGpx(routePoints);
  const dedupedTrack = dedupeGpx(trackPoints);
  const chosen = dedupedRoute.length >= 2 ? dedupedRoute : dedupedTrack;
  if (chosen.length < 2) {
    throw new Error("GPX did not contain a usable route or track.");
  }
  const routeName =
    metadataName ??
    textContent(routeNodes[0]?.querySelector(":scope > name")) ??
    textContent(trackNodes[0]?.querySelector(":scope > name")) ??
    undefined;
  return { routeName, points: chosen, preferPointLabels: dedupedRoute.length >= 2 };
}

function pointFromAttrs(node: Element): GpxPoint | null {
  const lat = parseFloat(node.getAttribute("lat") ?? "");
  const lon = parseFloat(node.getAttribute("lon") ?? "");
  if (!Number.isFinite(lat) || !Number.isFinite(lon)) return null;
  const label =
    textContent(node.querySelector(":scope > name")) ??
    textContent(node.querySelector(":scope > desc")) ??
    textContent(node.querySelector(":scope > cmt")) ??
    undefined;
  return { point: { latitude: lat, longitude: lon }, label };
}

function textContent(node: Element | null | undefined): string | undefined {
  const text = node?.textContent?.trim();
  return text && text.length > 0 ? text : undefined;
}

function dedupeGpx(points: GpxPoint[]): GpxPoint[] {
  const out: GpxPoint[] = [];
  for (const p of points) {
    const last = out[out.length - 1];
    if (
      !last ||
      last.point.latitude !== p.point.latitude ||
      last.point.longitude !== p.point.longitude
    ) {
      out.push(p);
    }
  }
  return out;
}

function buildGpxManeuvers(
  points: GpxPoint[],
  cumulative: number[],
  preferPointLabels: boolean,
): RouteManeuver[] {
  if (points.length === 0) return [];
  const first = points[0];
  const last = points[points.length - 1];
  const maneuvers: RouteManeuver[] = [
    {
      id: "depart",
      maneuverType: "depart",
      location: first.point,
      distanceFromStartMeters: 0,
      distanceToNextMeters: cumulative[1],
      instructionText: "Start riding",
    },
  ];
  for (let i = 1; i < points.length - 1; i++) {
    const pointLabel = points[i].label;
    const delta = turnDeltaDegrees(points[i - 1].point, points[i].point, points[i + 1].point);
    const turn = classifyTurn(delta);
    if (!turn && !(preferPointLabels && pointLabel)) continue;
    const distanceToNext =
      i + 1 < cumulative.length ? cumulative[i + 1] - cumulative[i] : undefined;
    maneuvers.push({
      id: `step-${i}`,
      maneuverType: turn?.type ?? "straight",
      location: points[i].point,
      distanceFromStartMeters: cumulative[i],
      distanceToNextMeters: distanceToNext,
      instructionText: pointLabel ?? turn?.instruction,
    });
  }
  maneuvers.push({
    id: "arrive",
    maneuverType: "arrive",
    location: last.point,
    distanceFromStartMeters: cumulative[cumulative.length - 1] ?? 0,
    instructionText: "Arrive at destination",
  });
  return maneuvers;
}

export function slugify(value: string): string {
  let out = "";
  let previousDash = false;
  for (const ch of value.toLowerCase()) {
    const isAlphaNum = /[a-z0-9]/.test(ch);
    const next = isAlphaNum ? ch : "-";
    if (next === "-") {
      if (previousDash) continue;
      previousDash = true;
      out += next;
    } else {
      previousDash = false;
      out += next;
    }
  }
  const trimmed = out.replace(/^-+|-+$/g, "");
  return trimmed.length === 0 ? "gpx-import" : trimmed;
}
