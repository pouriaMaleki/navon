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
- [x] Normalize first-slice POI categories from source map data.
- [ ] Add `.svm` round-trip tests.
- [ ] Add conversion fixture tests.
- [ ] Add next adapters architecture (`geojson`, `osm.pbf`).
- [ ] Add converter dependency CVE checks via workspace `cargo audit`.
- [ ] Ensure converter crates remain covered by repository-owned Rust CVE checks without relying on auto-update PR tooling.
- [ ] Add declarative conversion profiles that define presentation bands and feature sets.
- [x] Expand classification beyond `major` / `minor` / `path`.
- [ ] Split converter normalization into profile/provider modules so city-specific official POI sources can plug into the same normalized categories.
- [ ] Design richer `.svm` records for multi-LOD and future label/building support.
- [ ] Add geometry simplification/generalization passes for farther zoom bands.
- [ ] Replace overview-band street suppression with true split-street deduplication / centerline generalization.
- [ ] Add declarative transit exclusion rules per profile instead of growing hardcoded bike-only filters forever.
