# Camera Rotation Design

## Goal
Make riding-mode camera rotation behave like turn-by-turn navigation:
- direction of travel stays at the top of the screen
- heading changes feel smooth and stable
- short-lived jitter in motion data does not whip the map around

## Coordinate Conventions
- World `+Y` is north/up in map space.
- World `+X` is east/right in map space.
- Travel heading uses navigation convention:
  - `0 rad` = north/up
  - `+pi/2` = east/right
  - `-pi/2` = west/left
- `map_heading_rad` is the world rotation applied by camera transform.

Important:
- To make travel direction appear at the top of the screen, the camera must rotate the map by the negative of travel heading.
- Therefore:
  - `desired_map_heading_rad = -travel_heading_rad`

## Problems In Previous Model
- It used raw per-frame player delta too directly, which makes heading react to tiny direction changes.
- It used the wrong sign for map rotation in riding mode.
- It did not explicitly separate:
  - travel heading estimation
  - desired map heading
  - smoothed camera heading

## New Model
1. Estimate travel vector from player motion delta while moving.
2. Low-pass filter that vector over time.
3. Convert filtered vector to `travel_heading_rad`.
4. Convert travel heading to desired camera/map heading using negative sign:
   - `desired_map_heading_rad = -travel_heading_rad`
5. Apply manual rotate gesture offset in map-heading space.
6. Move current camera heading toward target with angular slew limiting and smooth blending.

## Travel Heading Estimation
- Only trust motion deltas once movement exceeds minimum threshold.
- When movement is too small/noisy:
  - keep previous stable travel heading
  - do not snap to a new value
- While moving:
  - blend filtered travel vector toward latest motion vector
  - derive heading from filtered vector

## Smoothing Strategy
- Vector smoothing reduces jitter from noisy position changes.
- Angular slew limiting caps how fast the map may rotate per second.
- Final heading blend keeps motion visually continuous.

This is intentionally a two-stage smoothing model:
- vector filtering handles noisy input
- angle slew/blend handles presentation

## Riding / Stopped Behavior
- Riding mode:
  - target map heading follows `-travel_heading_rad`
  - player marker heading still uses positive travel heading
- Stopped mode:
  - target map heading returns to `0 rad` (north-up)
- Temporary north-up mode:
  - target map heading is `0 rad`

## Ownership Boundary
- Emulator/browser may provide GPS position, speed, and heading/course input.
- Shared Rust runtime owns:
  - travel heading stabilization
  - map-heading target calculation
  - camera smoothing behavior

## Current Runtime Behavior
- Travel heading source is currently filtered movement delta in shared Rust camera controller, not raw simulator heading.
- Movement extraction currently uses quantized map-point deltas, then applies stabilization:
  - movement delta threshold (`MOVING_DELTA_THRESHOLD`)
  - travel-vector blend window (`TRAVEL_VECTOR_BLEND_MS`)
  - heading slew limiter (`HEADING_SLEW_RAD_PER_SEC`)
- Practical effect:
  - when frame-to-frame map-point deltas are small or direction changes quickly, visual camera rotation can lag even while simulator heading already changes.
  - this is expected current behavior, not an emulator-only camera policy.

## How To Interpret Logs
- `[emu:bike:physics] headingDeg` and `turnRateDegPerSec` show simulator bike-state turning.
- Camera/map rotation shown on screen reflects shared Rust controller output after movement-vector filtering and heading slew.
- Expected scenario under current behavior:
  - bike logs show immediate heading change after turn input,
  - visible camera rotation may follow with smoothing delay.

## Validation Expectations
- Riding east should rotate the map so east is up and north indicator points left.
- Riding west should rotate the map so west is up and north indicator points right.
- Quick left/right wiggles should not cause fast camera twitching.
- Stopping should preserve current heading briefly, then smoothly return to north-up.
