/**
 * Writes the `clean` line icons as SVG files for the desktop app (`gen-icons` in devenv).
 *
 * gpui paints an SVG as a single-colour mask, so every kind gets two files: `<kind>.body.svg`
 * with the filled `body` parts (tinted with the body colour) and `<kind>.svg` with the strokes
 * and `solid` parts (tinted with the stroke colour), drawn on top of each other.
 *
 * Run with Node's type stripping: `node web/scripts/gen-desktop-icons.ts <out-dir>`.
 */
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { CLEAN_ICONS, ICON_GRID } from "../src/render2d/icons-clean.ts";

/** Same ratio as the web canvas: 2 px strokes on a 43.2 px glyph (72 px tile × 0.6). */
const STROKE = 1.1;

const out = process.argv[2];
if (!out) throw new Error("usage: gen-desktop-icons.ts <out-dir>");
mkdirSync(out, { recursive: true });
for (const f of readdirSync(out)) if (f.endsWith(".svg")) rmSync(join(out, f));

const svg = (paths: string[]) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${ICON_GRID} ${ICON_GRID}" width="${ICON_GRID}" height="${ICON_GRID}">\n${paths.join("\n")}\n</svg>\n`;

for (const [kind, parts] of Object.entries(CLEAN_ICONS)) {
  const strokes = parts.map((p) => {
    const fill = p.kind === "solid" ? "#fff" : "none";
    const dash = p.dash ? ` stroke-dasharray="${p.dash.join(" ")}"` : "";
    return `<path d="${p.d}" fill="${fill}" stroke="#fff" stroke-width="${STROKE}" stroke-linecap="round" stroke-linejoin="round"${dash}/>`;
  });
  const bodies = parts
    .filter((p) => p.kind === "body")
    .map((p) => `<path d="${p.d}" fill="#fff"/>`);
  writeFileSync(join(out, `${kind}.svg`), svg(strokes));
  writeFileSync(join(out, `${kind}.body.svg`), svg(bodies));
}
