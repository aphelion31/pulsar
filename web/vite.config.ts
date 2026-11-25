import { reactRouter } from "@react-router/dev/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

const api = process.env.PULSAR_URL ?? "http://127.0.0.1:3000";

export default defineConfig({
  base: "/app/",
  plugins: [tailwindcss(), reactRouter()],
  resolve: { tsconfigPaths: true },
  // dev: proxy API, auth, websocket and files to a running pulsar binary
  server: {
    proxy: {
      "/api": api,
      "/auth": api,
      "/rails": api,
      "/cable": { target: api, ws: true },
    },
  },
});
