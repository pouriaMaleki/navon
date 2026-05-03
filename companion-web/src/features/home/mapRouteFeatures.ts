import type { RootStore } from "../../app/RootStore.js";
import type { CoordinatePoint, RouteAlternative } from "../../domain/models.js";

export function buildRouteFeatures(store: RootStore): GeoJSON.Feature[] {
  const features: GeoJSON.Feature[] = [];
  const preview = store.planningStore.preview;
  const guidance = store.guidanceStore;
  const homeMode = guidance.homeMode;
  const selectedId = preview.selectedAlternativeID ?? preview.alternatives[0]?.id;
  const showAlternates = homeMode === "planning";

  if (homeMode === "phoneGuidance") {
    // During exploration show the frozen active route + the new alternatives
    if (guidance.isExploringAlternativesFromGuidance) {
      const exploreSelectedId = guidance.selectedAlternativeIDForDisplay;
      const activeRoute = guidance.guidanceRoute;
      if (activeRoute) {
        features.push({
          type: "Feature",
          properties: { selected: exploreSelectedId === undefined, completed: false },
          geometry: {
            type: "LineString",
            coordinates: activeRoute.geometry.map((p) => [p.longitude, p.latitude]),
          },
        });
      }
      for (const alt of preview.alternatives) {
        features.push(routeFeature(alt, alt.id === exploreSelectedId));
      }
      return features;
    }

    // Normal guidance: show completed/remaining split
    const split = guidance.routeSplit;
    if (split) {
      if (split.remaining.length >= 2) {
        features.push({
          type: "Feature",
          properties: { selected: true, completed: false },
          geometry: {
            type: "LineString",
            coordinates: split.remaining.map((p) => [p.longitude, p.latitude]),
          },
        });
      }
      if (split.completed.length >= 2) {
        features.push({
          type: "Feature",
          properties: { selected: false, completed: true },
          geometry: {
            type: "LineString",
            coordinates: split.completed.map((p) => [p.longitude, p.latitude]),
          },
        });
      }
      return features;
    }
  }

  for (const alt of preview.alternatives) {
    const isSelected = alt.id === selectedId;
    if (!showAlternates && !isSelected) continue;
    features.push(routeFeature(alt, isSelected));
  }
  return features;
}

function routeFeature(alt: RouteAlternative, selected: boolean): GeoJSON.Feature {
  return {
    type: "Feature",
    properties: { id: alt.id, selected },
    geometry: {
      type: "LineString",
      coordinates: alt.normalizedPackage.geometry.map((p: CoordinatePoint) => [p.longitude, p.latitude]),
    },
  };
}
