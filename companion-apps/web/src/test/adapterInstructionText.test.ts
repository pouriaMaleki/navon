import { describe, expect, it } from "vitest";
import { classifyTurn } from "../integrations/geo.js";

describe("classifyTurn produces valid instruction text for all angle ranges", () => {
  // Edge values at each maneuver band boundary
  const cases: { deg: number; expectedType: string }[] = [
    { deg: 10, expectedType: "null" },
    { deg: 24, expectedType: "null" },
    { deg: 25, expectedType: "slightRight" },
    { deg: 49, expectedType: "slightRight" },
    { deg: -25, expectedType: "slightLeft" },
    { deg: -49, expectedType: "slightLeft" },
    { deg: 50, expectedType: "right" },
    { deg: 109, expectedType: "right" },
    { deg: -80, expectedType: "left" },
    { deg: 110, expectedType: "sharpRight" },
    { deg: 169, expectedType: "sharpRight" },
    { deg: -135, expectedType: "sharpLeft" },
    { deg: 170, expectedType: "uturn" },
    { deg: 180, expectedType: "uturn" },
  ];

  for (const { deg, expectedType } of cases) {
    it(`classifyTurn(${deg > 0 ? "+" : ""}${deg}°) → ${expectedType}`, () => {
      const result = classifyTurn(deg);
      if (expectedType === "null") {
        expect(result).toBeNull();
      } else {
        expect(result).not.toBeNull();
        expect(result!.type).toBe(expectedType);
        expect(result!.instruction.length).toBeGreaterThan(0);
        // Instruction must not contain "Bear" — should be "Slight" for slights
        if (expectedType.startsWith("slight")) {
          expect(result!.instruction).toMatch(/^Slight/);
        }
      }
    });
  }
});
