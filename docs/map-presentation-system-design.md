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
- `/work/map-runtime`
  - owns reading the richer `.svm` package
  - owns coarse query of the requested feature classes and LOD slices
- `/work/runtime-core`
  - owns deciding which presentation band is active for the current zoom
  - owns requesting the appropriate map feature set through `MapQuerySpec`
- `/work/render-core`
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

## Recommended Implementation Phases
### Phase 1: Band And Feature-Class Foundation
- Expand feature classification beyond `MajorRoad` / `MinorRoad` / `Path`.
- Add declarative profile and band definitions in the converter.
- Expand `MapQuerySpec` and runtime selection logic.

### Phase 2: Building And Detail Geometry
- Add building outlines and simpler generalized building shapes.
- Add dark low-contrast style treatment for local context geometry.

### Phase 3: Generalization
- Merge or simplify street geometry for farther zoom bands.
- Reduce duplicate parallel linework where a single centerline is preferable.
- Replace the current coarse “hide lower-priority street layers” workaround with true geometric deduplication and centerline-style overview output.

### Phase 4: Labels
- Add neighborhood and major-road labels as a sparse overlay.
- Keep labels out of the first foundation slice unless geometry and LOD behavior are already stable.

### Phase 5: POI Enrichment
- Add dedicated POI icon assets and richer provider-side normalization.
- Add clustering or aggregation only after the base non-interactive POI layer is stable.

### Phase 6: Alternate Profiles
- Add profile variants like `car` or `transit` without rewriting runtime policy.

## First Concrete Slice
The first implementation slice should avoid trying to solve everything at once.

Recommended first slice:
- one richer map package
- four presentation bands
- expanded line feature classes
- building outlines in close and ride detail only
- no label rendering yet
- bike profile excludes rail, metro, train, and tram geometry
- overview bands prefer arterial + bike-main geometry over dense street-level detail
- first POI slice shows bike utility in ride detail and utility + essentials in close detail

This gets the biggest UX improvement while keeping the first milestone realistic.

## Immediate Repo Impact
The next planning and implementation work should touch:
- [`/work/tools/map-vector-cli/docs/project-spec.md`](/work/tools/map-vector-cli/docs/project-spec.md)
- [`/work/tools/map-vector-cli/docs/current-plan.md`](/work/tools/map-vector-cli/docs/current-plan.md)
- [`/work/docs/project-spec.md`](/work/docs/project-spec.md)- [`/work/docs/poi-layer-design.md`](/work/docs/poi-layer-design.md)
- `runtime-core` map query model
- `map-runtime` `.svm` reader/query backend
- `render-core` style and geometry rendering

## Assumptions
- Labels are a later phase, not part of the first implementation slice.
- One richer regional map package is preferred over many separate zoom-specific files.
- The system should stay profile-driven and converter-led rather than adapter-led.
