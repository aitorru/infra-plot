import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { type Download, expect, type Page, test } from "@playwright/test";

const examples = resolve(import.meta.dirname, "../../examples");

interface Diagram {
  title: string;
  zones: { id: string; x: number; y: number; w: number; h: number; label?: string }[];
  nodes: { id: string; kind: string; x: number; y: number; label?: string }[];
  edges: { id: string; from: string; to: string }[];
  lines: { id: string; points: [number, number][] }[];
  notes: { id: string; x: number; y: number; text: string }[];
}

async function autosaved(page: Page): Promise<Diagram> {
  return page.evaluate(() => JSON.parse(localStorage.getItem("infraplot:autosave") ?? "{}"));
}

async function ready(page: Page, url = "/"): Promise<void> {
  await page.goto(url);
  await expect(page.locator("#app")).toHaveAttribute("data-ready", "true");
}

async function importFile(page: Page, file: string): Promise<void> {
  await page.getByTestId("file-input").setInputFiles(file);
}

/** World → screen coordinates, read from the 2D viewport transform. */
async function toScreen(page: Page, x: number, y: number): Promise<[number, number]> {
  return page.getByTestId("canvas-2d").evaluate(
    (svg, [wx, wy]) => {
      const t = svg.querySelector(".viewport")?.getAttribute("transform") ?? "";
      const m = /translate\(([-\d.e]+) ([-\d.e]+)\) scale\(([-\d.e]+)\)/.exec(t);
      if (!m) throw new Error(`unexpected transform: ${t}`);
      const r = svg.getBoundingClientRect();
      const [tx, ty, k] = [Number(m[1]), Number(m[2]), Number(m[3])];
      return [r.left + tx + wx * k, r.top + ty + wy * k] as [number, number];
    },
    [x, y] as const,
  );
}

async function clickAt(page: Page, x: number, y: number): Promise<void> {
  await page.mouse.click(...(await toScreen(page, x, y)));
}

async function dragWorld(page: Page, from: [number, number], to: [number, number]): Promise<void> {
  const a = await toScreen(page, ...from);
  const b = await toScreen(page, ...to);
  await page.mouse.move(...a);
  await page.mouse.down();
  await page.mouse.move((a[0] + b[0]) / 2, (a[1] + b[1]) / 2, { steps: 5 });
  await page.mouse.move(...b, { steps: 5 });
  await page.mouse.up();
}

async function blur(page: Page): Promise<void> {
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
}

async function saved(download: Download): Promise<string> {
  return readFileSync((await download.path()) ?? "", "utf8");
}

function count(toml: string, table: string): number {
  return toml.match(new RegExp(`^\\[\\[${table}\\]\\]`, "gm"))?.length ?? 0;
}

test("builds a diagram: node, label, zone, edge, free line and note", async ({ page }) => {
  await ready(page);
  const palette = page.getByTestId("palette");
  const props = page.getByTestId("props");
  const nodes = page.locator(".layer-nodes > g");

  // Place two nodes; each placement selects the node and returns to the select tool.
  await palette.locator('[data-tool="server"]').click();
  await clickAt(page, -160, 0);
  await expect(nodes).toHaveCount(1);
  await expect(props).toHaveAttribute("data-type", "node");
  await props.locator('input[name="label"]').fill("web-1");
  await blur(page);
  await expect(page.locator('.layer-nodes [data-id="server-1"] .label')).toHaveText("web-1");

  await palette.locator('[data-tool="database"]').click();
  await clickAt(page, 160, 0);
  await expect(nodes).toHaveCount(2);
  // Double-clicking an element jumps to its label.
  await page.locator('.layer-nodes [data-id="database-1"]').dblclick();
  await expect(props.locator('input[name="label"]')).toBeFocused();
  await blur(page);

  // Zone around both nodes (R).
  await page.keyboard.press("r");
  await dragWorld(page, [-280, -120], [280, 180]);
  await expect(page.locator(".layer-zones > g")).toHaveCount(1);
  await expect(props).toHaveAttribute("data-type", "zone");

  // Connect server → database (C).
  await page.keyboard.press("c");
  await dragWorld(page, [-160, 0], [160, 0]);
  await expect(page.locator(".layer-edges > g")).toHaveCount(1);
  await expect(props).toHaveAttribute("data-type", "edge");

  // Free line by dragging (L).
  await page.keyboard.press("l");
  await dragWorld(page, [-240, 120], [-60, 120]);
  await expect(page.locator(".layer-lines > g")).toHaveCount(1);

  // Multi-point line: click, click, double-click.
  await page.keyboard.press("l");
  await clickAt(page, 60, 120);
  await clickAt(page, 160, 140);
  await page.mouse.dblclick(...(await toScreen(page, 240, 120)));
  await expect(page.locator(".layer-lines > g")).toHaveCount(2);

  // Note (T): the text field gets focus straight away.
  await page.keyboard.press("t");
  await clickAt(page, -260, 220);
  await expect(props).toHaveAttribute("data-type", "note");
  await expect(props.locator('textarea[name="text"]')).toBeFocused();
  await page.keyboard.type("Primary site");
  await blur(page);
  await expect(page.locator('.layer-notes [data-id="note-1"]')).toContainText("Primary site");

  const doc = await autosaved(page);
  expect(doc.nodes).toMatchObject([
    { id: "server-1", kind: "server", label: "web-1", x: -160, y: 0 },
    { id: "database-1", kind: "database", x: 160, y: 0 },
  ]);
  expect(doc.zones).toMatchObject([{ id: "zone-1", x: -280, y: -120, w: 560, h: 300 }]);
  expect(doc.edges).toMatchObject([{ id: "edge-1", from: "server-1", to: "database-1" }]);
  expect(doc.lines.map((l) => l.points)).toEqual([
    [
      [-240, 120],
      [-60, 120],
    ],
    [
      [60, 120],
      [160, 140],
      [240, 120],
    ],
  ]);
  expect(doc.notes).toMatchObject([{ id: "note-1", x: -260, y: 220, text: "Primary site" }]);
});

