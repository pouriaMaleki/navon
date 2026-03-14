# map-vector-cli Project Spec

## Purpose
Convert large map datasets into a compact georeferenced street-vector standard (`.svm`) for runtime minimap systems.

## Product Boundary
- CLI converter only.
- No minimap camera or UI behavior.
- No firmware/touch/GPS runtime logic.

## Current Scope
- Input: MBTiles (`*.mbtiles`) from `/work/map-src`.
- Output: city-scale `.svm` vector map in `/work/map-data`.
- Conversion profiles define transport filtering policy (current default: `bike`).

## Data Model Goals
- Preserve GPS-to-vector mapping through georeferenced world coordinates.
- Store street vectors (points-of-interest and rich tags are future scope).
- Keep format compact for low-power runtime loading.
- Reserve per-segment metadata fields for future attributes (lane count, names, tags).
- For bike profile, include bike-relevant streets/paths and exclude ferry/boat/water transport lanes.

## Extensibility
Future source adapters (`geojson`, `osm.pbf`, etc.) must emit the same `.svm` contract.

## Security and CVE Management
- Converter dependencies in the Rust workspace must be covered by repository vulnerability scanning.
- Converter dependency changes must pass automated CVE checks in pull requests.
- Security updates to converter-relevant crates should be surfaced as automated dependency update PRs when available.
