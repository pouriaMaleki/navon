import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export type UxConstants = {
  enterMovingKph: number;
  exitMovingKph: number;
  movingDwellMs: number;
  recenterInactivityMs: number;
  northOverrideTimeoutMs: number;
  doubleTapWindowMs: number;
  offRouteDistanceM: number;
  offRouteDwellMs: number;
  typeaheadDebounceMs: number;
  stationaryViewRadiusM: number;
  recentsPageSize: number;
};

let cached: UxConstants | null = null;

/**
 * Read the shared UX-constants TOML from `parity-fixtures/data/ux-constants.toml`.
 * The parser is intentionally tiny — the file is trivial key = value pairs with
 * comments. Keeping it in-tree avoids pulling a TOML dep into test runtime.
 */
export function loadUxConstants(): UxConstants {
  if (cached) return cached;
  const thisFile = fileURLToPath(import.meta.url);
  const path = resolve(
    dirname(thisFile),
    "../../../../../data/parity-fixtures/data/ux-constants.toml",
  );
  const text = readFileSync(path, "utf-8");
  const values: Record<string, number> = {};
  for (const line of text.split("\n")) {
    const clean = line.split("#")[0]?.trim() ?? "";
    if (!clean.includes("=")) continue;
    const [keyRaw, rawValue] = clean.split("=", 2);
    const key = keyRaw?.trim();
    const value = Number.parseFloat(rawValue?.trim() ?? "");
    if (key && Number.isFinite(value)) values[key] = value;
  }
  cached = {
    enterMovingKph: values.enter_moving_kph,
    exitMovingKph: values.exit_moving_kph,
    movingDwellMs: values.moving_dwell_ms,
    recenterInactivityMs: values.recenter_inactivity_ms,
    northOverrideTimeoutMs: values.north_override_timeout_ms,
    doubleTapWindowMs: values.double_tap_window_ms,
    offRouteDistanceM: values.off_route_distance_m,
    offRouteDwellMs: values.off_route_dwell_ms,
    typeaheadDebounceMs: values.typeahead_debounce_ms,
    stationaryViewRadiusM: values.stationary_view_radius_m,
    recentsPageSize: values.recents_page_size,
  };
  return cached;
}
