import { defineConfig } from "vitest/config";

export default defineConfig({
  define: {
    "import.meta.env.VITE_APP_VERSION": JSON.stringify(process.env.VITE_APP_VERSION),
    "import.meta.env.VITE_APP_GIT_HASH": JSON.stringify(process.env.VITE_APP_GIT_HASH),
  },
  server: {
    host: "0.0.0.0",
    port: 5173,
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
  test: {
    environment: "jsdom",
    globals: true,
    css: false,
    // Playwright specs in e2e/ use @playwright/test; vitest must not try to
    // load them. Keep them on a clearly separate path.
    exclude: ["**/node_modules/**", "e2e/**"],
  },
});
