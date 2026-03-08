# map-vector-cli Current Plan

## Plan
1. Keep converter focused on city-scale georeferenced street vectors.
2. Preserve `.svm` compatibility for low-memory runtime use.
3. Emit integration metadata required by runtime camera projection.
4. Keep schema forward-compatible for future street tags/attributes.

## TODO
- [x] Keep city-scale conversion output in `.svm`.
- [x] Include world-bounds metadata for runtime GPS projection bridge.
- [x] Keep reserved per-segment metadata fields (road class/lane/tag id).
- [ ] Add `.svm` round-trip tests.
- [ ] Add conversion fixture tests.
- [ ] Add next adapters architecture (`geojson`, `osm.pbf`).
