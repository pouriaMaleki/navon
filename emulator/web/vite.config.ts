import { resolve } from "node:path";
import { defineConfig } from "vite";

// When the emulator is served under a path prefix (e.g. `/emulator/` in the combined
// companion-stack image), set `VITE_BASE_PATH=/emulator/` at build time so every asset,
// entry, and dynamic chunk URL is rewritten to live below that prefix. Default `/` keeps
// local dev and the stand-alone deployment unchanged.
const basePath = process.env.VITE_BASE_PATH ?? "/";

export default defineConfig({
  base: basePath,
  server: {
    host: "0.0.0.0",
    port: 5173,
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        webFullscreen: resolve(__dirname, "web-fullscreen.html"),
      },
    },
  },
});
