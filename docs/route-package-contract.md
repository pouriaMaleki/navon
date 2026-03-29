# Route Package Contract

This document defines the normalized routing contract shared by companion, adapters, and `runtime-core`.

## Goals
- Keep provider-specific parsing outside shared runtime logic.
- Guarantee deterministic validation and compatibility handling on device/emulator.
- Support HSL, Google ingest, OSM, GPX/FIT/TCX, and Garmin sources through one normalized shape.

## Schema
The canonical contract is represented by `runtime_core::api::RoutePackage` and related types:
- `RoutePackageVersion`
- `RoutePackage`
- `GeoPoint`
- `RouteManeuver` and `RouteManeuverType`
- `RouteSummary`
- `RouteProvenance` and `RouteProvider`
- `RouteSyncMessage` (`Set`, `Update`, `Clear`, `Status`, `RerouteRequest`)

## Required Invariants
`RoutePackage::validate()` enforces:
- version compatibility must pass current runtime policy
- `route_id` must be non-empty after trimming
- `geometry` must contain at least 2 points
- all geometry points must be finite and within valid lat/lon ranges
- `summary.total_distance_m` must be finite and non-negative
- maneuver locations must be valid geo points
- maneuver distances from start must be non-negative, finite, and not exceed route distance (with small tolerance)
- maneuver ordering must be non-decreasing by `distance_from_start_m`

## Compatibility Policy
Current version constant:
- `CURRENT_ROUTE_PACKAGE_VERSION = 1.0`

Compatibility rule:
- major version must match exactly
- incoming minor version must be less than or equal to runtime minor version

Interpretation:
- minor versions are backward-compatible additions only
- major changes are breaking and require explicit migration

Migration expectation for major bumps:
1. companion normalizer emits both old and new payloads during migration window, or
2. adapters/runtime include an explicit migration layer before route activation

## Normalization Guarantees
Companion routing orchestration must guarantee that normalized `RoutePackage` payloads:
- do not require provider-conditional runtime logic
- use canonical `RouteManeuverType` values
- provide provenance identifying source provider and source reference
- preserve deterministic route identity (`route_id`) and monotonic revisions

## Provider Fixture Coverage
Canonical provider fixtures are validated in:
- `runtime-core/tests/route_contract.rs`

Fixture set includes normalized examples for:
- HSL Digitransit
- Google ingest
- OSM
- GPX
- FIT
- TCX
- Garmin API
- Garmin file import
