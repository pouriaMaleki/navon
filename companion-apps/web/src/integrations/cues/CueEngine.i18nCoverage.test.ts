import { describe, expect, it } from "vitest";
import { tIn } from "../../i18n/index.js";
import { type CueEvent, cueMessage, type ManeuverKind } from "./CueEngine.js";

const ALL_KINDS: ManeuverKind[] = [
  "left",
  "right",
  "slightLeft",
  "slightRight",
  "exitLeft",
  "exitRight",
  "uturn",
  "roundabout",
  "merge",
  "ramp",
  "generic",
];

const ALL_KIND_PAIRS: [ManeuverKind, ManeuverKind][] = [];
for (const first of ALL_KINDS) {
  for (const second of ALL_KINDS) {
    ALL_KIND_PAIRS.push([first, second]);
  }
}

function enText(event: CueEvent): string {
  const msg = cueMessage(event, "metric");
  return tIn("en", msg.key, msg.values as Record<string, string | number>);
}

/** ICU select fallback values that indicate a missing branch in the i18n catalog. */
const ICU_FALLBACKS =
  /^Continue$|^follow the route$|^ahead$|^Continue then immediately ahead$|^In .*follow the route then quickly ahead$/;

describe("cueMessage coverage — every ManeuverKind produces valid EN text", () => {
  for (const k of ALL_KINDS) {
    it(`turn50m.${k} resolves to non-empty text`, () => {
      const text = enText({ kind: "turn50m", turnKind: k, distanceM: 45 });
      expect(text.length).toBeGreaterThan(0);
      expect(text).not.toContain("{");
    });
  }

  for (const k of ALL_KINDS) {
    it(`turn10m.${k} resolves to non-empty text`, () => {
      const text = enText({ kind: "turn10m", turnKind: k });
      expect(text.length).toBeGreaterThan(0);
      expect(text).not.toContain("{");
    });
  }

  for (const k of ALL_KINDS) {
    it(`nextTurnInAbout.${k} resolves to non-empty text`, () => {
      const text = enText({ kind: "nextTurnInAbout", turnKind: k, distanceM: 200 });
      expect(text.length).toBeGreaterThan(0);
      expect(text).not.toContain("{");
    });
  }
});

describe("cueMessage coverage — combined cues for every kind pair", () => {
  for (const [first, second] of ALL_KIND_PAIRS) {
    it(`turn50mCombined first=${first} second=${second}`, () => {
      const text = enText({
        kind: "turn50m",
        turnKind: first,
        distanceM: 30,
        followUpKind: second,
      });
      expect(text.length).toBeGreaterThan(0);
      // generic is a catch-all — its other branch is intentional
      if (first !== "generic" && second !== "generic") {
        expect(text).not.toMatch(ICU_FALLBACKS);
      }
    });
  }

  for (const [first, second] of ALL_KIND_PAIRS) {
    it(`turn10mCombined first=${first} second=${second}`, () => {
      const text = enText({ kind: "turn10m", turnKind: first, followUpKind: second });
      expect(text.length).toBeGreaterThan(0);
      if (first !== "generic" && second !== "generic") {
        expect(text).not.toMatch(ICU_FALLBACKS);
      }
    });
  }
});

describe("cueMessage coverage — non-directional events", () => {
  it("arrivingInM produces valid text", () => {
    const text = enText({ kind: "arrivingInM", distanceM: 100 });
    expect(text.length).toBeGreaterThan(0);
    expect(text).not.toContain("{");
  });

  it("arrived produces valid text", () => {
    const text = enText({ kind: "arrived" });
    expect(text.length).toBeGreaterThan(0);
  });

  it("offTrack produces valid text", () => {
    const text = enText({ kind: "offTrack" });
    expect(text.length).toBeGreaterThan(0);
  });

  it("rerouting produces valid text", () => {
    const text = enText({ kind: "rerouting" });
    expect(text.length).toBeGreaterThan(0);
  });

  it("onTrack produces valid text", () => {
    const text = enText({ kind: "onTrack" });
    expect(text.length).toBeGreaterThan(0);
  });

  it("repeatedOffTrackSilence produces valid text", () => {
    const text = enText({ kind: "repeatedOffTrackSilence" });
    expect(text.length).toBeGreaterThan(0);
  });
});
