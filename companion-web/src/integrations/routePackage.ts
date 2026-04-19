import {
  type NormalizedRoutePackage,
  type RoutePlanRequest,
  type RoutePreviewModel,
  selectedAlternative,
} from "../domain/models.js";

/** Generate a UUID-like id for a RouteAlternative without depending on `crypto.randomUUID`. */
export function newAlternativeId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `alt-${Math.random().toString(36).slice(2, 10)}-${Date.now().toString(36)}`;
}

/** Pick the selected alternative's package, or fall back to the first. Throws if empty. */
export function normalizedFromPreview(
  preview: RoutePreviewModel,
  _request: RoutePlanRequest,
): NormalizedRoutePackage {
  const selected = selectedAlternative(preview);
  if (!selected) {
    throw new Error("No alternatives available to normalize");
  }
  return selected.normalizedPackage;
}
