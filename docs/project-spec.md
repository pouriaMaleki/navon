# Project Specification

## Canonical Description (User-Defined)
The following project overview is preserved as the canonical definition for future agents:

> Over view of project:
>
> ESP32 map renderer in style of video games minimap. We are implementing something similar to minimap repo, but using vectors so we gain high quality perforamnce.
>
> Language is rust for ease of development and performance
>
> In the phase one, we will have only a sample map, which is just a mock small map made of a few vectors, and we try to render that on display in format of a video game map.

## Normalized Summary
- Target platform: ESP32
- Goal: render a video-game-style minimap
- Reference: `minimap/` sample repo (`/tmp/Video_Game_Mini_Maps-fork`)
- Key technical direction: vector-based map rendering for quality/performance benefits
- Language: Rust
- Target display device:
  - Waveshare ESP32-P4-WIFI6-Touch-LCD-XC
  - default panel mode: 800x800 (3.4-inch)
  - compatible mode: 720x720 (4-inch)
- Phase 1 scope:
  - Use a mock/sample map only
  - Sample consists of a small set of vector shapes
  - Render to display in minimap style

## Scope Boundary (Phase 1)
- In scope:
  - minimal rendering pipeline
  - sample vector map data
  - display output in minimap format
- Out of scope:
  - full game map ingestion
  - advanced interactivity/zoom/navigation systems
  - production optimization beyond baseline validation
