import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, type Locator, type Page, test } from "@playwright/test";

const examples = resolve(import.meta.dirname, "../../examples");

interface Diagram {
  zones?: unknown[];
  nodes?: { id: string; x: number; y: number }[];
  edges?: unknown[];
  notes?: unknown[];
}

const hello = JSON.parse(readFileSync(resolve(examples, "hello.json"), "utf8")) as Diagram;

/** Share of pixels that are clearly not the paper background (grid dots excluded). */
async function inkedShare(page: Page, png: Buffer): Promise<number> {
  return page.evaluate(async (b64) => {
    const blob = await (await fetch(`data:image/png;base64,${b64}`)).blob();
    const img = await createImageBitmap(blob);
    const ctx = new OffscreenCanvas(img.width, img.height).getContext("2d");
    if (!ctx) throw new Error("no 2D context");
    ctx.drawImage(img, 0, 0);
    const d = ctx.getImageData(0, 0, img.width, img.height).data;
    let inked = 0;
    for (let i = 0; i < d.length; i += 4) {
      const diff =
        Math.abs((d[i] ?? 0) - 0xfd) +
        Math.abs((d[i + 1] ?? 0) - 0xfc) +
        Math.abs((d[i + 2] ?? 0) - 0xf8);
      if (diff > 120) inked++;
    }
    return inked / (d.length / 4);
  }, png.toString("base64"));
}

async function open3d(page: Page, file: string, url = "/?view=3d"): Promise<Locator> {
  await page.goto(url);
  await expect(page.locator("#app")).toHaveAttribute("data-ready", "true");
  await page.getByTestId("file-input").setInputFiles(resolve(examples, file));
  const view = page.getByTestId("view-3d");
  await expect(view).toBeVisible();
  return view;
}

async function autosaved(page: Page): Promise<Diagram> {
  return page.evaluate(() => JSON.parse(localStorage.getItem("infraplot:autosave") ?? "{}"));
}

test("3D view renders every element and paints the canvas", async ({ page }) => {
  const view = await open3d(page, "hello.json");
  await expect(view).toHaveAttribute("data-nodes", String(hello.nodes?.length));
  await expect(view).toHaveAttribute("data-zones", String(hello.zones?.length));
  await expect(view).toHaveAttribute("data-edges", String(hello.edges?.length));
  await expect(view).toHaveAttribute("data-notes", String(hello.notes?.length));
  // hello.json: 1 zone slab + user (2) + LB (2) + API (2) + Postgres (3) + 3 edge tubes
  // + 3 arrow heads.
  await expect(view).toHaveAttribute("data-meshes", "16");
  await expect(view).toHaveAttribute("data-packets", /^[1-9]/);

  const canvas = page.getByTestId("canvas-3d");
  await expect(canvas).toBeVisible();
  const full = await inkedShare(page, await canvas.screenshot());
  expect(full).toBeGreaterThan(0.02);

  // An empty diagram leaves just paper (and the floating toolbar).
  await page.getByRole("button", { name: "New" }).click();
  await expect(view).toHaveAttribute("data-nodes", "0");
  await expect(view).toHaveAttribute("data-meshes", "0");
  expect(await inkedShare(page, await canvas.screenshot())).toBeLessThan(full / 4);
});

test("rotates in 90° steps, fits and toggles packets", async ({ page }) => {
  const view = await open3d(page, "three-tier.toml");
  await expect(view).toHaveAttribute("data-nodes", "8");
  await expect(view).toHaveAttribute("data-azimuth", "45");
  await page.keyboard.press("e");
  await expect(view).toHaveAttribute("data-azimuth", "135");
  await page.getByRole("button", { name: "⟳" }).click();
  await expect(view).toHaveAttribute("data-azimuth", "225");
  await page.keyboard.press("q");
  await page.keyboard.press("q");
  await expect(view).toHaveAttribute("data-azimuth", "45");

  // Zoom right in, then fit brings the whole diagram back.
  const fitted = Number(await view.getAttribute("data-zoom"));
  const canvas = page.getByTestId("canvas-3d");
  await canvas.hover();
  for (let i = 0; i < 5; i++) await page.mouse.wheel(0, -400);
  await expect
    .poll(async () => Number(await view.getAttribute("data-zoom")))
    .toBeGreaterThan(fitted);
  await page.keyboard.press("f");
  await expect
    .poll(async () => Number(await view.getAttribute("data-zoom")))
    .toBeCloseTo(fitted, 1);

  const packets = page.getByRole("button", { name: "Packets" });
  await expect(packets).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("p");
  await expect(packets).toHaveAttribute("aria-pressed", "false");
});

