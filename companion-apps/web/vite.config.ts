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
    exclude: ["**/node_modules/**", "e2e/**"],
    // Fixture files in __testlib__/fixtures/ use Node built-ins (node:fs, node:path,
    // node:url) to read data from parity-fixtures. Allow those built-ins in
    // the test runner environment instead of treating them as bundler errors.
    deps: {
      optimizer: {
        ssr: {
          include: ["**/__testlib__/fixtures/**"],
        },
      },
    },
    server: {
      deps: {
        external: [/^node:/],
      },
    },
  },
});
