# Project Specification

## Product Definition
ESP32 bike minimap renderer in video-game style.

The minimap product behavior (main project) is:
- User location is centered by default.
- Map orientation follows travel direction (heading-up by default).
- User can zoom in/out (target interaction: two-finger gesture when touch stack is ready).
- User can temporarily pan to inspect nearby streets.
- Map smoothly returns to centered-follow mode after a short idle period.

## Architecture Separation
- Main ESP32 project (`/work`):
  - runtime map camera logic
  - rendering and style
  - interaction behavior
  - packaging/deployment flow
- Map conversion project (`/work/map-vector-cli`):
  - source map ingestion/conversion
  - standard vector format ownership (`.svm`)
  - georeferenced vector output for runtime consumers

## Map Data Pipeline
- Source folder: `/work/map-src`
- Source type now: `*.mbtiles`
- Converted output folder: `/work/map-data`
- Canonical intermediate format: `.svm`
- Current integration bridge: `.svm` -> generated Rust module for existing renderer

## `.svm` Contract (Current)
- City-scale street vectors (not viewport-normalized source data).
- Coordinates map to georeferenced world space (Web Mercator tile world units).
- Includes schema capacity for future per-segment attributes:
  - road class
  - lane count (reserved now)
  - attr/tag id (reserved now)

## Vector Standard Alternatives (Summary)
- GeoJSON:
  - Pro: readable, ubiquitous.
  - Con: too heavy for low-memory runtime usage.
- Runtime MVT/Protobuf decoding:
  - Pro: standard ecosystem format.
  - Con: unnecessary runtime decode complexity on ESP32.
- FlatBuffers/Cap'n Proto:
  - Pro: efficient binary framing.
  - Con: extra schema/tooling overhead for this specific minimap path.
- Custom fixed-point/binary `.svm` [Selected]:
  - Pro: minimal parse overhead, deterministic, easy to tailor to renderer needs.
  - Con: project-owned versioning/migration burden.

## Decision
Use `.svm` as the stable intermediate vector standard and keep conversion concerns outside firmware/runtime.
