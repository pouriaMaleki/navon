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
- Future conversion profiles should also define presentation bands, feature classes, and simplification/generalization rules.
- The current converter also normalizes first-slice POI point categories from source map data.

## Data Model Goals
- Preserve GPS-to-vector mapping through georeferenced world coordinates.
- Store line and point geometry in a compact runtime-friendly format.
- Keep format compact for low-power runtime loading.
- Reserve per-segment metadata fields for future attributes (lane count, names, tags).
- For bike profile, include bike-relevant streets/paths and exclude ferry/boat/water transport lanes.
- Evolve from a small road-class model into richer feature classes that can express:
  - major and minor streets
  - main and local bike paths
  - building outlines
  - normalized POI categories such as bike parking, repair, supermarket, and food
  - generalized overview geometry
  - future labels and anchors

## Presentation-System Direction
- The converter should become the declarative source of truth for map presentation bands.
- Preferred output direction:
  - one regional `.svm` package with multiple feature classes and LOD slices
  - profile-driven inclusion/exclusion rules per zoom band
  - generalized geometry for farther zoom bands
- The current bike profile should:
  - exclude rail, tram, metro, and train geometry
  - keep ride-detail focused on roads, bike routes, and building outlines
  - normalize selected POI categories from the source `poi` layer
  - let overview bands drop street-level detail before true generalization is available
- The canonical system design lives in [`/work/docs/map-presentation-system-design.md`](/work/docs/map-presentation-system-design.md).
- The canonical POI design lives in [`/work/docs/poi-layer-design.md`](/work/docs/poi-layer-design.md).

## Extensibility
Future source adapters (`geojson`, `osm.pbf`, etc.) must emit the same `.svm` contract and declarative profile model.

## Security and CVE Management
- Converter dependencies in the Rust workspace must be covered by repository vulnerability scanning.
- Converter dependency changes must pass automated CVE checks in pull requests.
- Security updates to converter-relevant crates should be surfaced as automated dependency update PRs when available.
