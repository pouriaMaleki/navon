const DOUBLE_TAP_WINDOW_MS = 320;

let installed = false;

export function installMobileGestureGuards(): void {
  if (installed || typeof window === "undefined" || typeof document === "undefined") {
    return;
  }
  installed = true;

  let lastTouchEndMs = 0;

  const preventNativeGesture = (event: Event): void => {
    event.preventDefault();
  };

  const preventDoubleTapZoom = (event: TouchEvent): void => {
    const activeTouches = event.touches.length;
    const changedTouches = event.changedTouches.length;
    if (activeTouches > 0 || changedTouches !== 1) {
      lastTouchEndMs = 0;
      return;
    }

    const nowMs = window.performance.now();
    if (nowMs - lastTouchEndMs < DOUBLE_TAP_WINDOW_MS) {
      event.preventDefault();
      lastTouchEndMs = 0;
      return;
    }
    lastTouchEndMs = nowMs;
  };

  document.addEventListener("gesturestart", preventNativeGesture, { passive: false });
  document.addEventListener("gesturechange", preventNativeGesture, { passive: false });
  document.addEventListener("gestureend", preventNativeGesture, { passive: false });
  document.addEventListener("touchend", preventDoubleTapZoom, { passive: false });
}
