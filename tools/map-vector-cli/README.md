# map-vector-cli

Host-side Rust CLI for converting large map datasets into a compact street-vector standard (`.svm`).

## Product Responsibility
This tool does **not** implement minimap camera behavior.
It converts source maps into georeferenced city-scale vectors that can be consumed by embedded/runtime renderers.

Current scope:
- Extract street-like vectors from MBTiles.
- Preserve GPS-to-vector mapping using shared world coordinates.
- Keep schema ready for future metadata (street name, lane count, tags).
- Use a bike-focused filter profile by default (exclude ferry/boat/water transport lines).
- Emit only the shared `.svm` map format; runtime adapters are responsible for consuming it.

## Commands
Convert whole-city MBTiles to `.svm`:
```bash
cargo run -p map-vector-cli -- \
  convert-mbtiles \
  --input /work/map-src/osm-2020-02-10-v3.11_finland_helsinki.mbtiles \
  --output /work/map-data/city.svm \
  --target-zoom 16 \
  --profile bike
```

## `.svm` Standard Summary
- Magic/versioned binary (`SVM1`).
- Stores city-scale georeferenced street segments.
- Segment schema includes reserved metadata fields for future tags.

See `/work/map-vector-cli/docs/vector-format-options.md` for alternatives and rationale.
