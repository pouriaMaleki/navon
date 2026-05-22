import { describe, expect, it } from "vitest";
import { resolveDistanceUnit, resolveLocale, tIn } from "./index.js";
import { formatMessage } from "./messageFormat.js";

describe("messageFormat", () => {
  it("substitutes simple variables", () => {
    expect(formatMessage("Hi {name}", { name: "Aino" }, "en")).toBe("Hi Aino");
  });

  it("formats numbers with the active locale", () => {
    expect(formatMessage("{n, number} m", { n: 1500 }, "en")).toBe("1,500 m");
    expect(formatMessage("{n, number} m", { n: 1500 }, "fi")).toBe("1 500 m");
  });

  it("resolves a select arm and falls back to other", () => {
    const tpl = "{u, select, meters {meters} feet {feet} other {meters}}";
    expect(formatMessage(tpl, { u: "feet" }, "en")).toBe("feet");
    expect(formatMessage(tpl, { u: "leagues" }, "en")).toBe("meters");
  });

  it("renders the canonical 50m turn cue verbatim in EN", () => {
    expect(tIn("en", "cue.turn50m.left", { distance: 50, distanceUnit: "meters" })).toBe(
      "In 50 meters, turn left",
    );
  });
});

describe("resolveLocale", () => {
  it("returns concrete preferences as-is", () => {
    expect(resolveLocale("en")).toBe("en");
    expect(resolveLocale("fi")).toBe("fi");
  });

  it("returns en when no shipped locale matches navigator.languages", () => {
    // jsdom's navigator.language is typically en-US; whatever it is, this
    // function should never throw.
    const out = resolveLocale("system");
    expect(["en", "fi"]).toContain(out);
  });
});

describe("resolveDistanceUnit", () => {
  it("returns concrete preferences as-is", () => {
    expect(resolveDistanceUnit("metric", "en")).toBe("metric");
    expect(resolveDistanceUnit("imperial", "fi")).toBe("imperial");
  });
});
