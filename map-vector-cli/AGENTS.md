# Agents Guide (map-vector-cli)

## Scope
- This project is a host-side CLI map converter.
- It is intentionally separate from firmware/runtime rendering code.

## Canonical Docs
- Spec: `/work/map-vector-cli/docs/project-spec.md`
- Current plan: `/work/map-vector-cli/docs/current-plan.md`
- Format decision: `/work/map-vector-cli/docs/vector-format-options.md`

## Development Workflow
1. Update spec and plan docs first when requirements change.
2. Keep `.svm` format backward-compatible when possible.
3. Add new source format support in CLI adapters, not in firmware renderer.
4. Keep output deterministic for regression tests.

## Current Commands
- `convert-mbtiles`: MBTiles -> `.svm`

## Future Direction
- Add additional adapters (`geojson`, `osm.pbf`, etc.) that all emit the same `.svm` standard.
