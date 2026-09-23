import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "@playwright/test";

const hello = resolve(import.meta.dirname, "../../examples/hello.json");

test("opens an example and renders its elements", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("canvas-2d")).toBeVisible();
  await page.locator('input[type="file"]').setInputFiles(hello);
  const doc = JSON.parse(readFileSync(hello, "utf8")) as { nodes: unknown[] };
  await expect(page.locator(".layer-nodes > g")).toHaveCount(doc.nodes.length);
});

test("API rejects invalid diagrams with 422", async ({ request }) => {
  const res = await request.put("/api/diagrams/bad", {
    data: { version: 1, title: "bad", edges: [{ id: "e", from: "nope", to: "nada" }] },
  });
  expect(res.status()).toBe(422);
});
