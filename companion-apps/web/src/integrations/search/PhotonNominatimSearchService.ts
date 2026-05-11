import type { CoordinatePoint, DestinationSearchResult } from "../../domain/models.js";
import type { PlaceSearchService } from "../../domain/providers.js";

const PHOTON_BASE = "https://photon.komoot.io/api/";
const NOMINATIM_BASE = "https://nominatim.openstreetmap.org/reverse";
const APP_USER_AGENT = "navon-bike-web/0.1";

type PhotonFeature = {
  geometry?: { coordinates?: [number, number] };
  properties?: {
    name?: string;
    street?: string;
    housenumber?: string;
    city?: string;
    state?: string;
    country?: string;
    postcode?: string;
    type?: string;
    osm_id?: number;
    osm_type?: string;
  };
};

type PhotonResponse = { features?: PhotonFeature[] };

type NominatimAddress = {
  road?: string;
  house_number?: string;
  city?: string;
  town?: string;
  village?: string;
  suburb?: string;
  state?: string;
  country?: string;
};

type NominatimResponse = {
  display_name?: string;
  name?: string;
  address?: NominatimAddress;
};

export class PhotonNominatimSearchService implements PlaceSearchService {
  async searchDestinations(
    query: string,
    limit: number,
    riderBias?: CoordinatePoint,
    signal?: AbortSignal,
  ): Promise<DestinationSearchResult[]> {
    const trimmed = query.trim();
    if (trimmed.length === 0) return [];
    // Photon accepts `lat` + `lon` as a bias hint for ranking; when supplied,
    // same-city results rank first (spec line 75).
    const biasParams =
      riderBias && Number.isFinite(riderBias.latitude) && Number.isFinite(riderBias.longitude)
        ? `&lat=${riderBias.latitude}&lon=${riderBias.longitude}`
        : "";
    const url = `${PHOTON_BASE}?q=${encodeURIComponent(trimmed)}&limit=${Math.max(1, Math.min(20, limit))}${biasParams}`;
    try {
      const response = await fetch(url, { signal });
      if (!response.ok) return [];
      const data = (await response.json()) as PhotonResponse;
      const features = data.features ?? [];
      return features
        .map((feature, index) => mapPhotonFeature(feature, index))
        .filter((r): r is DestinationSearchResult => r !== null);
    } catch (err) {
      if ((err as Error)?.name === "AbortError") throw err;
      return [];
    }
  }

  async resolveDestination(
    coordinate: CoordinatePoint,
    fallbackTitle: string,
    signal?: AbortSignal,
  ): Promise<DestinationSearchResult | null> {
    const url = `${NOMINATIM_BASE}?format=json&lat=${coordinate.latitude}&lon=${coordinate.longitude}&zoom=18&addressdetails=1`;
    try {
      const response = await fetch(url, {
        signal,
        headers: { "User-Agent": APP_USER_AGENT, accept: "application/json" },
      });
      if (!response.ok) {
        return fallbackResult(coordinate, fallbackTitle);
      }
      const data = (await response.json()) as NominatimResponse;
      const address = data.address ?? {};
      const street = [address.road, address.house_number].filter(Boolean).join(" ");
      const locality = address.city ?? address.town ?? address.village ?? address.suburb;
      const title = street || data.name || locality || fallbackTitle;
      const subtitleParts = [locality, address.state, address.country].filter(Boolean);
      const subtitle = subtitleParts.join(" • ") || data.display_name || "";
      return {
        id: `pin-${coordinate.latitude.toFixed(5)}-${coordinate.longitude.toFixed(5)}`,
        title,
        subtitle,
        coordinate,
      };
    } catch (err) {
      if ((err as Error)?.name === "AbortError") throw err;
      return fallbackResult(coordinate, fallbackTitle);
    }
  }
}

function mapPhotonFeature(feature: PhotonFeature, index: number): DestinationSearchResult | null {
  const coords = feature.geometry?.coordinates;
  if (!coords || coords.length < 2) return null;
  const [lon, lat] = coords;
  if (!Number.isFinite(lat) || !Number.isFinite(lon)) return null;
  const props = feature.properties ?? {};
  const street = [props.street, props.housenumber].filter(Boolean).join(" ");
  const title = props.name ?? street ?? props.city ?? "Unnamed place";
  const subtitleParts = [props.city, props.state, props.country].filter(Boolean);
  const subtitle = subtitleParts.join(" • ");
  const idBase =
    props.osm_id !== undefined && props.osm_type
      ? `${props.osm_type}-${props.osm_id}`
      : `idx-${index}`;
  return {
    id: `photon-${idBase}-${lat.toFixed(5)}-${lon.toFixed(5)}`,
    title,
    subtitle,
    coordinate: { latitude: lat, longitude: lon },
  };
}

function fallbackResult(
  coordinate: CoordinatePoint,
  fallbackTitle: string,
): DestinationSearchResult {
  return {
    id: `pin-${coordinate.latitude.toFixed(5)}-${coordinate.longitude.toFixed(5)}`,
    title: fallbackTitle,
    subtitle: `${coordinate.latitude.toFixed(5)}, ${coordinate.longitude.toFixed(5)}`,
    coordinate,
  };
}
