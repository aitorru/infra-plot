/** Small rough.js icons for the palette, sharing the glyphs of the 2D canvas. */
import rough from "roughjs";
import type { Options } from "roughjs/bin/core";
import { INK, NODE_KINDS, NODE_SIZE, ZONE_KINDS } from "../model/catalog";
import type { NodeKind, ZoneKind } from "../model/doc";
import { hashSeed } from "../model/geometry";
import { drawGlyph } from "../render2d/glyphs";

const SVG_NS = "http://www.w3.org/2000/svg";

function iconSvg(): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", `0 0 ${NODE_SIZE} ${NODE_SIZE}`);
  svg.setAttribute("class", "icon");
  svg.setAttribute("aria-hidden", "true");
  return svg;
}

function base(key: string, fill: string): Options {
  return {
    seed: hashSeed(key),
    roughness: 1,
    stroke: INK,
    strokeWidth: 2.2,
    fill,
    fillStyle: "hachure",
    hachureGap: 6,
    fillWeight: 1.6,
  };
}

export function nodeIcon(kind: NodeKind): SVGSVGElement {
  const svg = iconSvg();
  const rc = rough.svg(svg);
  const s = NODE_SIZE - 4;
  svg.appendChild(
    drawGlyph(rc, kind, NODE_SIZE / 2, NODE_SIZE / 2, s, base(kind, NODE_KINDS[kind].color)),
  );
  return svg;
}

export function zoneIcon(kind: ZoneKind): SVGSVGElement {
  const svg = iconSvg();
  const rc = rough.svg(svg);
  const info = ZONE_KINDS[kind];
  const o: Options = { ...base(kind, info.color), fillStyle: "solid", stroke: "#495057" };
  if (info.style === "dashed") o.strokeLineDash = [10, 8];
  if (info.style === "dotted") o.strokeLineDash = [2, 7];
  svg.appendChild(rc.rectangle(6, 14, NODE_SIZE - 12, NODE_SIZE - 28, o));
  return svg;
}
