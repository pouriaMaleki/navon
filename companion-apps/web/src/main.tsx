import React from "react";
import { createRoot } from "react-dom/client";
import "maplibre-gl/dist/maplibre-gl.css";
import { AppShell } from "./app/AppShell.js";
import { RootStore } from "./app/RootStore.js";
import "./vars.css";
import "./global.css";

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