test("selects and drags a node on the ground plane", async ({ page }) => {
  await page.goto("/?view=3d");
  await expect(page.locator("#app")).toHaveAttribute("data-ready", "true");
  const canvas = page.getByTestId("canvas-3d");
  // With a single node, "fit" puts it in the middle of the canvas.
  await page.getByRole("button", { name: /^Server/ }).click();
  const box = await canvas.boundingBox();
  if (!box) throw new Error("no canvas");
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  await page.mouse.click(cx, cy);
  const view = page.getByTestId("view-3d");
  await expect(view).toHaveAttribute("data-nodes", "1");
  const placed = (await autosaved(page)).nodes?.[0];
  if (!placed) throw new Error("node not placed");
  await expect(page.getByTestId("props")).toHaveAttribute("data-type", "node");

  await page.keyboard.press("Escape");
  await expect(page.getByTestId("props")).not.toHaveAttribute("data-type", "node");
  await page.keyboard.press("f");

  const target = page.getByTestId("view-3d");
  await expect.poll(async () => Number(await target.getAttribute("data-zoom"))).toBeGreaterThan(1);
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 60, cy + 40, { steps: 8 });
  await page.mouse.move(cx + 120, cy + 80, { steps: 8 });
  await page.mouse.up();
  await expect(page.getByTestId("props")).toHaveAttribute("data-type", "node");

  await expect
    .poll(async () => {
      const n = (await autosaved(page)).nodes?.[0];
      return n ? Math.hypot(n.x - placed.x, n.y - placed.y) : 0;
    })
    .toBeGreaterThan(20);
  // Still on the 20-unit grid.
  const moved = (await autosaved(page)).nodes?.[0];
  expect((moved?.x ?? 1) % 20).toBe(0);
  expect((moved?.y ?? 1) % 20).toBe(0);

  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await autosaved(page)).nodes?.[0]?.x).toBe(placed.x);
});

test("?embed=1 shows only the 3D canvas", async ({ page }) => {
  await page.route("**/fixtures/k8s.toml", (route) =>
    route.fulfill({
      body: readFileSync(resolve(examples, "k8s-platform.toml"), "utf8"),
      contentType: "application/toml",
    }),
  );
  await page.goto("/?src=/fixtures/k8s.toml&embed=1&view=3d");
  await expect(page.locator("#app")).toHaveAttribute("data-ready", "true");
  const view = page.getByTestId("view-3d");
  await expect(view).toHaveAttribute("data-nodes", "13");
  await expect(view).toHaveAttribute("data-zones", "5");
  await expect(page.getByTestId("canvas-3d")).toBeVisible();
  await expect(page.getByTestId("toolbar-3d")).toBeHidden();
  await expect(page.locator(".topbar")).toBeHidden();
  expect(await inkedShare(page, await page.getByTestId("canvas-3d").screenshot())).toBeGreaterThan(
    0.02,
  );
});

test("PNG export in 3D is a high-resolution photo of the view", async ({ page }) => {
  await open3d(page, "hello.json");
  const canvas = page.getByTestId("canvas-3d");
  const box = await canvas.boundingBox();
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "PNG", exact: true }).click(),
  ]);
  expect(download.suggestedFilename()).toBe("hello-infra-plot-3d.png");
  const png = readFileSync((await download.path()) ?? "");
  expect(png.subarray(1, 4).toString()).toBe("PNG");
  const width = png.readUInt32BE(16);
  expect(width).toBe(Math.min(Math.round((box?.width ?? 0) * 3), 4096));
  expect(await inkedShare(page, png)).toBeGreaterThan(0.02);
});