test("undo/redo, delete with dangling edges, duplicate, move zone with contents", async ({
  page,
}) => {
  await ready(page);
  await importFile(page, resolve(examples, "hello.json"));
  const nodes = page.locator(".layer-nodes > g");
  const edges = page.locator(".layer-edges > g");
  await expect(nodes).toHaveCount(4);
  await expect(edges).toHaveCount(3);

  // Deleting the LB also removes both edges touching it.
  await page.locator('.layer-nodes [data-id="lb"]').click();
  await page.keyboard.press("Delete");
  await expect(nodes).toHaveCount(3);
  await expect(edges).toHaveCount(1);
  expect((await autosaved(page)).edges.map((e) => e.id)).toEqual(["e3"]);

  await page.keyboard.press("Control+z");
  await expect(nodes).toHaveCount(4);
  await expect(edges).toHaveCount(3);
  await page.keyboard.press("Control+y");
  await expect(nodes).toHaveCount(3);
  await page.getByRole("button", { name: "↶" }).click();
  await expect(nodes).toHaveCount(4);
  await expect(page.getByRole("button", { name: "↷" })).toBeEnabled();

  // Duplicate lands next to the original and gets selected.
  await page.locator('.layer-nodes [data-id="api"]').click();
  await page.keyboard.press("Control+d");
  await expect(nodes).toHaveCount(5);
  await expect(page.getByTestId("props").locator('input[name="id"]')).toHaveValue("service-1");
  expect((await autosaved(page)).nodes.find((n) => n.id === "service-1")).toMatchObject({
    x: 430,
    y: 210,
    label: "API",
  });
  await page.keyboard.press("Control+z");
  await expect(nodes).toHaveCount(4);

  // Dragging the VPC by its corner brings along what is fully inside it.
  const before = await autosaved(page);
  await dragWorld(page, [140, 60], [240, 120]);
  await expect.poll(async () => (await autosaved(page)).zones[0]?.x).toBe(220);
  const after = await autosaved(page);
  const pos = (d: Diagram, id: string) => d.nodes.find((n) => n.id === id);
  expect(after.zones[0]).toMatchObject({ x: 220, y: 100, w: 520, h: 260 });
  for (const id of ["lb", "api", "db"]) {
    expect(pos(after, id)).toMatchObject({ x: (pos(before, id)?.x ?? 0) + 100, y: 230 });
  }
  expect(pos(after, "user")).toMatchObject({ x: 40, y: 170 });
  expect(after.notes[0]).toMatchObject({ x: 380, y: 330 });

  // A single undo puts the zone and its contents back.
  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await autosaved(page)).zones[0]?.x).toBe(120);
  expect(pos(await autosaved(page), "db")).toMatchObject({ x: 560, y: 170 });
});

