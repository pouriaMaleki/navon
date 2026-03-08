# map-vector-cli Project Spec

## Purpose
Convert large source maps into a compact georeferenced street-vector standard (`.svm`) for embedded/runtime consumers.

## Product Boundary
This tool does not implement minimap UX or camera behavior.
Its role is data preparation and schema standardization.

## Current Scope
- CLI-only host tool.
- Input: MBTiles (`*.mbtiles`).
- Output: city-scale `.svm` vector maps.
- Bridge output for current runtime integration: windowed generated Rust module.

## Data Standard Goals
- Preserve geospatial mapping between vectors and GPS coordinates.
- Keep schema compact for low-power runtimes.
- Reserve fields for future metadata/tagging:
  - street naming
  - lane count
  - additional attribute IDs

## Non-Goals
- No direct rendering.
- No firmware UI or interaction logic.
- No network tile fetch pipeline.

## Extensibility
Future adapters (`geojson`, `osm.pbf`, etc.) should emit the same `.svm` contract.
