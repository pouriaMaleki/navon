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
- Optional bridge output: windowed generated Rust module for current wasm integration.

## Data Model Goals
- Preserve GPS-to-vector mapping through georeferenced world coordinates.
- Store street vectors (points-of-interest and rich tags are future scope).
- Keep format compact for low-power runtime loading.
- Reserve per-segment metadata fields for future attributes (lane count, names, tags).

## Extensibility
Future source adapters (`geojson`, `osm.pbf`, etc.) must emit the same `.svm` contract.
