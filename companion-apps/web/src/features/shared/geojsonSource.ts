import type { GeoJSONSource, Map as MaplibreMap } from "maplibre-gl";

/**
 * Looks up a source by id and verifies it is a GeoJSON source before casting.
 * `map.getSource` alone only proves the id exists — a same-id source of a
 * different type would make the cast lie and `setData` throw.
 */
export function getGeojsonSource(map: MaplibreMap, id: string): GeoJSONSource | null {
  const source = map.getSource(id);
  if (!source || !("setData" in source)) return null;
  return source as GeoJSONSource;
}
