import type { CoordinatePoint } from "../../domain/models.js";

/**
 * Browsers can't follow `maps.app.goo.gl` redirects directly because Google
 * doesn't set CORS headers on those responses. We use a small fallback chain
 * of public proxies that fetch the URL server-side, follow redirects, and
 * return text we can scrape for a coordinate.
 *
 * - r.jina.ai converts pages to LLM-friendly markdown (good for `<title>`,
 *   keeps the canonical URL on a `URL Source:` line, but strips inline JSON
 *   and script tags so the `geo` JSON-LD blob is lost).
 * - api.allorigins.win returns the raw HTML (which keeps `<script>` tags and
 *   the `!3d!4d` patterns embedded in `<link rel="canonical" ...>`).
 *
 * We try both, accumulate the text, and run every regex over the union, so
 * the first proxy that surfaces a coordinate wins.
 */
const PROXIES: ((url: string) => string)[] = [
  (url) => `https://r.jina.ai/${url}`,
  (url) => `https://api.allorigins.win/raw?url=${encodeURIComponent(url)}`,
];

const COORD_AT = /@(-?\d{1,3}(?:\.\d+)?),(-?\d{1,3}(?:\.\d+)?)/;
// Google Maps query params can come URL-encoded with `%2C` for comma and `+`/`%20` for spaces
// (e.g. q=38.733385%2C+-9.147573). Match raw and encoded variants.
const COORD_QUERY =
  /[?&](?:q|destination|ll|sll|center|near|daddr)=(-?\d{1,3}(?:\.\d+)?)(?:,|%2C)(?:\s|\+|%20)*(-?\d{1,3}(?:\.\d+)?)/i;
const DAATA_LL = /!3d(-?\d{1,3}(?:\.\d+)?)!4d(-?\d{1,3}(?:\.\d+)?)/;
const PROPERTY_GEO =
  /"geo":\s*\{[^}]*?"lat(?:itude)?":\s*(-?\d+\.?\d*)[^}]*?"l(?:on|ng)(?:itude)?":\s*(-?\d+\.?\d*)/;
// Google's place pages put a `dms` / decimal coordinate inside an itemprop="name" attribute when
// the destination resolves to coordinates only (e.g. <meta itemprop="name" content="38.7, -9.1">).
const META_COORD = /content=["'](-?\d{1,3}(?:\.\d+)?)\s*,\s*(-?\d{1,3}(?:\.\d+)?)["']/;

export type UrlExpansion = {
  coordinate?: CoordinatePoint;
  expandedUrl?: string;
  title?: string;
};

/** Try to extract a coordinate from a URL string directly (no network). */
export function extractCoordinateFromText(text: string): CoordinatePoint | undefined {
  return (
    matchCoord(COORD_AT, text) ??
    matchCoord(COORD_QUERY, text) ??
    matchCoord(DAATA_LL, text) ??
    matchCoord(PROPERTY_GEO, text) ??
    matchCoord(META_COORD, text)
  );
}

/** Fetch the URL through one or more proxies and try to extract a destination. */
export async function expandShortLink(url: string, signal?: AbortSignal): Promise<UrlExpansion> {
  const errors: string[] = [];
  let combinedBody = "";
  let firstTitle: string | undefined;
  let firstExpandedUrl: string | undefined;

  for (const proxyOf of PROXIES) {
    if (signal?.aborted) throw makeAbortError();
    try {
      const response = await fetch(proxyOf(url), {
        signal,
        headers: { accept: "text/plain, text/html, */*" },
      });
      if (!response.ok) {
        errors.push(`HTTP ${response.status}`);
        continue;
      }
      const body = await response.text();
      combinedBody += `\n${body}`;
      const title = extractTitle(body);
      if (title && !firstTitle) firstTitle = title;
      const expandedUrl = extractFinalUrl(body);
      if (expandedUrl && !firstExpandedUrl) firstExpandedUrl = expandedUrl;
      const coordinate = extractCoordinateFromText(body);
      if (coordinate) {
        return { coordinate, expandedUrl: expandedUrl ?? url, title };
      }
    } catch (err) {
      if ((err as Error)?.name === "AbortError") throw err;
      errors.push(err instanceof Error ? err.message : "Unknown error");
    }
  }

  // One last sweep over the union of all bodies in case patterns straddle proxies.
  const coordinate = combinedBody ? extractCoordinateFromText(combinedBody) : undefined;
  if (coordinate) {
    return { coordinate, expandedUrl: firstExpandedUrl ?? url, title: firstTitle };
  }
  if (errors.length > 0 && combinedBody === "") {
    throw new Error(`URL expansion failed: ${errors.join(" | ")}`);
  }
  return { expandedUrl: firstExpandedUrl ?? url, title: firstTitle };
}

function makeAbortError(): Error {
  const error = new Error("Aborted");
  error.name = "AbortError";
  return error;
}

function matchCoord(regex: RegExp, source: string): CoordinatePoint | undefined {
  const match = regex.exec(source);
  if (!match) return undefined;
  const lat = parseFloat(match[1]);
  const lon = parseFloat(match[2]);
  if (!Number.isFinite(lat) || !Number.isFinite(lon)) return undefined;
  if (lat < -90 || lat > 90 || lon < -180 || lon > 180) return undefined;
  return { latitude: lat, longitude: lon };
}

function extractFinalUrl(body: string): string | undefined {
  // r.jina.ai puts the final resolved URL on a "URL Source: ..." line.
  const jina = /^URL Source:\s*(\S+)/m.exec(body);
  if (jina?.[1]) return jina[1];
  // Otherwise look for a canonical link tag (raw HTML proxies).
  const canonical = /<link\s[^>]*rel=["']canonical["'][^>]*href=["']([^"']+)["']/i.exec(body);
  if (canonical?.[1]) return canonical[1];
  // Or an OpenGraph URL.
  const og = /<meta\s[^>]*property=["']og:url["'][^>]*content=["']([^"']+)["']/i.exec(body);
  return og?.[1];
}

const GENERIC_TITLES = new Set(["google maps", "openstreetmap", "redirecting", "loading", ""]);

function extractTitle(body: string): string | undefined {
  const jina = /^Title:\s*(.+)$/m.exec(body);
  const candidate = jina?.[1]?.trim() ?? /<title>([^<]*)<\/title>/i.exec(body)?.[1]?.trim();
  if (!candidate) return undefined;
  return GENERIC_TITLES.has(candidate.toLowerCase()) ? undefined : candidate;
}

const URL_REGEX = /^(https?:\/\/\S+)$/i;

/** Heuristic: does this look like the user pasted a URL? */
export function looksLikeUrl(text: string): boolean {
  return URL_REGEX.test(text.trim());
}
