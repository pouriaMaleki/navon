# Overview Mode Design

## Purpose
Define the future map overview mode that becomes active when zooming far enough out that dense vector overlap reduces readability.

## Problem
The current renderer intentionally clamps zoom-out before roads fully collapse into each other. That protects readability, but it also limits broader route context. A separate overview mode should allow larger area coverage by intentionally reducing visual density instead of drawing every line with the same policy.

## Trigger
- Candidate trigger: enter overview mode when zoom would otherwise exceed the current readability-safe zoom-out clamp.
- Exit overview mode when zoom returns above that threshold with a small hysteresis gap to avoid mode flicker.

## Behavior
- Preserve camera ownership in shared Rust core.
- Keep north-up / riding-mode behavior consistent with the existing camera controller.
- Switch rendering policy, not navigation policy.

## Rendering Strategy
1. Road hierarchy filtering
- Keep major roads and critical connectors visible at all overview levels.
- Fade or suppress minor residential/service paths first.

2. Density windowing
- Divide world space into coarse cells.
- Limit the number of minor segments rendered per cell.

3. Style shift
- Increase contrast between major and minor classes.
- Reduce stroke width for suppressed-detail layers before dropping them entirely.

4. Water/building handling
- Do not add decorative layers that compete with roads in overview mode.
- If future non-road layers exist, they should degrade earlier than major roads.

## Data Requirements
- Converter output should eventually carry enough road-class information to distinguish major/minor hierarchy directly in runtime data.
- Until then, overview mode should remain a design target, not a half-implemented runtime heuristic.

## UX Rules
- Transition into overview mode must be smooth and not look like a hard scene swap.
- User marker remains readable and visually dominant.
- North indicator and interaction affordances remain unchanged.

## Non-Goals
- This is not a full route-planning mode.
- This does not change GPS-follow, pan recenter, or heading ownership.
- This does not move source-map conversion logic into firmware runtime.
