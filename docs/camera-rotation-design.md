# Camera Rotation UX Design

This document is the canonical camera-orientation UX specification for the bike minimap.
Implementation planning and code should reference this file instead of redefining the behavior in multiple places.

## Summary
- Shared Rust owns all orientation behavior for firmware and emulator.
- The camera uses two top-level motion modes:
  - `Stopped`
  - `Riding`
- The orientation UX is expressed through five user-visible substates:
  - `Stopped North-Up`
  - `Heading Acquisition`
  - `Travel-Up Auto`
  - `North Preview`
  - `North Locked`
- Compass feedback is icon-only by default. The compass visual state must distinguish auto-follow, temporary preview, and locked north-up without adding a text badge in the normal product UI.

## Orientation States

### Stopped North-Up
- Rider is centered.
- Map is north-up.
- This is the default stationary presentation.
- When the rider stops after travel-up motion, the camera holds the last trusted travel angle briefly, then settles smoothly back to north-up.

### Heading Acquisition
- Entered when speed/motion suggests riding has started but travel direction is not yet trustworthy enough for travel-up rotation.
- Rider remains centered.
- The map holds the last trusted angle instead of following noisy raw GPS course data.
- From a stationary start, this means the map stays north-up until heading confidence is good enough to transition.

### Travel-Up Auto
- Entered only when movement is steady enough and heading confidence is good.
- Rider anchor moves to the lower quarter of the screen for forward look-ahead.
- The camera rotates smoothly so the trusted direction of travel points toward the top of the screen.
- Small GPS wiggles should not cause visible camera twitching.

### North Preview
- Triggered by a single tap on the north indicator while `Travel-Up Auto` is active.
- The camera animates to centered north-up.
- This mode is temporary and is meant for quick re-orientation, not a permanent browsing mode.
- The compass should show an active temporary state, including a visible countdown/progress treatment.

### North Locked
- Triggered by a double tap on the north indicator while moving, or by double tapping during `North Preview`.
- The camera stays centered and north-up even while movement continues.
- This mode persists until the user taps the north indicator again to unlock it.
- The compass should show a persistent locked state without a temporary countdown treatment.

## State Transitions
- `Stopped North-Up -> Heading Acquisition`:
  - movement is detected, but heading is not yet trusted
- `Heading Acquisition -> Travel-Up Auto`:
  - heading confidence becomes stable enough for smooth travel-up rotation
- `Travel-Up Auto -> North Preview`:
  - single tap on the north indicator
- `Travel-Up Auto -> North Locked`:
  - double tap on the north indicator
- `North Preview -> Travel-Up Auto`:
  - preview timeout expires and heading confidence is good enough to resume travel-up
- `North Preview -> North Locked`:
  - second tap within the double-tap window
- `North Locked -> Travel-Up Auto`:
  - tap the north indicator again to unlock while heading confidence is good
- `Travel-Up Auto -> Heading Acquisition`:
  - motion continues but heading confidence becomes temporarily weak; hold the last trusted angle rather than snapping north or following jitter
- `Riding states -> Stopped North-Up`:
  - speed drops below stopped thresholds; hold last trusted angle briefly, then settle to north-up

## Rider Anchor Policy
- `Stopped North-Up`: centered
- `Heading Acquisition`: centered
- `Travel-Up Auto`: lower quarter
- `North Preview`: centered
- `North Locked`: centered

Only confident travel-up auto mode uses the lower-quarter look-ahead anchor.
Moving north-up modes remain centered so they read like a conventional map view.

## Heading Confidence And Low-Confidence Behavior
- Camera rotation must prefer trusted travel heading derived from stable movement, not raw GPS course alone.
- If motion is too small or noisy:
  - keep the last trusted camera angle
  - do not snap to a new angle
  - do not snap back to north-up just because heading confidence dipped briefly
- While riding with weak heading confidence, the system should stay visually calm and wait for heading confidence to recover before resuming travel-up rotation.
- The rider marker may still show travel direction while the map itself stays steady in north-up modes.

## Compass Interaction Rules
- Single tap:
  - from `Travel-Up Auto`, enter `North Preview`
  - from `North Locked`, unlock and return to auto-follow when heading confidence allows
- Double tap:
  - from `Travel-Up Auto`, enter `North Locked`
  - from `North Preview`, convert the temporary preview into `North Locked`
- In stopped or heading-acquisition states, tapping the compass should not create a separate mode change beyond a brief acknowledgement, because the map is already north-up.

## Gesture Policy
- One-finger pan: allowed in all states
- Pinch zoom: allowed in all states
- Two-finger rotate: allowed only in `Travel-Up Auto`
- During manual pan:
  - rider position stays anchored to the actual GPS location in map space
  - only camera offset changes until recenter completes
- During `North Preview`, the preview countdown pauses while touch interaction is active
- After pan idle timeout and recenter, the camera returns to the orientation state that should currently apply

## Timing Defaults
- Heading acquisition hold before travel-up is trusted: about `0.8 s`
- North preview timeout: about `2.5 s`
- Double-tap recognition window: about `400 ms`
- Stop hold before north-up settle begins: about `600 ms`
- Stop settle duration back to north-up: about `900 ms`

These are product defaults for implementation planning. Final config fields and exact thresholds can be tuned during runtime implementation.

## Validation Scenarios
- Start moving from rest with noisy GPS:
  - remain centered and stable first
  - only transition into lower-quarter travel-up once heading is trustworthy
- Ride straight:
  - map stays visually calm
  - minor left/right GPS wiggles do not cause camera twitch
- Make a real turn while riding:
  - map rotates smoothly toward the new travel direction
  - rotation should lag slightly for stability, but not feel unresponsive
- Single tap north indicator while riding:
  - map animates to centered north-up preview
  - preview auto-returns only after timeout and adequate heading confidence
- Double tap north indicator while riding:
  - north-up remains pinned during continued movement until explicitly unlocked
- Drop heading confidence while still moving:
  - hold the last trusted angle
  - do not snap to raw course jitter
- Stop after riding:
  - preserve heading briefly
  - smoothly settle back to centered north-up
- Pan and pinch in any state:
  - browsing works without losing rider/map relationship
  - recenter restores the appropriate orientation mode cleanly

## Future Planning Notes
- Runtime output will likely need an orientation substate beyond the current coarse `riding` / `stopped` split.
- Overlay output will likely need explicit compass visual states for:
  - auto-follow
  - temporary north preview
  - locked north-up
- Emulator diagnostics should expose heading-confidence or heading-ready status so "moving but not rotating yet" is easy to validate during development.
