import { describe, expect, it } from "vitest";
import {
  approximateDistanceMeters,
  classifyTurn,
  decodePolyline,
  deduplicateConsecutive,
} from "../integrations/geo.js";

describe("geo", () => {
  it("approximates distance meters for nearby points", () => {
    const a = { latitude: 60.1699, longitude: 24.9384 };
    const b = { latitude: 60.1709, longitude: 24.9384 };
    const distance = approximateDistanceMeters(a, b);
    expect(distance).toBeGreaterThan(105);
    expect(distance).toBeLessThan(120);
  });

  it("decodes a known Google polyline", () => {
    const points = decodePolyline("_p~iF~ps|U_ulLnnqC_mqNvxq`@");
    expect(points).toHaveLength(3);
    expect(points[0].latitude).toBeCloseTo(38.5, 1);
    expect(points[0].longitude).toBeCloseTo(-120.2, 1);
  });

  it("deduplicates consecutive duplicates only", () => {
    const result = deduplicateConsecutive([
      { latitude: 1, longitude: 1 },
      { latitude: 1, longitude: 1 },
      { latitude: 2, longitude: 2 },
      { latitude: 1, longitude: 1 },
    ]);
    expect(result).toHaveLength(3);
  });

  it("classifies turn magnitudes per the iOS spec", () => {
    expect(classifyTurn(10)).toBeNull();
    expect(classifyTurn(30)?.type).toBe("slightRight");
    expect(classifyTurn(-30)?.type).toBe("slightLeft");
    expect(classifyTurn(80)?.type).toBe("right");
    expect(classifyTurn(-80)?.type).toBe("left");
    expect(classifyTurn(120)?.type).toBe("sharpRight");
    expect(classifyTurn(-120)?.type).toBe("sharpLeft");
    expect(classifyTurn(175)?.type).toBe("uturn");
  });
});
