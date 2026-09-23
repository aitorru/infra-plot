/** README screenshots: `SCREENSHOTS=1 devenv --profile e2e shell -- e2e screenshots`. */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, type Page, test } from "@playwright/test";

const root = resolve(import.meta.dirname, "../..");
const out = resolve(root, "docs/screenshots");

test.skip(!process.env.SCREENSHOTS, "set SCREENSHOTS=1 to refresh docs/screenshots");
test.use({ viewport: { width: 1440, height: 900 } });

async function open(page: Page, example: string, query: string): Promise<void> {
  await page.route(
    (url) => url.pathname === `/fixtures/${example}`,
    (route) =>
      route.fulfill({
        body: readFileSync(resolve(root, "examples", example), "utf8"),
        contentType: "application/toml",
      }),
  );
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto(`/?src=/fixtures/${example}${query}`);
  await expect(page.locator("#app")).toHaveAttribute("data-ready", "true");
}

test("2D editor", async ({ page }) => {
  await open(page, "three-tier.toml", "");
  await page.locator('.layer-nodes [data-id="app1"]').click();
  await page.screenshot({ path: resolve(out, "editor-2d.png") });
});

test("3D view", async ({ page }) => {
  await open(page, "k8s-platform.toml", "&view=3d&embed=1");
  await page.getByTestId("canvas-3d").screenshot({ path: resolve(out, "view-3d.png") });
});
