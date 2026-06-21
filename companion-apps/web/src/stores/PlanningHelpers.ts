import {
  ROUTE_PROVIDER_DISPLAY_NAME,
  type RouteAlternative,
  type RoutePreviewModel,
  type RouteSourceMode,
} from "../domain/index.js";

export function decoratePreview(
  preview: RoutePreviewModel,
  mode: RouteSourceMode,
): RoutePreviewModel {
  return {
    ...preview,
    alternatives: presentAlternatives(preview.alternatives, mode),
    selectedAlternativeID: preview.alternatives[0]?.id,
  };
}

export function mixedNotice(
  effective: RoutePreviewModel[],
  totalRacers: number,
  includeHsl: boolean,
): string {
  if (effective.length === 1 && effective[0].planningNotice) return effective[0].planningNotice;
  if (effective.length < totalRacers) {
    return "Showing available routes while some providers are hidden.";
  }
  return includeHsl ? "Mixed routes from HSL and OSM" : "OSM bike routes";
}

export function mergeMixedAlternatives(alternatives: RouteAlternative[]): RouteAlternative[] {
  if (alternatives.length === 0) return [];
  const sorted = [...alternatives].sort((a, b) => {
    if (a.durationSeconds === b.durationSeconds) return a.distanceMeters - b.distanceMeters;
    return a.durationSeconds - b.durationSeconds;
  });
  const remaining = [...sorted];
  const chosen: RouteAlternative[] = [];
  const takeFirstMatching = (predicate: (a: RouteAlternative) => boolean): void => {
    const idx = remaining.findIndex(predicate);
    if (idx >= 0) {
      chosen.push(remaining[idx]);
      remaining.splice(idx, 1);
    } else if (remaining.length > 0) {
      chosen.push(remaining.shift() as RouteAlternative);
    }
  };
  if (remaining.length > 0) {
    chosen.push(remaining.shift() as RouteAlternative);
  }
  takeFirstMatching((a) => a.normalizedPackage.provenance.providerID === "osm");
  takeFirstMatching(() => true);
  while (chosen.length < 3 && remaining.length > 0) {
    chosen.push(remaining.shift() as RouteAlternative);
  }
  return presentAlternatives(chosen, "mixed");
}

export function presentAlternatives(
  alternatives: RouteAlternative[],
  _mode: RouteSourceMode,
): RouteAlternative[] {
  return alternatives.slice(0, 3).map((alt) => {
    const label = friendlyAlternativeLabel(alt);
    return { ...alt, title: label.title, subtitle: label.subtitle };
  });
}

export function friendlyAlternativeLabel(alt: RouteAlternative): {
  title: string;
  subtitle: string;
} {
  const providerID = alt.normalizedPackage.provenance.providerID;
  const sourceRef = (alt.normalizedPackage.provenance.sourceReference ?? "").toLowerCase();
  if (providerID === "osm") {
    if (sourceRef.includes("fastbike")) return { title: "BRouter fastbike", subtitle: "" };
    if (sourceRef.includes("trekking")) return { title: "BRouter trekking", subtitle: "" };
    return { title: "OSM Route", subtitle: "" };
  }
  if (providerID === "hsl") {
    if (sourceRef.includes("fastest")) return { title: "HSL Fastest", subtitle: "" };
    return { title: "HSL Route", subtitle: "" };
  }
  return { title: ROUTE_PROVIDER_DISPLAY_NAME[providerID], subtitle: "" };
}
