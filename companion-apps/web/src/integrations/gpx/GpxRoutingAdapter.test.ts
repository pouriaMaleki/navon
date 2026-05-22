import { describe, expect, it } from "vitest";
import { GpxRoutingAdapter, parseGpx, slugify } from "./GpxRoutingAdapter.js";

const SAMPLE_GPX = `<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test">
  <metadata><name>Helsinki Loop</name></metadata>
  <rte>
    <name>Helsinki Loop</name>
    <rtept lat="60.1699" lon="24.9384"><name>Start</name></rtept>
    <rtept lat="60.1750" lon="24.9450"><name>Middle</name></rtept>
    <rtept lat="60.1820" lon="24.9300"><name>End</name></rtept>
  </rte>
</gpx>`;

describe("GpxRoutingAdapter", () => {
  it("parses route points and prefers them over tracks", () => {
    const parsed = parseGpx(SAMPLE_GPX);
    expect(parsed.routeName).toBe("Helsinki Loop");
    expect(parsed.points).toHaveLength(3);
    expect(parsed.preferPointLabels).toBe(true);
  });

  it("imports a GPX into a single-alternative preview", () => {
    const adapter = new GpxRoutingAdapter();
    const preview = adapter.importFile("loop.gpx", SAMPLE_GPX);
    expect(preview.alternatives).toHaveLength(1);
    expect(preview.alternatives[0].title).toBe("Helsinki Loop");
    const package_ = preview.alternatives[0].normalizedPackage;
    expect(package_.geometry).toHaveLength(3);
    expect(package_.maneuvers[0].maneuverType).toBe("depart");
    expect(package_.maneuvers[package_.maneuvers.length - 1].maneuverType).toBe("arrive");
  });

  it("falls back to track points when route is empty", () => {
    const trackOnly = `<?xml version="1.0"?><gpx><trk><name>T</name><trkseg>
      <trkpt lat="60.1" lon="24.9"/><trkpt lat="60.2" lon="24.95"/></trkseg></trk></gpx>`;
    const parsed = parseGpx(trackOnly);
    expect(parsed.points).toHaveLength(2);
    expect(parsed.routeName).toBe("T");
    expect(parsed.preferPointLabels).toBe(false);
  });

  it("rejects GPX with too few points", () => {
    expect(() => parseGpx(`<gpx><rte><rtept lat="1" lon="1"/></rte></gpx>`)).toThrow(
      /usable route or track/i,
    );
  });

  it("slugifies titles", () => {
    expect(slugify("Helsinki Loop!")).toBe("helsinki-loop");
    expect(slugify("   ---")).toBe("gpx-import");
  });
});
