import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { autosaved, clickAt, dragWorld, ready } from "./helpers";

const root = resolve(import.meta.dirname, "../..");
const schema = JSON.parse(readFileSync(resolve(root, "schema/diagram.schema.json"), "utf8")) as {
  $defs: { NodeKind: { enum: string[] } };
};
const nodeKinds = schema.$defs.NodeKind.enum;

async function load(page: Page, doc: object, url = "/"): Promise<void> {
  await ready(page, url);
  await page.getByTestId("file-input").setInputFiles({
    name: "doc.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify({ version: 1, title: "Test", ...doc })),
  });
}

test("themes: ?theme= overrides, the picker switches and persists", async ({ page }) => {
  await ready(page, "/?theme=dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  await ready(page, "/");
  const picker = page.getByTestId("theme-select");
  await picker.selectOption("nier");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "nier");
  // The canvas repaints with the theme's grid colour.
  await expect(page.locator("#grid circle")).toHaveAttribute("fill", "#bdb8a1");

  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "nier");
  await expect(picker).toHaveValue("nier");
  const bg = await page
    .getByTestId("canvas-2d")
    .evaluate((el) => getComputedStyle(el).backgroundColor);
  expect(bg).toBe("rgb(209, 205, 183)");
});

test("palette search filters node tiles and hides empty groups", async ({ page }) => {
  await ready(page);
  const palette = page.getByTestId("palette");
  await palette.getByTestId("palette-search").fill("buck");
  await expect(palette.locator('[data-tool="bucket"]')).toBeVisible();
  await expect(palette.locator('[data-tool="database"]')).toBeHidden();
  await expect(palette.getByRole("heading", { name: "Data" })).toBeVisible();
  await expect(palette.getByRole("heading", { name: "Compute" })).toBeHidden();
  await palette.getByTestId("palette-search").fill("");
  await expect(palette.locator('[data-tool="database"]')).toBeVisible();
});

test("look switches between clean and sketch and round-trips", async ({ page }) => {
  await load(page, {
    nodes: [{ id: "api", kind: "service", label: "API", x: 0, y: 0 }],
  });
  const label = page.locator('.layer-nodes [data-id="api"] text').last();
  await expect(label).toHaveAttribute("font-family", /Inter/);
  expect((await autosaved(page)).look).toBeUndefined();

  await page.getByTestId("props").getByLabel("look").selectOption("sketch");
  await expect(label).toHaveAttribute("font-family", /Kalam/);
  expect((await autosaved(page)).look).toBe("sketch");

  await page.keyboard.press("Control+z");
  await expect(label).toHaveAttribute("font-family", /Inter/);
});

test("orthogonal bend: drag the handle, undo, and the props slider", async ({ page }) => {
  await load(page, {
    nodes: [
      { id: "a", kind: "service", x: 0, y: 0 },
      { id: "b", kind: "database", x: 300, y: 200 },
    ],
    edges: [{ id: "e", from: "a", to: "b", route: "orthogonal" }],
  });
  // Boxes are 72 wide with 6 of padding: the bend spans x = 42..258, so 0.5 is x = 150.
  await clickAt(page, 150, 100);
  const handle = page.getByTestId("bend-handle");
  await expect(handle).toHaveCount(1);
  await dragWorld(page, [150, 100], [200, 100]);
  const edge = async () => (await autosaved(page)).edges[0];
  await expect.poll(async () => (await edge())?.bend).toBeCloseTo((200 - 42) / 216, 3);

  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await edge())?.bend).toBeUndefined();

  const slider = page.getByTestId("props").locator('input[name="bend"]');
  await slider.fill("25");
  await expect.poll(async () => (await edge())?.bend).toBe(0.25);
});

test("dragging a line segment keeps its right angles", async ({ page }) => {
  await load(page, {
    lines: [
      {
        id: "l",
        points: [
          [0, 0],
          [200, 0],
          [200, 200],
        ],
      },
    ],
  });
  await clickAt(page, 100, 0);
  await expect(page.getByTestId("props")).toHaveAttribute("data-type", "line");
  await expect(page.getByTestId("vertex-handle")).toHaveCount(3);
  await expect(page.getByTestId("segment-handle")).toHaveCount(2);
  // The vertical segment moves sideways; both of its ends follow.
  await dragWorld(page, [200, 100], [260, 100]);
  await expect
    .poll(async () => (await autosaved(page)).lines[0]?.points)
    .toEqual([
      [0, 0],
      [260, 0],
      [260, 200],
    ]);
});

test("every node kind renders in 2D and 3D", async ({ page }) => {
  expect(nodeKinds.length).toBe(42);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  await load(page, {
    nodes: nodeKinds.map((kind, i) => ({
      id: kind,
      kind,
      label: kind,
      x: (i % 7) * 140,
      y: Math.floor(i / 7) * 140,
    })),
  });
  await expect(page.locator(".layer-nodes > g")).toHaveCount(42);
  await page.keyboard.press("3");
  await expect(page.getByTestId("view-3d")).toHaveAttribute("data-nodes", "42");
  expect(errors).toEqual([]);
});
