# Phase 4 Riding Mode Plan

## Goal
Deliver a game-like bike minimap experience with clear riding/stopped behavior, controlled zoom scale, stronger visual identity, and smoother interaction performance.

## Scope
- In scope:
  - Runtime camera behavior and state transitions.
  - Marker and vector visual style updates.
  - North indicator UX.
  - Renderer improvements needed for smooth pan/zoom interaction.
- Out of scope:
  - Source map conversion ownership and `.svm` format changes beyond render-facing metadata.
  - Full overview-mode decluttering implementation (design only in this phase).

## Functional Requirements
1. Zoom policy
- Maximum zoom-in should show about 100 m of surrounding context.
- Zoom-out must stop before line overlap makes roads unreadable.

2. Camera modes
- Riding mode:
  - Heading-up orientation.
  - Rider anchored near lower quarter lane (forward look-ahead).
- Stopped mode:
  - Rider centered.
  - After stop delay, map rotates smoothly to north-up.
- Temporary north-up override:
  - Activated from north indicator tap.
  - Auto-expires back to riding mode when movement resumes for configured time.
- Manual pan:
  - Freeze camera follow target at pan start so rider marker does not drift with live position updates.
  - Resume live follow only after pan idle recenter finishes.

3. UI indicator
- Top-right north indicator icon.
- Reflect current orientation/mode and respond to tap.

4. Marker style
- Riding marker: glowing yellow-green forward-facing shape.
- Stopped marker: larger, readable game-map style marker.

5. Vector style
- Dark/navy background with strong major-road emphasis and muted minor roads.
- Preserve circular minimap frame with clear ring boundary.

## Technical Approach
1. Mode state machine
- Add explicit mode enum and transition guard conditions.
- Inputs:
  - Position delta/speed proxy.
  - Heading delta.
  - Idle timers.
  - Manual override flag.

2. Animation model
- Use time-based interpolation for:
  - Camera anchor shift (lower-quarter <-> center).
  - Heading rotation (heading-up <-> north-up).
  - Pan recenter decay and follow-lock release.
- Keep deterministic dt-based stepping for firmware and emulator parity.

3. Rendering performance baseline
- Precompute camera transform constants once per frame.
- Clip visible lines to viewport before raster stepping.
- Enable adaptive quality mode during active touch pan.

## Milestones
1. M1: Camera state model + transitions.
2. M2: Zoom clamps + riding/stopped anchors.
3. M3: North indicator + temporary override logic.
4. M4: Marker and vector visual styling pass.
5. M5: Rendering optimization pass and behavior validation.

## Validation Checklist
- Drag and pinch on mobile browser emulator remain responsive.
- Riding mode keeps rider near lower quarter with stable heading-up.
- Stop transition recenters and rotates to north-up after delay smoothly.
- North indicator tap toggles temporary north-up and auto-returns while moving.
- Zoom-in never exceeds close-context target; zoom-out preserves readability.

## Risks
- Large vector sets can still degrade frame time without spatial indexing.
- Heading noise can cause visual jitter if smoothing thresholds are weak.
- Style updates may reduce readability if contrast is not calibrated per display.

## Follow-up (Phase 5 Candidate)
- Overview mode with dynamic vector filtering/aggregation.
- Runtime spatial index/query path for large `.svm` maps.
