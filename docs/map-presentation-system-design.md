# Map Presentation System Design

## Purpose
- Define the next-generation map presentation system for the bike minimap.
- Support richer zoom-dependent context without pushing style or camera policy into adapters.
- Keep the system declarative and extensible so future profiles like `bike`, `car`, or `transit` can share the same pipeline.

## Product Goal
The map should not behave like one flat vector dataset scaled up and down. It should change presentation intent as zoom changes:
- close zooms feel local and spatially rich
- default riding zoom feels legible while moving
- farther zooms feel simplified and network-oriented
- overview zooms show only the most important context

## User-Facing Zoom Bands
The initial UX target is four presentation bands.

### 1. Close Detail
For stopped use, slow movement, and high zoom.
- Show:
  - major streets
  - minor streets
  - main bike lanes
  - local bike paths
  - smaller paths where useful
  - building outlines near the rider
  - nearby bike parking, bike repair, supermarket, and food POIs
- Building outlines should render in low-contrast colors close to the background.
- Dense geometry is acceptable because the user is zoomed in enough to read it.

### 2. Ride Detail
For the normal riding/default zoom.
- Show:
  - arterial roads
  - street roads
  - main bike routes
  - local bike routes
  - building outlines
  - bike parking
  - bike repair / pump stations
- Remove path noise that does not help riding decisions.
- Rail, metro, train, tram, and other transit track geometry should stay hidden in the bike profile.
- Hide lower-priority essentials like supermarkets and food in this band.
- The map should still feel place-aware, but cleaner than `Close Detail`.

### 3. Network Overview
For zoomed-out riding and route awareness.
- Show:
  - arterial roads
  - main bike corridors
- Hide:
  - small paths
  - service-level detail
  - ordinary street-road detail
  - building outlines
- Streets that are currently stored as many parallel edges should be generalized into fewer visual lines where possible.
- Until true geometry generalization exists, the overview band should avoid requesting street-level detail that makes split Helsinki corridors look doubled.

### 4. District Overview
For high-level orientation.
- Show:
  - arterial roads
  - major bike corridors
  - neighborhood or district names
- Hide:
  - local paths
  - most street-level geometry
  - rail and transit lines
  - building geometry
- The goal is fast mental orientation, not local navigation detail.

## Preferred Data Strategy
Preferred direction: one generated map package per region that contains multiple feature classes and multiple LOD slices.

Why this is preferred over many separate map files:
- one artifact is easier to package for emulator and firmware
- runtime can switch presentation bands without swapping files
- shared geometry and metadata can be reused across bands
- future profiles can stay declarative instead of multiplying file-management complexity

Multiple files per region are still a valid fallback if storage or tooling constraints make a single richer package too complex, but that should not be the default direction.

## Declarative Profile Model
Map presentation should be driven by declarative converter profiles rather than hardcoded runtime logic.

A profile should be able to define:
- transport intent:
  - `bike`
  - `car`
  - `transit`
- feature classification rules
- zoom bands
- inclusion and exclusion rules per band
- simplification or generalization rules per band
- style-class identifiers that `render-core` can map to colors and stroke widths

Illustrative shape:

```toml
[profile]
name = "bike"

[[band]]
id = "close_detail"
min_zoom = 16.0
features = [
  "road_major",
  "road_minor",
  "bike_lane_main",
  "bike_path_local",
  "footpath_local",
  "building_outline_dense",
]

[[band]]
id = "ride_detail"
min_zoom = 14.0
features = [
  "road_major",
  "road_minor",
  "bike_lane_main",
  "bike_path_main",
  "building_outline_simple",
]

[[band]]
id = "network_overview"
min_zoom = 12.0
features = [
  "road_major",
  "road_minor_selected",
  "bike_corridor_main",
]

[[band]]
id = "district_overview"
min_zoom = 0.0
features = [
  "road_arterial",
  "bike_corridor_main",
  "label_neighborhood",
]
```

## Data Model Direction
The current map layer model is too small for this UX. The system should grow from a small fixed road-class model into a richer feature-class model.

Likely future feature classes:
- `road_major`
- `road_minor`
- `road_local`
- `bike_lane_main`
- `bike_path_main`
- `bike_path_local`
- `footpath_local`
- `building_outline_dense`
- `building_outline_simple`
- `road_arterial_generalized`
- `label_neighborhood`
- `label_major_road`

Geometry types should also expand over time:
- polyline: roads, bike lanes, paths
- polygon or outline: buildings, blocks
- point or label anchor: neighborhoods, major street names
- point POIs: bike parking, repair, supermarket, food

## Ownership Boundaries
- `/work/tools/map-vector-cli`
  - owns source ingestion
  - owns feature classification
  - owns simplification/generalization
  - owns declarative profile parsing
  - owns emitting richer `.svm` data
- `/work/device/core/map-runtime`
  - owns reading the richer `.svm` package
  - owns coarse query of the requested feature classes and LOD slices
- `/work/device/core/runtime-core`
  - owns deciding which presentation band is active for the current zoom
  - owns requesting the appropriate map feature set through `MapQuerySpec`
- `/work/device/core/render-core`
  - owns styling the resulting feature classes
  - owns final visibility/clipping and screen-space rendering
- adapters (`firmware`, `render-core-wasm`, emulator TS)
  - must not own zoom-band map policy

## Query Model Changes
The current query model uses broad zoom buckets and a small `LodMask`. That should evolve toward a richer query contract.

Likely additions:
- presentation-band identifier
- feature-class mask or feature-set id
- optional profile id
- optional label enablement flag

The runtime should still output a compact `MapQuerySpec`, but that spec should be expressive enough to say:
- which map presentation band is active
- which feature families are needed
- whether text labels are allowed for this zoom/profile

## Rendering Direction
The renderer should continue to stay stateless, but it will need richer style classes.

Expected visual behavior:
- close-detail buildings render dark and low-contrast
- riding-detail streets and bike lanes stay legible while moving
- overview roads use simplified stronger strokes
- labels, if enabled, are zoom-band-specific and sparse

