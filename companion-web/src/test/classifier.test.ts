import { describe, expect, it } from "vitest";
import { classifyImport } from "../integrations/shareImport/UrlImportClassifier.js";

describe("UrlImportClassifier", () => {
  it("classifies .gpx files by extension", () => {
    const result = classifyImport({ kind: "file", fileName: "loop.gpx", content: "" });
    expect(result.classification).toBe("gpxFile");
  });

  it("classifies content-sniffed gpx", () => {
    const result = classifyImport({ kind: "file", fileName: "loop.bin", content: "<gpx></gpx>" });
    expect(result.classification).toBe("gpxFile");
  });

  it("extracts coordinates from Google Maps @lat,lon URLs", () => {
    const result = classifyImport({
      kind: "url",
      url: "https://www.google.com/maps/place/?q=Helsinki/@60.17,24.94,15z",
    });
    expect(result.classification).toBe("googleMapsLocationLink");
    expect(result.coordinate?.latitude).toBeCloseTo(60.17);
    expect(result.coordinate?.longitude).toBeCloseTo(24.94);
  });

  it("extracts coordinates from query string", () => {
    const result = classifyImport({
      kind: "url",
      url: "https://maps.google.com/?q=60.17,24.94",
    });
    expect(result.classification).toBe("googleMapsLocationLink");
    expect(result.coordinate?.latitude).toBeCloseTo(60.17);
  });

  it("classifies Garmin links separately", () => {
    const result = classifyImport({
      kind: "url",
      url: "https://connect.garmin.com/modern/course/12345",
    });
    expect(result.classification).toBe("garminCourseLink");
  });

  it("extracts plain coordinates from text", () => {
    const result = classifyImport({ kind: "text", text: "60.17, 24.94" });
    expect(result.classification).toBe("coordinatesText");
    expect(result.coordinate?.longitude).toBeCloseTo(24.94);
  });

  it("rejects out-of-range coordinates", () => {
    const result = classifyImport({ kind: "text", text: "lat 91.0, lon 200.0" });
    expect(result.classification).toBe("unknown");
  });
});
