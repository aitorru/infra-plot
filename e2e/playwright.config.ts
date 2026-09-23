import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { defineConfig, devices } from "@playwright/test";

const root = resolve(import.meta.dirname, "..");
const port = Number(process.env.INFRAPLOT_E2E_PORT ?? 31090);
const dataDir = mkdtempSync(join(tmpdir(), "infraplot-e2e-"));

export default defineConfig({
  testDir: "tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    trace: "retain-on-failure",
    // WebGL for the 3D view: headless Chromium only falls back to SwiftShader when asked.
    launchOptions: { args: ["--enable-unsafe-swiftshader", "--use-angle=swiftshader"] },
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: `${root}/target/release/infraplot-server`,
    url: `http://127.0.0.1:${port}/api/health`,
    reuseExistingServer: false,
    env: {
      INFRAPLOT_BIND: `127.0.0.1:${port}`,
      INFRAPLOT_DATA_DIR: dataDir,
      INFRAPLOT_STATIC_DIR: `${root}/web/dist`,
    },
  },
});
