/** Hand-drawn icons for every node kind, drawn with rough.js into a square box. */

import type { Options } from "roughjs/bin/core";
import type { RoughSVG } from "roughjs/bin/svg";
import { FONT_FAMILY, INK } from "../model/catalog";
import type { NodeKind } from "../model/doc";

const SVG_NS = "http://www.w3.org/2000/svg";

export function svgText(
  x: number,
  y: number,
  text: string,
  size: number,
  opts: { anchor?: "start" | "middle" | "end"; color?: string; weight?: number } = {},
): SVGTextElement {
  const t = document.createElementNS(SVG_NS, "text");
  t.setAttribute("x", String(x));
  t.setAttribute("y", String(y));
  t.setAttribute("font-family", FONT_FAMILY);
  t.setAttribute("font-size", String(size));
  t.setAttribute("fill", opts.color ?? INK);
  t.setAttribute("text-anchor", opts.anchor ?? "middle");
  t.setAttribute("dominant-baseline", "central");
  if (opts.weight) t.setAttribute("font-weight", String(opts.weight));
  t.textContent = text;
  return t;
}

/**
 * Draws the glyph for `kind` centred on (cx, cy) inside a `s`×`s` box.
 * `base` carries seed/colour so the sketch is deterministic per element.
 */
export function drawGlyph(
  rc: RoughSVG,
  kind: NodeKind,
  cx: number,
  cy: number,
  s: number,
  base: Options,
): SVGGElement {
  const g = document.createElementNS(SVG_NS, "g");
  const add = (el: SVGElement) => g.appendChild(el);
  const h = s / 2;
  const x0 = cx - h;
  const y0 = cy - h;
  const line: Options = { ...base };
  delete line.fill;
  const u = s / 72; // designed on a 72px grid

  switch (kind) {
    case "service":
      add(rc.path(roundedRect(x0 + 4 * u, y0 + 10 * u, s - 8 * u, s - 20 * u, 12 * u), base));
      add(svgText(cx, cy, "{ }", 22 * u, { weight: 700 }));
      break;
    case "server": {
      const w = 48 * u;
      add(rc.rectangle(cx - w / 2, y0 + 2 * u, w, s - 4 * u, base));
      for (let i = 1; i < 4; i++) {
        const y = y0 + 2 * u + (i * (s - 4 * u)) / 4;
        add(rc.line(cx - w / 2, y, cx + w / 2, y, line));
      }
      for (let i = 0; i < 4; i++) {
        const y = y0 + 2 * u + ((i + 0.5) * (s - 4 * u)) / 4;
        add(rc.circle(cx + w / 2 - 9 * u, y, 5 * u, { ...base, fill: INK, fillStyle: "solid" }));
        add(rc.line(cx - w / 2 + 7 * u, y, cx + 4 * u, y, line));
      }
      break;
    }
    case "vm":
      add(rc.rectangle(x0 + 4 * u, y0 + 8 * u, s - 8 * u, s - 22 * u, base));
      add(
        rc.rectangle(x0 + 12 * u, y0 + 16 * u, s - 24 * u, s - 38 * u, {
          ...line,
          strokeLineDash: [4, 4],
        }),
      );
      add(rc.line(cx - 12 * u, y0 + s - 6 * u, cx + 12 * u, y0 + s - 6 * u, line));
      add(rc.line(cx, y0 + s - 14 * u, cx, y0 + s - 6 * u, line));
      break;
    case "container":
      add(isoCube(rc, cx, cy + 4 * u, 30 * u, base));
      break;
    case "pod":
      add(rc.polygon(hexagon(cx, cy, h - 2 * u), base));
      add(isoCube(rc, cx, cy + 3 * u, 14 * u, { ...base, fill: "#ffffff" }));
      break;
    case "k8s": {
      add(rc.polygon(ngon(cx, cy, h - 2 * u, 7), base));
      add(rc.circle(cx, cy, 30 * u, { ...line, strokeWidth: 2 }));
      for (let i = 0; i < 7; i++) {
        const a = (i / 7) * Math.PI * 2 - Math.PI / 2;
        add(rc.line(cx, cy, cx + Math.cos(a) * 20 * u, cy + Math.sin(a) * 20 * u, line));
      }
      add(rc.circle(cx, cy, 8 * u, { ...base, fill: INK, fillStyle: "solid" }));
      break;
    }
    case "proxy":
      add(
        rc.polygon(
          [
            [cx, y0 + 4 * u],
            [x0 + s - 4 * u, cy],
            [cx, y0 + s - 4 * u],
            [x0 + 4 * u, cy],
          ],
          base,
        ),
      );
      add(arrowLine(rc, cx - 18 * u, cy - 5 * u, cx + 18 * u, cy - 5 * u, line));
      add(arrowLine(rc, cx + 18 * u, cy + 6 * u, cx - 18 * u, cy + 6 * u, line));
      break;
    case "load-balancer":
      add(rc.circle(cx, cy, s - 8 * u, base));
      add(rc.line(cx - 22 * u, cy, cx - 4 * u, cy, line));
      for (const dy of [-14, 0, 14]) {
        add(arrowLine(rc, cx - 4 * u, cy, cx + 20 * u, cy + dy * u, line));
      }
      break;
    case "api-gateway":
      add(
        rc.path(
          `M${x0 + 8 * u} ${y0 + s - 4 * u} V${y0 + 26 * u} Q${cx} ${y0} ${x0 + s - 8 * u} ${y0 + 26 * u} V${y0 + s - 4 * u} Z`,
          base,
        ),
      );
      for (const dx of [-12, 0, 12]) {
        add(rc.line(cx + dx * u, y0 + 24 * u, cx + dx * u, y0 + s - 4 * u, line));
      }
      break;
    case "database": {
      const w = 52 * u;
      const top = y0 + 10 * u;
      const bottom = y0 + s - 10 * u;
      const eh = 16 * u;
      add(
        rc.path(
          `M${cx - w / 2} ${top} V${bottom} A${w / 2} ${eh / 2} 0 0 0 ${cx + w / 2} ${bottom} V${top}`,
          base,
        ),
      );
      add(rc.ellipse(cx, top, w, eh, { ...base }));
      for (const k of [0.36, 0.68]) {
        const y = top + (bottom - top) * k;
        add(rc.path(`M${cx - w / 2} ${y} A${w / 2} ${eh / 2} 0 0 0 ${cx + w / 2} ${y}`, line));
      }
      break;
    }
    case "cache": {
      const w = 56 * u;
      const top = y0 + 20 * u;
      const bottom = y0 + s - 18 * u;
      add(
        rc.path(
          `M${cx - w / 2} ${top} V${bottom} A${w / 2} ${8 * u} 0 0 0 ${cx + w / 2} ${bottom} V${top}`,
          base,
        ),
      );
      add(rc.ellipse(cx, top, w, 16 * u, base));
      add(
        rc.polygon(
          [
            [cx + 2 * u, y0 + 2 * u],
            [cx - 10 * u, cy + 4 * u],
            [cx, cy + 4 * u],
            [cx - 4 * u, y0 + s - 2 * u],
            [cx + 12 * u, cy - 4 * u],
            [cx + 2 * u, cy - 4 * u],
          ],
          { ...base, fill: "#ffe066", fillStyle: "solid" },
        ),
      );
      break;
    }
    case "queue": {
      const w = s - 4 * u;
      const hh = 30 * u;
      add(rc.rectangle(cx - w / 2, cy - hh / 2, w, hh, base));
      for (let i = 1; i < 5; i++) {
        const x = cx - w / 2 + (i * w) / 5;
        add(rc.line(x, cy - hh / 2, x, cy + hh / 2, line));
      }
      add(arrowLine(rc, cx - w / 2, cy + hh / 2 + 10 * u, cx + w / 2, cy + hh / 2 + 10 * u, line));
      break;
    }
    case "storage": {
      const top = y0 + 12 * u;
      add(
        rc.polygon(
          [
            [x0 + 8 * u, top],
            [x0 + s - 8 * u, top],
            [x0 + s - 16 * u, y0 + s - 4 * u],
            [x0 + 16 * u, y0 + s - 4 * u],
          ],
          base,
        ),
      );
      add(rc.ellipse(cx, top, s - 16 * u, 14 * u, { ...base, fill: "#ffffff" }));
      break;
    }
    case "function":
      add(rc.circle(cx, cy, s - 8 * u, base));
      add(svgText(cx, cy + 2 * u, "λ", 36 * u, { weight: 700 }));
      break;
    case "firewall": {
      const top = y0 + 8 * u;
      const rows = 4;
      const rh = (s - 16 * u) / rows;
      add(rc.rectangle(x0 + 4 * u, top, s - 8 * u, s - 16 * u, base));
      for (let r = 0; r < rows; r++) {
        const y = top + r * rh;
        if (r > 0) add(rc.line(x0 + 4 * u, y, x0 + s - 4 * u, y, line));
        const bw = (s - 8 * u) / 3;
        const offsets = r % 2 === 0 ? [bw, 2 * bw] : [bw / 2, 1.5 * bw, 2.5 * bw];
        for (const off of offsets) {
          const x = x0 + 4 * u + off;
          add(rc.line(x, y, x, y + rh, line));
        }
      }
      break;
    }
    case "cdn":
      add(rc.circle(cx, cy, s - 8 * u, base));
      add(rc.ellipse(cx, cy, 28 * u, s - 8 * u, line));
      add(rc.line(x0 + 4 * u, cy, x0 + s - 4 * u, cy, line));
      add(
        rc.path(
          `M${x0 + 10 * u} ${cy - 16 * u} Q${cx} ${cy - 10 * u} ${x0 + s - 10 * u} ${cy - 16 * u}`,
          line,
        ),
      );
      add(
        rc.path(
          `M${x0 + 10 * u} ${cy + 16 * u} Q${cx} ${cy + 10 * u} ${x0 + s - 10 * u} ${cy + 16 * u}`,
          line,
        ),
      );
      break;
    case "dns":
      add(rc.path(roundedRect(x0 + 2 * u, y0 + 18 * u, s - 4 * u, s - 36 * u, 8 * u), base));
      add(svgText(cx, cy, "DNS", 18 * u, { weight: 700 }));
      break;
    case "user":
      add(rc.circle(cx, y0 + 20 * u, 26 * u, base));
      add(
        rc.path(
          `M${x0 + 10 * u} ${y0 + s - 2 * u} Q${x0 + 10 * u} ${y0 + 38 * u} ${cx} ${y0 + 38 * u} Q${x0 + s - 10 * u} ${y0 + 38 * u} ${x0 + s - 10 * u} ${y0 + s - 2 * u} Z`,
          base,
        ),
      );
      break;
    case "internet":
      add(
        rc.path(
          `M${x0 + 16 * u} ${y0 + 54 * u} A${12 * u} ${12 * u} 0 0 1 ${x0 + 14 * u} ${y0 + 32 * u} A${14 * u} ${14 * u} 0 0 1 ${x0 + 34 * u} ${y0 + 18 * u} A${16 * u} ${16 * u} 0 0 1 ${x0 + 60 * u} ${y0 + 30 * u} A${12 * u} ${12 * u} 0 0 1 ${x0 + 58 * u} ${y0 + 54 * u} Z`,
          base,
        ),
      );
      break;
    case "monitoring":
      add(rc.rectangle(x0 + 4 * u, y0 + 6 * u, s - 8 * u, s - 24 * u, base));
      add(
        rc.linearPath(
          [
            [x0 + 12 * u, y0 + 40 * u],
            [x0 + 24 * u, y0 + 28 * u],
            [x0 + 34 * u, y0 + 36 * u],
            [x0 + 46 * u, y0 + 16 * u],
            [x0 + 60 * u, y0 + 24 * u],
          ],
          { ...line, stroke: "#e03131", strokeWidth: 2 },
        ),
      );
      add(rc.line(cx, y0 + s - 18 * u, cx, y0 + s - 6 * u, line));
      add(rc.line(cx - 14 * u, y0 + s - 6 * u, cx + 14 * u, y0 + s - 6 * u, line));
      break;
  }
  return g;
}

