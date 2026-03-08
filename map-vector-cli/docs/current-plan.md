# map-vector-cli Current Plan

## Plan
1. Lock city-scale `.svm` schema
- Keep georeferenced vector coordinates.
- Include forward-compatible segment metadata fields.

2. Implement robust MBTiles conversion
- Extract street vectors across city scope.
- Keep deterministic output and configurable zoom/limits.

3. Support runtime bridge outputs
- Generate windowed Rust module from `.svm` for current renderer integration.

4. Quality and compatibility
- Add parser/serializer round-trip tests.
- Add fixture snapshots for deterministic conversion.

## TODO
- [x] Create standalone converter crate.
- [x] Implement `convert-mbtiles` for city-scale vectors.
- [x] Implement `emit-rust-window` bridge command.
- [x] Add metadata-ready segment schema fields.
- [ ] Add `.svm` round-trip tests.
- [ ] Add conversion fixture tests.
- [ ] Add new source adapter architecture notes.
