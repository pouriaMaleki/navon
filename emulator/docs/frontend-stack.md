# Frontend Stack Guide

This document describes how the emulator frontend is structured with React, MobX, and CSS Modules.

## Stack Summary
- View layer: React 19 function components.
- State layer: MobX stores with `makeAutoObservable`.
- React bindings: `mobx-react-lite` `observer`.
- Styling: CSS Modules (`*.module.css`).
- Lint/format: Biome.
- Build tool: Vite 7 + TypeScript strict mode.

## Directory Conventions
- `web/src/ui`: presentational components and small UI composition logic.
- `web/src/stores`: stateful orchestration, runtime side effects, browser APIs.
- `web/src/programs`: WASM render program adapter and camera update logic.
- `web/src/core`: lower-level emulator engine/canvas primitives.
- `web/src/types.ts`: shared types used across stores/programs/core.

## MobX Conventions
- Keep stores class-based and focused:
  - `AppStore`: composition root.
  - `EmulatorStore`: emulator lifecycle and control actions.
  - `GeoStore`: geolocation + simulation fallback.
  - `TouchStore`: gesture translation to camera inputs.
- Use `makeAutoObservable(this, overrides, { autoBind: true })`.
- Mark non-observable/private implementation fields with annotation overrides.
- Keep side effects in store methods, not in render functions.

## React Conventions
- Use small function components with typed props.
- Wrap components that read observable state with `observer`.
- Keep component effects minimal and lifecycle-specific (for example `init`/`dispose` bridging).
- Avoid global mutable state outside MobX stores.

## CSS Modules Conventions
- Use one `ComponentName.module.css` per component group.
- Import as `styles` and access class names via bracket notation: `styles["className"]`.
- Keep styles component-local; avoid global CSS except app bootstrapping.
- Prefer semantic class names tied to UI role (`panel`, `status`, `button`).

## TypeScript Rules
- Strict mode is mandatory.
- Do not use `any` or `as any`.
- Keep shared cross-module contracts in `web/src/types.ts`.
- Use explicit guards for nullable browser/runtime values.

## Biome Rules
- Use `npm run lint` for CI/local checks.
- Use `npm run lint:fix` for safe fixes + formatting.
- Use `npm run format` for formatter-only pass.
- Keep `biome.json` aligned with current CLI schema version.

## Typical Feature Flow
1. Add/update shared contract in `web/src/types.ts` when crossing module boundaries.
2. Implement state and side effects in a store.
3. Wire state into UI component via typed props + `observer`.
4. Add component-scoped styles in CSS Module.
5. Run `npm run typecheck` and `npm run build`.
