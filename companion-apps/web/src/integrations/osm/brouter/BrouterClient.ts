import type { CoordinatePoint } from "../../../domain/models.js";

/**
 * BRouter is a free, public, OSM-based cycling routing service. Each call
 * returns ONE route. To diversify alternatives we call multiple profiles
 * in parallel from the orchestrator.
 *
 * Spec source: see `docs/companion-app-architecture.md` "OSM cycling
 * sources" — the `OsmCyclingRoutingAdapter` orchestrates fastbike +
 * trekking BRouter calls plus an OSRM bike call so the user picks among
 * routes with different cycle-infrastructure trade-offs.
 *
 * `timode=2` is required to populate `voicehints`; without it BRouter
 * returns a route with no turn instructions.
 */
export type BrouterProfile = "trekking" | "fastbike" | "safety";

export type BrouterFeature = {
  type: "Feature";
  properties: {
    "track-length"?: string;
    "total-time"?: string;
    voicehints?: number[][];
    [key: string]: unknown;
  };
  geometry: {
    type: "LineString";
    coordinates: number[][];
  };
};

export type BrouterResponse = {
  type: "FeatureCollection";
  features: BrouterFeature[];
};

export const BROUTER_BASE = "https://brouter.de/brouter";

export async function fetchBrouter(
  profile: BrouterProfile,
  origin: CoordinatePoint,
  destination: CoordinatePoint,
  signal?: AbortSignal,
): Promise<BrouterFeature> {
  const lonlats = `${origin.longitude.toFixed(6)},${origin.latitude.toFixed(6)}|${destination.longitude.toFixed(6)},${destination.latitude.toFixed(6)}`;
  const url = `${BROUTER_BASE}?lonlats=${encodeURIComponent(lonlats)}&profile=${profile}&alternativeidx=0&format=geojson&timode=2`;
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const data = (await response.json()) as BrouterResponse;
  if (!data.features || data.features.length === 0) {
    throw new Error("BRouter returned no features");
  }
  return data.features[0];
}
