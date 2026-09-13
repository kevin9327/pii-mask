import { defineConfig } from "vite";

export default defineConfig({
  root: ".",
  worker: { format: "es" },
  server: {
    host: "127.0.0.1",
    port: 4177,
    headers: {
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
  preview: {
    port: 4177,
  },
  build: {
    target: "es2022",
  },
});
