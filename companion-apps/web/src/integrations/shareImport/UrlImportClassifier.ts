import type { CoordinatePoint, ImportClassification } from "../../domain/models.js";

export type ImportInput =
  | { kind: "url"; url: string }
  | { kind: "text"; text: string }
  | { kind: "file"; fileName: string; content: string };

export type ClassifiedImport = {
  classification: ImportClassification;
  coordinate?: CoordinatePoint;
  fileName?: string;
  fileContent?: string;
  note?: string;
  debugTrail: string[];
};

const COORD_REGEX = /(-?\d{1,3}(?:\.\d+)?)[,\s]+(-?\d{1,3}(?:\.\d+)?)/;
const GOOGLE_MAPS_AT_REGEX = /@(-?\d{1,3}(?:\.\d+)?),(-?\d{1,3}(?:\.\d+)?)/;
const GOOGLE_MAPS_QUERY_REGEX = /[?&]q=(-?\d{1,3}(?:\.\d+)?),(-?\d{1,3}(?:\.\d+)?)/;
const GARMIN_HOSTS = ["connect.garmin.com", "garmin.com"];
const GOOGLE_MAPS_HOSTS = ["google.com", "goo.gl", "maps.app.goo.gl", "consent.google.com"];

export function classifyImport(input: ImportInput): ClassifiedImport {
  const trail: string[] = [];

  if (input.kind === "file") {
    trail.push(`file=${input.fileName}`);
    if (/\.gpx$/i.test(input.fileName)) {
      return {
        classification: "gpxFile",
        fileName: input.fileName,
        fileContent: input.content,
        debugTrail: trail,
      };
    }
    if (/\.fit$/i.test(input.fileName)) {
      return {
        classification: "fitFile",
        fileName: input.fileName,
        fileContent: input.content,
        debugTrail: trail,
      };
    }
    if (/\.tcx$/i.test(input.fileName)) {
      return {
        classification: "tcxFile",
        fileName: input.fileName,
        fileContent: input.content,
        debugTrail: trail,
      };
    }
    if (
      input.content.includes("<gpx") ||
      input.content.includes("<rte") ||
      input.content.includes("<trk")
    ) {
      return {
        classification: "gpxFile",
        fileName: input.fileName,
        fileContent: input.content,
        debugTrail: [...trail, "sniffed-gpx-content"],
      };
    }
    return { classification: "unknown", fileName: input.fileName, debugTrail: trail };
  }

  if (input.kind === "url") {
    trail.push(`url=${input.url}`);
    let parsed: URL | null = null;
    try {
      parsed = new URL(input.url);
    } catch {
      trail.push("url-parse-failed");
    }
    if (parsed) {
      const host = parsed.hostname.toLowerCase();
      if (GARMIN_HOSTS.some((h) => host.endsWith(h))) {
        return {
          classification: "garminCourseLink",
          note: "Garmin course imports require manual GPX export.",
          debugTrail: trail,
        };
      }
      if (GOOGLE_MAPS_HOSTS.some((h) => host.endsWith(h))) {
        const coordinate =
          extractCoord(GOOGLE_MAPS_AT_REGEX, input.url) ??
          extractCoord(GOOGLE_MAPS_QUERY_REGEX, input.url);
        if (coordinate) {
          return {
            classification: "googleMapsLocationLink",
            coordinate,
            debugTrail: [...trail, "extracted-google-maps-coords"],
          };
        }
        return {
          classification: "googleMapsLocationLink",
          note: "Could not extract coordinates from Google Maps URL.",
          debugTrail: trail,
        };
      }
      const inlineCoord = extractCoord(COORD_REGEX, input.url);
      if (inlineCoord) {
        return {
          classification: "coordinatesText",
          coordinate: inlineCoord,
          debugTrail: [...trail, "extracted-inline-coords"],
        };
      }
      return {
        classification: "genericUrl",
        note: "Generic URL — no destination extracted.",
        debugTrail: trail,
      };
    }
    return { classification: "unknown", debugTrail: trail };
  }

  trail.push(`text=${input.text.slice(0, 80)}`);
  const coord = extractCoord(COORD_REGEX, input.text);
  if (coord) {
    return { classification: "coordinatesText", coordinate: coord, debugTrail: trail };
  }
  return { classification: "unknown", debugTrail: trail };
}

function extractCoord(regex: RegExp, source: string): CoordinatePoint | undefined {
  const match = regex.exec(source);
  if (!match) return undefined;
  const lat = parseFloat(match[1]);
  const lon = parseFloat(match[2]);
  if (!Number.isFinite(lat) || !Number.isFinite(lon)) return undefined;
  if (lat < -90 || lat > 90 || lon < -180 || lon > 180) return undefined;
  return { latitude: lat, longitude: lon };
}