for (const file of ["three-tier.toml", "k8s-platform.toml"]) {
  test(`imports ${file} and round-trips it through JSON and TOML`, async ({ page }) => {
    const toml = readFileSync(resolve(examples, file), "utf8");
    const expected = {
      zones: count(toml, "zones"),
      nodes: count(toml, "nodes"),
      edges: count(toml, "edges"),
      lines: count(toml, "lines"),
      notes: count(toml, "notes"),
    };
    const rendered = async () => {
      for (const [layer, n] of Object.entries(expected)) {
        await expect(page.locator(`.layer-${layer} > g`)).toHaveCount(n);
      }
    };

    await ready(page);
    await importFile(page, resolve(examples, file));
    await rendered();

    const [jsonDl] = await Promise.all([
      page.waitForEvent("download"),
      page.getByRole("button", { name: "JSON", exact: true }).click(),
    ]);
    const json = await saved(jsonDl);
    const doc = JSON.parse(json) as Diagram;
    expect(doc.nodes).toHaveLength(expected.nodes);
    expect(doc.edges).toHaveLength(expected.edges);

    const [tomlDl] = await Promise.all([
      page.waitForEvent("download"),
      page.getByRole("button", { name: "TOML", exact: true }).click(),
    ]);
    expect(tomlDl.suggestedFilename()).toMatch(/\.toml$/);

    // Re-import both exports: same elements, and exporting again gives the same JSON.
    for (const dl of [jsonDl, tomlDl]) {
      await page.getByRole("button", { name: "New" }).click();
      await expect(page.locator(".layer-nodes > g")).toHaveCount(0);
      await importFile(page, (await dl.path()) ?? "");
      await rendered();
      const [again] = await Promise.all([
        page.waitForEvent("download"),
        page.getByRole("button", { name: "JSON", exact: true }).click(),
      ]);
      expect(JSON.parse(await saved(again))).toEqual(doc);
    }
  });
}

test("saves to the server and reloads /d/:id with the changes", async ({ page, request }) => {
  await ready(page);
  await importFile(page, resolve(examples, "hello.json"));
  await expect(page.locator(".layer-nodes > g")).toHaveCount(4);
  await page.getByLabel("Diagram title").fill("Persisted diagram");
  await page.getByLabel("Diagram title").press("Enter");
  await page.locator('.layer-nodes [data-id="api"]').click();
  await page.getByTestId("props").locator('input[name="label"]').fill("API v2");
  await blur(page);

  await page.keyboard.press("Control+s");
  await expect(page).toHaveURL(/\/d\/persisted-diagram$/);
  await expect(page.locator(".toast")).toContainText("Saved as persisted-diagram");

  const res = await request.get("/api/diagrams/persisted-diagram");
  expect(res.ok()).toBe(true);
  const stored = (await res.json()) as Diagram;
  expect(stored.title).toBe("Persisted diagram");
  expect(stored.nodes.find((n) => n.id === "api")?.label).toBe("API v2");
  const asToml = await request.get("/api/diagrams/persisted-diagram?format=toml");
  expect(await asToml.text()).toContain('title = "Persisted diagram"');

  // A fresh page (no autosave) opens the saved copy from the URL.
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(page.locator("#app")).toHaveAttribute("data-ready", "true");
  await expect(page.getByLabel("Diagram title")).toHaveValue("Persisted diagram");
  await expect(page.locator('.layer-nodes [data-id="api"] .label')).toHaveText("API v2");
  await expect(page.locator(".layer-nodes > g")).toHaveCount(4);

  // Saving again overwrites the same id.
  await page.locator('.layer-nodes [data-id="user"]').click();
  await page.keyboard.press("Delete");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".toast").last()).toContainText("Saved as persisted-diagram");
  const updated = (await (await request.get("/api/diagrams/persisted-diagram")).json()) as Diagram;
  expect(updated.nodes.map((n) => n.id)).toEqual(["lb", "api", "db"]);

  await page.goto("/d/does-not-exist");
  await expect(page.locator(".toast.error")).toContainText("Could not load does-not-exist: 404");
});

test("rejects an invalid import with the validation issues", async ({ page }) => {
  await ready(page);
  await importFile(page, resolve(examples, "hello.json"));
  await page.getByTestId("file-input").setInputFiles({
    name: "broken.json",
    mimeType: "application/json",
    buffer: Buffer.from(
      JSON.stringify({ version: 1, title: "x", edges: [{ id: "e", from: "a", to: "b" }] }),
    ),
  });
  await expect(page.locator(".toast.error")).toBeVisible();
  // The current diagram is left untouched.
  await expect(page.locator(".layer-nodes > g")).toHaveCount(4);
});
