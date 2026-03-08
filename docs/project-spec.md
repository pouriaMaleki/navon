# Project Specification

## Product Definition
ESP32 bike minimap renderer in a video-game style UI.

## Main Runtime Behavior
- User location is centered in follow mode.
- Map orientation follows movement direction (heading-up).
- User can pinch to zoom.
- User can temporarily pan to inspect nearby streets.
- System smoothly recenters after pan idle timeout.

## Architecture Separation
- Main ESP32 project (`/work`): runtime camera/render/input behavior.
- Map conversion project (`/work/map-vector-cli`): city-scale source conversion and `.svm` format.

## Data Flow
- Source maps: `/work/map-src`
- Converted maps: `/work/map-data/city.svm`
- Current bridge for renderer integration: generated Rust map module.
- Target direction: direct `.svm` runtime loading in firmware.

## Current Phase Notes
- Shared camera transform supports heading-up, zoom, and pan.
- Emulator uses browser geolocation and touch/pointer gestures.
- Firmware has no_std GPS/touch behavior scaffold for hardware integration phase.
