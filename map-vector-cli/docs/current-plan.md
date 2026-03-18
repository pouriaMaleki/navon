# map-vector-cli Current Plan

## Plan
1. Keep converter focused on city-scale georeferenced street vectors.
2. Preserve `.svm` compatibility for low-memory runtime use.
3. Emit integration metadata required by runtime camera projection.
4. Keep schema forward-compatible for future street tags/attributes.
5. Make feature filtering profile-driven for bike-runtime relevance.
6. Integrate converter crates into repository CVE detection and update automation.
7. Expand converter output toward declarative multi-band map presentation with richer feature classes and generalized zoom slices.

## TODO
- [x] Keep city-scale conversion output in `.svm`.
- [x] Include world-bounds metadata for runtime GPS projection bridge.
- [x] Keep reserved per-segment metadata fields (road class/lane/tag id).
- [x] Add bike profile filtering to exclude ferry/boat/water transport segments.
- [ ] Add `.svm` round-trip tests.
- [ ] Add conversion fixture tests.
- [ ] Add next adapters architecture (`geojson`, `osm.pbf`).
- [ ] Add converter dependency CVE checks via workspace `cargo audit`.
- [ ] Ensure converter crates are included in Dependabot cargo updates.
- [ ] Add declarative conversion profiles that define presentation bands and feature sets.
- [ ] Expand classification beyond `major` / `minor` / `path`.
- [ ] Design richer `.svm` records for multi-LOD and future label/building support.
- [ ] Add geometry simplification/generalization passes for farther zoom bands.
