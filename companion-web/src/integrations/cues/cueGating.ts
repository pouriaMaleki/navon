// Pure decision function for whether to dispatch audio cues this tick.
// Kept separate from RoutingActivityCoordinator so it can be unit tested
// without driving the MobX autorun. The coordinator's job is to gather
// the inputs (settings, snapshot, page visibility) and feed them here.
//
// Spec line 144: `audioCuesOnlyInBackground` defaults to true; while it
// is on and the page is visible (the rider has the tab focused), cues
// are suppressed because the map is already on-screen.

export type CueGatingInput = {
  isRouting: boolean;
  audioCuesEnabled: boolean;
  allowBackgroundGps: boolean;
  pairedWithDevice: boolean;
  audioCuesOnlyInBackground: boolean;
  isAppInBackground: boolean;
  isExploringAlternativesFromGuidance: boolean;
};

export function shouldDispatchCues(input: CueGatingInput): boolean {
  if (!input.isRouting) return false;
  if (!input.audioCuesEnabled) return false;
  if (!input.allowBackgroundGps) return false;
  if (input.pairedWithDevice) return false;
  if (input.audioCuesOnlyInBackground && !input.isAppInBackground) return false;
  if (input.isExploringAlternativesFromGuidance) return false;
  return true;
}
