# POI Layer Design

## Purpose
- Define the first shared POI layer system for the bike minimap.
- Keep POI behavior inside the shared map/query/render pipeline rather than emulator-only UI code.
- Normalize category handling so Helsinki and future regions can share the same runtime behavior.

## Product Goal
POIs should feel like useful minimap context, not like a dense phone-map overlay.

The first slice focuses on nearby utility and essentials:
- bicycle parking
- bicycle repair / pump stations
- supermarkets
- food

## First-Slice UX

### Close Detail
For stopped browsing and high zoom.
- Show:
  - bicycle parking
  - bicycle repair / pump stations
  - supermarkets
  - food
- Use small decluttered markers only.
- No text labels in the first slice.
- Bike utility should stay visually stronger than food/shop context.

### Ride Detail
For the default moving zoom.
- Show:
  - bicycle parking
  - bicycle repair / pump stations
- Hide:
  - supermarkets
  - food
- The moving map should stay useful without turning into a cluttered search overlay.

### Network Overview
- Hide POIs.

### District Overview
- Hide POIs.

## Decluttering Rules
- POIs are point features in the shared map package, not overlay annotations.
- Rendering should suppress markers that land too close together on screen.
- Bike utility categories have higher priority than essentials when deduplicating.
- The first implementation uses renderer-side screen-space spacing rather than full map-side clustering.

## Normalized Categories
The shared runtime/render path currently uses these normalized POI categories:
- `BikeParking`
- `BikeRepair`
- `Supermarket`
- `Food`

Future categories can be added without changing the ownership model.

## Data Strategy

### Current Helsinki Slice
- Source POIs from the existing OpenMapTiles / OSM-backed MBTiles `poi` layer already used for Helsinki.
- Normalize source `class` / `subclass` values into shared POI categories during conversion.
- Keep the runtime blind to Helsinki-specific source details.

### Future Direction
- Keep converter-side normalization as the integration point.
- Additional source adapters can map city-specific open data into the same normalized POI categories.
- This allows a future Helsinki official-data adapter without changing `runtime-core`, `map-runtime`, or emulator behavior.

## Ownership Boundaries
- `/work/tools/map-vector-cli`
  - owns POI source ingestion
  - owns source-to-category normalization
  - owns writing POI point records into `.svm`
- `/work/map-runtime`
  - owns decoding POI point records from `.svm`
  - owns returning point candidates in `MapQueryResult`
- `/work/runtime-core`
  - owns which POI categories are requested at each presentation band
- `/work/render-core`
  - owns POI styling, decluttering, and drawing
- adapters
  - must not add product POI policy

## Current Implementation Notes
- The first slice reuses the existing `.svm` record layout and marks point records with a geometry-kind byte.
- The current renderer uses shared SVG-backed icon assets for bike parking, repair, supermarket, and food markers.
- Future icon swaps should stay inside `render-core` without changing runtime/query behavior.

## Next Steps
- Add dedicated POI SVG icon assets.
- Add richer converter/provider modules so official city data can augment or replace OSM-backed POIs per region.
- Add clustering or aggregation for farther zooms if POI density grows.
- Add optional tap/browse interactions only after the non-interactive layer is stable.
