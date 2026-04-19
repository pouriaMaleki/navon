# Companion Web

Web companion app for the ESP32 bike minimap. Mirrors the iOS companion's product surface (Home + Settings) but renders on OSM tiles via MapLibre, runs entirely in the browser, and has no device/BLE features. Phone guidance is the only post-`Start` mode.

## Stack
- React 19 + `mobx-react-lite`, with all logic in MobX sub-stores under one `RootStore`.
- Vite 6 + TypeScript 5 (strict).
- MapLibre GL JS with the OSM raster style.
- Photon (typeahead) + Nominatim (reverse geocode) for places.
- Digitransit HSL GraphQL + OSRM bike for routing, GPX import via DOMParser, sample fallback for everything.
- `localStorage` for persistence, key prefix `companion.*`.

## Run
```bash
cd companion-web
npm install
npm run dev    # http://localhost:5173
npm run typecheck
npm run lint
npm run test
```

## Architecture
See `/docs/companion-app-architecture.md` for the shared product architecture. Web-specific structure:

```
src/
  app/           composition root (RootStore, AppShell)
  domain/        TypeScript port of CompanionModels.swift contracts
  stores/        observable sub-stores (Settings, Planning, Guidance, History, MapCamera, Diagnostics)
  integrations/  routing adapters, search, persistence, GPX, URL classifier
  features/      observer components for Home and Settings
  test/          vitest specs
```

Components are dumb observers and only receive a store reference. All business logic lives in stores or integrations.
