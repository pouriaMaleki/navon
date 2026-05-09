# Vector Map Standard Options

## Requirements
- City-scale street data ingestion from large datasets.
- Georeferenced vectors (GPS-aligned) for runtime camera queries.
- Low parsing/memory cost on constrained devices.
- Forward-compatible metadata fields for future road attributes.

## Option A: GeoJSON
Pros:
- Human-readable and easy to inspect.
- Strong tooling ecosystem.

Cons:
- Large files and expensive parse/allocation path.
- Not ideal for low-power real-time embedded rendering.

## Option B: Runtime Mapbox Vector Tile / Protobuf decode
Pros:
- Standard format in mapping stack.
- Rich metadata support.

Cons:
- Runtime decode complexity too high for ESP32-class targets.
- Carries extra data not needed for minimap rendering loop.

## Option C: FlatBuffers / Cap'n Proto schema
Pros:
- Efficient binary transport and layout.
- Better schema evolution controls than ad-hoc binary.

Cons:
- Extra schema/tooling overhead for this focused use case.
- Still requires custom runtime integration effort.

## Option D: Custom Binary `.svm` (Selected)
Pros:
- Minimal parse cost and predictable memory behavior.
- Stores only required street-vector fields.
- Keeps georeferenced coordinates for runtime camera extraction.
- Includes reserved metadata fields for future tags/lane/name mapping.

Cons:
- Project-owned format requires strict versioning discipline.
- Needs dedicated tooling for deep inspection.

## Decision
Selected: **`.svm`** for current product stage.

Reason:
The project needs city-scale preprocessing on host, then low-overhead runtime consumption on device. A compact georeferenced binary with reserved metadata fields is the most practical bridge between those two constraints.