function isoCube(rc: RoughSVG, cx: number, cy: number, r: number, o: Options): SVGGElement {
  const g = document.createElementNS(SVG_NS, "g");
  const dx = r * Math.cos(Math.PI / 6);
  const dy = r / 2;
  const top: [number, number][] = [
    [cx, cy - r],
    [cx + dx, cy - r + dy],
    [cx, cy],
    [cx - dx, cy - r + dy],
  ];
  const left: [number, number][] = [
    [cx - dx, cy - r + dy],
    [cx, cy],
    [cx, cy + r],
    [cx - dx, cy + dy],
  ];
  const right: [number, number][] = [
    [cx, cy],
    [cx + dx, cy - r + dy],
    [cx + dx, cy + dy],
    [cx, cy + r],
  ];
  g.appendChild(rc.polygon(left, o));
  g.appendChild(rc.polygon(right, { ...o, fillStyle: "cross-hatch" }));
  g.appendChild(rc.polygon(top, { ...o, fill: "#ffffff", fillStyle: "solid" }));
  return g;
}

function ngon(cx: number, cy: number, r: number, n: number): [number, number][] {
  return Array.from({ length: n }, (_, i) => {
    const a = (i / n) * Math.PI * 2 - Math.PI / 2;
    return [cx + Math.cos(a) * r, cy + Math.sin(a) * r];
  });
}

