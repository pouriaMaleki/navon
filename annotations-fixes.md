# CueEngine Bug Fixes from Annotations

## Context

Five annotations were filed against recorded routing diagnostics sessions. All five are CueEngine bugs
that manifest identically on Web, iOS, and Android (the engine is ported 1:1).

## Bugs

| # | Annotation | Root Cause | Fix |
|---|-----------|-----------|-----|
| 1 | Cue flooding for back-to-back turns | `nextTurnInAbout` fires for follow-up maneuver immediately after combined `turn10m` fires for the lead. The passed-maneuver block doesn't check that the next-after maneuver was already part of a combined cue that fired on this tick. | Add `announcedNextManuever` latch: when a combined cue fires, also pre-latch the `nextTurnInAbout` for the **passed** maneuver (the follow-up's predecessor). The passed-maneuver block checks this before firing `nextTurnInAbout`. |
| 2 | Negative distance in `nextTurnInAbout` ("-10 meters") | `nextAfter.distanceFromStartM - snapshot.progressDistanceM` is negative when the rider has passed two maneuvers in one tick. The passed-maneuver block uses `lastPassed` (most-recently-passed), and `nextAfter` is the maneuver right after it — but the rider may have already passed `nextAfter` too. | Guard: only emit `nextTurnInAbout` when `distanceToNext > 0`. If ≤ 0, skip and fall through to `arrivingInM` logic. |
| 3 | Audio cue fires after `routeStopped` | `declareArrival` sets `arrivalNotice` then calls `stopActiveNavigation`, which calls `dispatchCueTick()` one final time. That tick has a snapshot where `arrived = true` but the maneuver cues don't check `arrived`. The `arrived` guard only protects `arrivingInM` (Bug 4 fix), not `turn10m`. | Gate the entire maneuver-cue block on `!snapshot.arrived`, not just the `arrivingInM` sub-paths. |
| 4 | Confusing `bearRange` distance (250m vs nearby turn) | NOT A BUG. `bearRange.distanceM` = segment length (bear maneuver start → next maneuver start), not distance-from-rider. The 11m is distance TO the bear entrance; 250m is the bear segment LENGTH. The cue says "Bear right for the next 250 meters" — this is correct. | No code change. |

## Test Cases (all platforms)

### Test: No negative `nextTurnInAbout` distance
- Setup: Snapshot where rider has passed two maneuvers (progress > maneuver2.distance + 10m)
- Expect: No `nextTurnInAbout` event emitted with negative distanceM

### Test: No cue flooding for back-to-back combined + next-turn
- Setup: Two maneuvers within 30m, rider < 15m from first
- Expect: At most one audio cue per tick; `nextTurnInAbout` for follow-up NOT emitted on same tick as combined `turn10m`

### Test: No maneuver cues when arrived
- Setup: Snapshot with `arrived = true`, rider 8m from a turn
- Expect: No `turn50m`, `turn10m`, `nextTurnInAbout`, or `bearRange` events
- But: `arrived` event should still fire

### Test: Cue latches prevent duplicate cues
- Setup: Two consecutive ticks with same snapshot except progress
- Expect: Each maneuver produces at most one `turn10m` and one `turn50m`

### Test: Back-to-back combined cue properly suppresses individual cues
- Setup: Two maneuvers within 30m, rider approaching first
- Expect: Combined cue fires; NO individual `turn50m`/`turn10m` for follow-up maneuver

## Implementation Order

1. Write failing tests on Web (Jest in /work/companion-apps/web/src/test/)
2. Fix Web CueEngine.ts
3. Verify Web tests pass
4. Port identical fix to iOS CueEngine.swift
5. Port identical fix to Android CueEngine.kt

## Files to Modify

- **Web tests**: `/work/companion-apps/web/src/test/` (new or existing cue test file)
- **Web engine**: `/work/companion-apps/web/src/integrations/cues/CueEngine.ts`
- **iOS engine**: `/work/companion-apps/ios/CompanionApp/Integration/Cues/CueEngine.swift`
- **Android engine**: `/work/companion-apps/android/app/src/main/java/app/navon/bike/integration/cues/CueEngine.kt`
