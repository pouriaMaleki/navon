import basicSsl from "@vitejs/plugin-basic-ssl";
import { defineConfig } from "astro/config";

export default defineConfig({
  vite: {
    plugins: [basicSsl()],
  },
});
