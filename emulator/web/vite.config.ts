import { resolve } from "node:path";
import { defineConfig } from "vite";

export default defineConfig({
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
