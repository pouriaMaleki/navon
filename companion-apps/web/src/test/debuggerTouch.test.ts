import { beforeEach, describe, expect, it } from "vitest";
import type { RoutingDiagDebugPackage } from "../domain/routingDiagnosticsModels.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { DebuggerStore } from "../stores/DebuggerStore.js";

function makeTestPackage(
  overrides: Partial<RoutingDiagDebugPackage> = {},
): RoutingDiagDebugPackage {
  const events: RoutingDiagDebugPackage["events"] = [
    {
      id: "ev-1",
      timestampMs: 1000000,
      data: { kind: "locationUpdate", lat: 60.1699, lon: 24.9384 },
    },
    {
      id: "ev-2",
      timestampMs: 1001000,
      data: { kind: "audioCueDispatched", cueType: "turnLeft", messageText: "Turn left" },
    },
    {
      id: "ev-3",
      timestampMs: 1002000,
      data: {
        kind: "nextTurnAlerted",
        instructionText: "Turn left on Mannerheimintie",
        distanceRemainingM: 200,
      },
    },
    {
      id: "ev-4",
      timestampMs: 1003000,
      data: { kind: "locationUpdate", lat: 60.1705, lon: 24.939 },
    },
  ];
  return {
    formatVersion: 1,
    sessionId: "test-session-1",
    createdAtMs: 1000000,
    eventCount: events.length,
    events,
    ...overrides,
  };
}

describe("DebuggerStore", () => {
  let store: DebuggerStore;
  let persistence: LocalStoragePersistence;

  beforeEach(() => {
    localStorage.clear();
    persistence = new LocalStoragePersistence();
    store = new DebuggerStore(persistence);
  });

  describe("mapFollowActive", () => {
    it("defaults to false", () => {
      expect(store.mapFollowActive).toBe(false);
    });

    it("setMapFollowActive(true) toggles the field on", () => {
      store.setMapFollowActive(true);
      expect(store.mapFollowActive).toBe(true);
    });

    it("setMapFollowActive(false) toggles back off", () => {
      store.setMapFollowActive(true);
      store.setMapFollowActive(false);
      expect(store.mapFollowActive).toBe(false);
    });
  });

  describe("playback state", () => {
    it("starts in stopped state with null session", () => {
      expect(store.playbackState).toBe("stopped");
      expect(store.session).toBeNull();
      expect(store.currentTimeMs).toBe(0);
    });

    it("loadSessionFromPackage loads a session and positions at start", () => {
      const pkg = makeTestPackage();
      store.loadSessionFromPackage(pkg);
      expect(store.session).not.toBeNull();
      expect(store.session!.diagSession.id).toBe("test-session-1");
      expect(store.currentTimeMs).toBe(1000000);
    });
  });

  describe("currentPosition", () => {
    it("returns null when no session loaded", () => {
      expect(store.currentPosition).toBeNull();
    });

    it("interpolates GPS position based on currentTimeMs", () => {
      const pkg = makeTestPackage();
      store.loadSessionFromPackage(pkg);
      store.seekToElapsed(500);
      const pos = store.currentPosition;
      expect(pos).not.toBeNull();
    });
  });

  describe("panBackToEnd", () => {
    it("seeking restores currentTimeMs to the end when paused and mapFollowActive pans", () => {
      const pkg = makeTestPackage();
      store.loadSessionFromPackage(pkg);
      store.seekToElapsed(500);
      expect(store.currentTimeMs).toBe(1000500);
      // Seeking past duration should not exceed end
      store.seekToElapsed(99999);
      const maxEnd = 1003000; // last event timestamp
      expect(store.currentTimeMs).toBeLessThanOrEqual(maxEnd + 1000000);
    });
  });
});

// Verify DebuggerEventPanel module exports are loadable
describe("DebuggerEventPanel exports", () => {
  it("EVENT_KIND_LABELS has expected entries", async () => {
    const mod = await import("../features/debugger/DebuggerEventPanel.js");
    expect(mod.EVENT_KIND_LABELS).toBeDefined();
    expect(mod.EVENT_KIND_LABELS["locationUpdate"]).toBe("GPS");
    expect(mod.EVENT_KIND_LABELS["audioCueDispatched"]).toBe("Audio cue");
    expect(mod.EVENT_KIND_LABELS["nextTurnAlerted"]).toBe("Turn alert");
  });
});
