/**
 * Palette icons, in the clean style of the 2D canvas (the palette is UI, so it never sketches).
 * Colours are CSS expressions over the theme's custom properties (`--ink`, `--panel`, …), so
 * the icons follow theme switches without being rebuilt. `--icon-tint` (default 60%) sets how
 * much of the kind colour goes into the tiles; dark themes may want it lower.
 */
import { NODE_KINDS, NODE_SIZE, ZONE_KINDS } from "../model/catalog";
import type { NodeKind, ZoneKind } from "../model/doc";
import { cleanTile } from "../render2d/icons-clean";

const SVG_NS = "http://www.w3.org/2000/svg";

function iconSvg(): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", `0 0 ${NODE_SIZE} ${NODE_SIZE}`);
  svg.setAttribute("class", "icon");
  svg.setAttribute("aria-hidden", "true");
  return svg;
}

const tint = (color: string) =>
  `color-mix(in srgb, ${color} var(--icon-tint, 60%), var(--panel, #fff))`;
const shade = (color: string, ink: number) =>
  `color-mix(in srgb, ${color} ${100 - ink}%, var(--ink, #1f2328))`;

export function nodeIcon(kind: NodeKind): SVGSVGElement {
  const svg = iconSvg();
  const color = NODE_KINDS[kind].color;
  const c = NODE_SIZE / 2;
  // Drawn at 72 units and shown at ~36px: strokes are doubled to stay ~1.5px on screen.
  svg.appendChild(
    cleanTile(
      kind,
      c,
      c,
      NODE_SIZE,
      {
        tile: tint(color),
        border: shade(color, 40),
        stroke: shade(color, 75),
        body: `color-mix(in srgb, ${color} 14%, var(--panel, #fff))`,
      },
      3.4,
      2.4,
    ),
  );
  return svg;
}

export function zoneIcon(kind: ZoneKind): SVGSVGElement {
  const svg = iconSvg();
  const info = ZONE_KINDS[kind];
  const rect = document.createElementNS(SVG_NS, "rect");
  const attrs = { x: 6, y: 14, width: NODE_SIZE - 12, height: NODE_SIZE - 28, rx: 10 };
  for (const [k, v] of Object.entries(attrs)) rect.setAttribute(k, String(v));
  rect.style.setProperty("fill", tint(info.color));
  rect.style.setProperty("stroke", `color-mix(in srgb, ${info.color} 35%, var(--muted, #6a737d))`);
  rect.style.setProperty("stroke-width", "3");
  rect.style.setProperty("stroke-linecap", "round");
  if (info.style === "dashed") rect.style.setProperty("stroke-dasharray", "9 7");
  if (info.style === "dotted") rect.style.setProperty("stroke-dasharray", "0 7");
  svg.appendChild(rect);
  // The kind tag in the top-left corner, as on the canvas.
  const tag = document.createElementNS(SVG_NS, "rect");
  for (const [k, v] of Object.entries({ x: 14, y: 22, width: 22, height: 6, rx: 3 })) {
    tag.setAttribute(k, String(v));
  }
  tag.style.setProperty("fill", "var(--muted, #6a737d)");
  svg.appendChild(tag);
  return svg;
}
