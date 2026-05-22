import { beforeEach, describe, expect, it } from "vitest";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { LocationStore } from "./LocationStore.js";
import { FakeLocationService } from "../__testlib__/fakes/index.js";
import { loadHelsinkiGravelStream } from "../__testlib__/fixtures/helsinkiGravel.js";
import { loadUxConstants } from "../__testlib__/fixtures/uxConstants.js";

describe("rider lifecycle replay (flow #63)", () => {
  beforeEach(() => {
    globalThis.localStorage?.clear();
  });

  it("replays helsinki-gravel fixture and reaches final position", () => {
    const stream = loadHelsinkiGravelStream();
    expect(stream.length).toBeGreaterThan(100);
    const persistence = new LocalStoragePersistence();
    const fakeLoc = new FakeLocationService();
    fakeLoc.setPermission("granted");
    const location = new LocationStore(fakeLoc, persistence);
    location.start();

    for (const sample of stream) {
      fakeLoc.emitFix(sample.latitude, sample.longitude, {
        accuracyMeters: sample.accuracyM,
      });
    }

    const last = stream[stream.length - 1];
    expect(location.currentLocation).toEqual({
      latitude: last.latitude,
      longitude: last.longitude,
    });
  });

  it("persists last fix so a later instance can read it back (spec: locating spinner fallback)", () => {
    const stream = loadHelsinkiGravelStream();
    const persistence = new LocalStoragePersistence();
    const fakeLoc = new FakeLocationService();
    fakeLoc.setPermission("granted");
    const first = new LocationStore(fakeLoc, persistence);
    first.start();
    fakeLoc.emitFix(stream[10].latitude, stream[10].longitude);
    first.stop();

    const fakeLoc2 = new FakeLocationService();
    const second = new LocationStore(fakeLoc2, persistence);
    expect(second.lastKnownLocation).toEqual({
      latitude: stream[10].latitude,
      longitude: stream[10].longitude,
    });
  });

  it("loads pinned UX constants (spec bridge, flow metadata)", () => {
    const constants = loadUxConstants();
    expect(constants.enterMovingKph).toBe(0.5);
    expect(constants.recentsPageSize).toBe(20);
  });
});
