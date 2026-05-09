import { makeAutoObservable } from "mobx";
import { WAVESHARE_ESP32_P4_3_4 } from "../core/screenProfiles";
import type { ScreenProfile } from "../core/types";
import type { RouteAlertVerbosity } from "../types";
import { BikeSimStore } from "./BikeSimStore";
import { EmulatorStore } from "./EmulatorStore";
import { GeoStore } from "./GeoStore";
import { TouchStore } from "./TouchStore";

type ScreenProfileFactory = (canvas: HTMLCanvasElement) => ScreenProfile;

type AppStoreOptions = {
  screenProfile?: ScreenProfile | ScreenProfileFactory;
  autoRequestLiveGps?: boolean;
};

const ROUTE_ALERTS_STORAGE_KEY = "esp32-minimap.route-alerts";
const ROUTE_ALERT_VERBOSITY_OPTIONS: RouteAlertVerbosity[] = ["essential", "standard", "detailed"];

export class AppStore {
  readonly bikeSimStore: BikeSimStore;
  readonly geoStore: GeoStore;
  readonly touchStore: TouchStore;
  readonly emulatorStore: EmulatorStore;
  routeAlertVerbosity: RouteAlertVerbosity;

  constructor(options?: AppStoreOptions) {
    const configuredProfile = options?.screenProfile;
    const profileFactory =
      typeof configuredProfile === "function"
        ? configuredProfile
        : () => configuredProfile ?? WAVESHARE_ESP32_P4_3_4;
    this.routeAlertVerbosity = readInitialRouteAlertVerbosity();
    this.geoStore =
      options?.autoRequestLiveGps === undefined
        ? new GeoStore()
        : new GeoStore({ autoRequestLiveGps: options.autoRequestLiveGps });
    this.bikeSimStore = new BikeSimStore(this.geoStore);
    this.touchStore = new TouchStore();
    this.emulatorStore = new EmulatorStore(this, profileFactory);

    makeAutoObservable(this, {}, { autoBind: true });
  }

  setRouteAlertVerbosity(value: RouteAlertVerbosity): void {
    if (this.routeAlertVerbosity === value) {
      return;
    }
    this.routeAlertVerbosity = value;
    persistRouteAlertVerbosity(value);
    void this.emulatorStore.restartRuntime();
  }

  dispose(): void {
    this.emulatorStore.dispose();
  }
}

function readInitialRouteAlertVerbosity(): RouteAlertVerbosity {
  const searchValue = readRouteAlertVerbosityFromSearch(globalThis.location?.search);
  if (searchValue) {
    persistRouteAlertVerbosity(searchValue);
    return searchValue;
  }

  try {
    const stored = globalThis.localStorage?.getItem(ROUTE_ALERTS_STORAGE_KEY) ?? null;
    if (isRouteAlertVerbosity(stored)) {
      return stored;
    }
  } catch {
    // Ignore storage access failures and fall back to default.
  }

  return "standard";
}

function persistRouteAlertVerbosity(value: RouteAlertVerbosity): void {
  try {
    globalThis.localStorage?.setItem(ROUTE_ALERTS_STORAGE_KEY, value);
  } catch {
    // Ignore storage access failures so the UI remains interactive.
  }

  try {
    const url = new URL(globalThis.location?.href ?? "", globalThis.location?.origin);
    url.searchParams.set("routeAlerts", value);
    globalThis.history?.replaceState?.(null, "", `${url.pathname}${url.search}${url.hash}`);
  } catch {
    // Ignore URL mutation failures in non-browser or locked-down contexts.
  }
}

function readRouteAlertVerbosityFromSearch(search: string | null | undefined): RouteAlertVerbosity | null {
  const raw = typeof search === "string" ? search : "";
  const value = new URLSearchParams(raw).get("routeAlerts")?.trim().toLowerCase() ?? null;
  return isRouteAlertVerbosity(value) ? value : null;
}

function isRouteAlertVerbosity(value: string | null): value is RouteAlertVerbosity {
  return typeof value === "string" && ROUTE_ALERT_VERBOSITY_OPTIONS.includes(value as RouteAlertVerbosity);
}
