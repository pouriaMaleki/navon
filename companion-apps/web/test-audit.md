# companion-web test audit

Mapping of the original 35 vitest tests to the new flow-numbered taxonomy.
The "original" tests are kept under `src/test/` as narrower unit companions to
the flow tests under `src/test/flows/`. Nothing was deleted.

## Original suite → flow mapping

| File | Tests | Maps to flow(s) | Disposition |
|---|---|---|---|
| `classifier.test.ts` | 7 | URL paste import path; pre-dates plan flow #29 | keep — covers URL classification edge cases not in flow #29 |
| `geo.test.ts` | 4 | Spec geo helpers (distance, bearing, polyline decode, turn class) | keep — used by every routing test |
| `gpx.test.ts` | 5 | GPX import path; pre-dates the plan | keep — covers a code path the flow tests don't exercise |
| `locationStore.test.ts` | 8 | Used by flow #15 (loading state) and flow #63 (rider lifecycle) | keep — narrower than flow tests; tests the LocationStore in isolation |
| `persistence.test.ts` | 4 | Used by flow #22 (recents dedupe) | keep — focused on persistence merging |
| `planning.test.ts` | 2 | Mixed-mode merging used by flows #38–41 | keep — tests `mergeMixedAlternatives` directly |
| `urlExpander.test.ts` | 5 | Used by flows #29–31 (URL paste) | keep — tests the expander parser; flows #29-31 test the store wrapper |

## Flow tests added

| File | Plan flows |
|---|---|
| `flows/sourceSelection.test.ts` | #38, #39, #41 |
| `flows/whereTo.test.ts` | #20, #25–27, #35–37, #23–24 |
| `flows/urlPaste.test.ts` | #29–31 |
| `flows/cameraModes.test.ts` | #44, #45, #49, #52, #54 (skipped — L3) |
| `flows/routingSession.test.ts` | #43, #44, #61, #62 |
| `flows/riderLifecycle.test.ts` | #15 (locating), #63 (full ride), pinned-constants bridge |

## Playwright specs

| File | Plan flow |
|---|---|
| `e2e/specs/rowHitArea.spec.ts` | #32 |
| `e2e/specs/outsideTapDismiss.spec.ts` | #33 |
| `e2e/specs/escapeDismiss.spec.ts` | #34 (web variant) |
| `e2e/specs/longPressDestination.spec.ts` | #46 |
| `e2e/specs/gestureHandlerReached.spec.ts` | #1 invariant only (test handler output, not gesture semantics) |

## Out of scope on web (platform-specific)

Only flows that have no web UI surface:

- #34 back-button dismiss (Android only — hardware/gesture Back).
- #34 IME Done dismiss (Android — web uses Escape, covered separately).

## Named-contract REDs (spec-asserted, code change required to turn green)

- #50 / #51 stationary & moving recenter on companion — asserted via the
  `MapCameraStore.riderAnchorNormalizedY` contract in `cameraModes.test.ts`.
- #52 companion north-indicator single-tap recenter — asserted via
  `GuidanceStore.onRecenterRequested(callback)` in `cameraModes.test.ts`.
- #23 recents pagination (load-more gated on last-item) — asserted via
  `loadMoreRecentsIfNeeded("non-last-id")` being a no-op in `whereTo.test.ts`.
- #24 recents pagination loading indicator — asserted via
  `PlanningStore.isLoadingMoreRecents` in `whereTo.test.ts`.
- #25 typeahead debounce — asserted via call count after rapid keystrokes in
  `whereTo.test.ts`.
- #26 typeahead loading indicator — asserted via `isTypeaheadSearching` in
  `whereTo.test.ts`.
- #27 typeahead area bias — asserted via `FakePlaceSearch.lastQueryBias`
  being populated after a search in `whereTo.test.ts`.

## How to run

```bash
cd companion-web
npm run test            # vitest unit + flow tests
npm run test:e2e        # Playwright (requires `npm i -D @playwright/test` first)
npm run test:all        # both
```
