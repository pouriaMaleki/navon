import { describe, expect, it } from "vitest";
import { extractCoordinateFromText, looksLikeUrl } from "./UrlExpander.js";

describe("UrlExpander", () => {
  it("extracts inline @lat,lng from a Google Maps URL", () => {
    const point = extractCoordinateFromText(
      "https://www.google.com/maps/place/.../@60.17,24.94,15z",
    );
    expect(point).toEqual({ latitude: 60.17, longitude: 24.94 });
  });

  it("extracts !3d!4d from a deep Google Maps URL", () => {
    const point = extractCoordinateFromText(
      "https://www.google.com/maps/place/.../data=!3m1!4b1!4m6!3d38.71!4d-9.13",
    );
    expect(point).toEqual({ latitude: 38.71, longitude: -9.13 });
  });

  it("extracts URL-encoded query coords (q=lat%2C+lng) from a proxy response body", () => {
    // The exact form Google Maps redirects produce when there is no place page,
    // as observed via api.allorigins.win on a maps.app.goo.gl link.
    const body = `<link href="/search?tbm=map&q=38.733385%2C+-9.147573&pb=..." as="fetch">`;
    const point = extractCoordinateFromText(body);
    expect(point?.latitude).toBeCloseTo(38.733385);
    expect(point?.longitude).toBeCloseTo(-9.147573);
  });

  it("rejects out-of-range coordinates", () => {
    expect(extractCoordinateFromText("@200,300")).toBeUndefined();
  });

  it("looksLikeUrl recognizes http(s) inputs", () => {
    expect(looksLikeUrl("https://maps.app.goo.gl/abc")).toBe(true);
    expect(looksLikeUrl("not a url")).toBe(false);
  });
});
