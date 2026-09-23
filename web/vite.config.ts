import { defineConfig } from "vite";

export default defineConfig({
  server: {
    port: 5173,
    // examples/ and schema/ live outside the web package.
    fs: { allow: [".."] },
    proxy: { "/api": "http://127.0.0.1:8080" },
  },
  build: {
    target: "es2022",
    sourcemap: true,
    chunkSizeWarningLimit: 1200,
  },
});
