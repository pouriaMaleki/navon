/**
 * Distinguishes user gestures from MapLibre's echoes of our own programmatic
 * camera animations. MapLibre fires `dragstart`/`zoomstart`/`rotatestart`/
 * `pitchstart` for `easeTo`/`fitBounds` too — without this gate, the
 * MapSurface schedules `noteUserMapInteraction` on every programmatic move
 * and the compass-lock's fit-route animation snaps the camera back to
 * follow-rider ~1.3s later (the '🧭 reverts after 1.3s' regression).
 *
 * Mirrors the `lastProgrammaticCameraSetAt` quiet-window pattern used on
 * iOS's CompanionHomeView.
 */
export class MapInteractionGate {
  private lastProgrammaticAtMs = Number.NEGATIVE_INFINITY;

  constructor(private readonly quietWindowMs: number) {}

  recordProgrammaticMove(nowMs: number): void {
    this.lastProgrammaticAtMs = nowMs;
  }

  isLikelyUserEvent(nowMs: number): boolean {
    return nowMs - this.lastProgrammaticAtMs > this.quietWindowMs;
  }
}
