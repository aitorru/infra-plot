import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, type Page, test } from "@playwright/test";

const examples = resolve(import.meta.dirname, "../../examples");
const hello = JSON.parse(readFileSync(resolve(examples, "hello.json"), "utf8")) as {
  nodes: { id: string }[];
};

async function openHello(page: Page, url = "/"): Promise<void> {
  await page.goto(url);
  await page.getByTestId("file-input").setInputFiles(resolve(examples, "hello.json"));
  await expect(page.locator(".layer-nodes > g")).toHaveCount(hello.nodes.length);
}

test("palette shows rough icons for every kind", async ({ page }) => {
  await page.goto("/");
  const palette = page.getByTestId("palette");
  // 8 zone kinds + 20 node kinds, each with a sketched icon.
  await expect(palette.locator("svg.icon")).toHaveCount(28);
  await expect(palette.locator('[data-tool="database"] svg.icon path').first()).toBeVisible();
});

test("2D/3D toggle, shortcuts and ?view=3d", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("canvas-2d")).toBeVisible();
  await page.keyboard.press("3");
  await expect(page.getByTestId("view-3d")).toBeVisible();
  await expect(page.getByTestId("canvas-2d")).toBeHidden();
  await expect(page).toHaveURL(/view=3d/);
  await page.getByRole("button", { name: "2D", exact: true }).click();
  await expect(page.getByTestId("canvas-2d")).toBeVisible();
  await expect(page).not.toHaveURL(/view=3d/);

  await page.goto("/?view=3d");
  await expect(page.getByTestId("view-3d")).toBeVisible();
});

test("examples menu loads a TOML example", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Open…" }).click();
  const examplesList = page.getByTestId("examples");
  await expect(examplesList.getByRole("button")).toHaveText([
    "hello.json",
    "k8s-platform.toml",
    "three-tier.toml",
  ]);
  await examplesList.getByRole("button", { name: "three-tier.toml" }).click();
  await expect(page.getByLabel("Diagram title")).toHaveValue("Three-tier web app");
  await expect(page.locator(".layer-nodes > g")).toHaveCount(8);
  await expect(page.locator(".layer-zones > g")).toHaveCount(5);
});

test("server list: save, list, open and delete", async ({ page }) => {
  await openHello(page);
  await page.getByLabel("Diagram title").fill("Listed diagram");
  await page.getByLabel("Diagram title").press("Enter");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page).toHaveURL(/\/d\/listed-diagram$/);

  await page.getByRole("button", { name: "New" }).click();
  await expect(page.locator(".layer-nodes > g")).toHaveCount(0);

  await page.keyboard.press("Control+o");
  const item = page.getByTestId("server-diagrams").locator('li[data-id="listed-diagram"]');
  await expect(item).toContainText("Listed diagram");
  await item.getByRole("button").first().click();
  await expect(page.locator(".layer-nodes > g")).toHaveCount(hello.nodes.length);
  await expect(page).toHaveURL(/\/d\/listed-diagram$/);

  await page.keyboard.press("Control+o");
  page.once("dialog", (d) => void d.accept());
  await item.getByRole("button", { name: /Delete/ }).click();
  await expect(item).toHaveCount(0);
});

test("properties panel edits node kind, meta and edge style", async ({ page }) => {
  await openHello(page);
  const props = page.getByTestId("props");

  await page.locator('.layer-nodes [data-id="api"]').click();
  await expect(props).toHaveAttribute("data-type", "node");
  await props.getByLabel("kind").selectOption("function");
  await props.getByRole("button", { name: "+ Add" }).click();
  await props.getByLabel("meta key").fill("port");
  await props.getByLabel("meta value").fill("8080");
  await props.getByLabel("meta value").press("Tab");

  await page.locator('.layer-edges [data-id="e2"] .hit').click({ force: true });
  await expect(props).toHaveAttribute("data-type", "edge");
  await props.getByLabel("style").selectOption("dotted");
  await props.getByLabel("arrow").selectOption("both");
  await props.getByLabel("route").selectOption("orthogonal");

  const doc = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("infraplot:autosave") ?? "{}"),
  );
  const api = doc.nodes.find((n: { id: string }) => n.id === "api");
  expect(api.kind).toBe("function");
  expect(api.meta).toEqual({ port: "8080" });
  expect(doc.edges.find((e: { id: string }) => e.id === "e2")).toMatchObject({
    style: "dotted",
    arrow: "both",
    route: "orthogonal",
  });
});

test("?src= loads a remote file and ?embed=1 hides the editor chrome", async ({ page }) => {
  await page.route("**/fixtures/k8s.toml", (route) =>
    route.fulfill({
      body: readFileSync(resolve(examples, "k8s-platform.toml"), "utf8"),
      contentType: "application/toml",
    }),
  );
  await page.goto("/?src=/fixtures/k8s.toml&embed=1");
  await expect(page.locator("#app")).toHaveAttribute("data-ready", "true");
  await expect(page.locator(".layer-nodes > g")).toHaveCount(13);
  await expect(page.locator(".topbar")).toBeHidden();
  await expect(page.getByTestId("palette")).toBeHidden();
  await expect(page.getByTestId("props")).toBeHidden();
});

test("SVG and PNG exports embed the Kalam font", async ({ page }) => {
  await openHello(page);
  const [svgDownload] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "SVG", exact: true }).click(),
  ]);
  const svg = readFileSync((await svgDownload.path()) ?? "", "utf8");
  expect(svg).toContain("@font-face{font-family:Kalam");
  expect(svg).toContain("data:font/woff2;base64,");

  const [pngDownload] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "PNG", exact: true }).click(),
  ]);
  expect(pngDownload.suggestedFilename()).toBe("hello-infra-plot.png");
  const png = readFileSync((await pngDownload.path()) ?? "");
  expect(png.subarray(1, 4).toString()).toBe("PNG");
  // 2× the SVG's width.
  const svgWidth = Number(/width="(\d+(?:\.\d+)?)"/.exec(svg)?.[1]);
  expect(png.readUInt32BE(16)).toBe(Math.ceil(svgWidth * 2));
});