function hexagon(cx: number, cy: number, r: number): [number, number][] {
  return ngon(cx, cy, r, 6);
}

export function roundedRect(x: number, y: number, w: number, h: number, r: number): string {
  return `M${x + r} ${y} H${x + w - r} Q${x + w} ${y} ${x + w} ${y + r} V${y + h - r} Q${x + w} ${y + h} ${x + w - r} ${y + h} H${x + r} Q${x} ${y + h} ${x} ${y + h - r} V${y + r} Q${x} ${y} ${x + r} ${y} Z`;
}

export function arrowHead(
  rc: RoughSVG,
  fx: number,
  fy: number,
  tx: number,
  ty: number,
  o: Options,
  size = 12,
): SVGGElement {
  const g = document.createElementNS(SVG_NS, "g");
  const a = Math.atan2(ty - fy, tx - fx);
  const solid: Options = { ...o };
  delete solid.strokeLineDash;
  for (const d of [Math.PI * 0.82, -Math.PI * 0.82]) {
    g.appendChild(rc.line(tx, ty, tx + Math.cos(a + d) * size, ty + Math.sin(a + d) * size, solid));
  }
  return g;
}

function arrowLine(
  rc: RoughSVG,
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  o: Options,
): SVGGElement {
  const g = document.createElementNS(SVG_NS, "g");
  g.appendChild(rc.line(x1, y1, x2, y2, o));
  g.appendChild(arrowHead(rc, x1, y1, x2, y2, o, 7));
  return g;
}
