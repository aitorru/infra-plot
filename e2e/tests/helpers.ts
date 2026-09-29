/** Shared helpers for the E2E specs; everything works in world coordinates of the 2D canvas. */
import { readFileSync } from "node:fs";
import { type Download, expect, type Page } from "@playwright/test";

export interface Diagram {
  title: string;
  look?: string;
  zones: { id: string; x: number; y: number; w: number; h: number; label?: string }[];
  nodes: { id: string; kind: string; x: number; y: number; label?: string }[];
  edges: { id: string; from: string; to: string; route?: string; bend?: number }[];
  lines: { id: string; points: [number, number][] }[];
  notes: { id: string; x: number; y: number; text: string }[];
}

export async function autosaved(page: Page): Promise<Diagram> {
  return page.evaluate(() => JSON.parse(localStorage.getItem("infraplot:autosave") ?? "{}"));
}

export async function ready(page: Page, url = "/"): Promise<void> {
  await page.goto(url);
  await expect(page.locator("#app")).toHaveAttribute("data-ready", "true");
}

export async function importFile(page: Page, file: string): Promise<void> {
  await page.getByTestId("file-input").setInputFiles(file);
}

/** World → screen coordinates, read from the 2D viewport transform. */
export async function toScreen(page: Page, x: number, y: number): Promise<[number, number]> {
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

export async function clickAt(page: Page, x: number, y: number): Promise<void> {
  await page.mouse.click(...(await toScreen(page, x, y)));
}

export async function dragWorld(
  page: Page,
  from: [number, number],
  to: [number, number],
): Promise<void> {
  const a = await toScreen(page, ...from);
  const b = await toScreen(page, ...to);
  await page.mouse.move(...a);
  await page.mouse.down();
  await page.mouse.move((a[0] + b[0]) / 2, (a[1] + b[1]) / 2, { steps: 5 });
  await page.mouse.move(...b, { steps: 5 });
  await page.mouse.up();
}

export async function blur(page: Page): Promise<void> {
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
}

export async function saved(download: Download): Promise<string> {
  return readFileSync((await download.path()) ?? "", "utf8");
}
