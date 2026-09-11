import { setWorkerUrl } from "maplibre-gl";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url";
import React from "react";
import { createRoot } from "react-dom/client";
import "maplibre-gl/dist/maplibre-gl.css";
import { AppShell } from "./app/AppShell.js";
import { RootStore } from "./app/RootStore.js";
import "./vars.css";
import "./global.css";

// v6 resolves its worker URL at runtime from import.meta.url, which Vite cannot
// rewrite. ?worker&url makes Vite bundle the worker self-contained (the raw
// file imports maplibre-gl-shared.mjs, which plain ?url never emits).
setWorkerUrl(workerUrl);

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("#root element missing from index.html");
}

const store = new RootStore();

createRoot(rootElement).render(
  <React.StrictMode>
    <AppShell store={store} />
  </React.StrictMode>,
);
