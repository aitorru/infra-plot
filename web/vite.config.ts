import { defineConfig } from "vite";

const webPort = Number(process.env.INFRAPLOT_WEB_PORT ?? 31173);
const apiPort = Number(process.env.INFRAPLOT_API_PORT ?? 31080);

export default defineConfig({
  server: {
    port: webPort,
    strictPort: true,
    // examples/ and schema/ live outside the web package.
    fs: { allow: [".."] },
    proxy: { "/api": `http://127.0.0.1:${apiPort}` },
  },
  build: {
    target: "es2022",
    sourcemap: true,
    chunkSizeWarningLimit: 1200,
  },
});
