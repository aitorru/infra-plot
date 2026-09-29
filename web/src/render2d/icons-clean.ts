/**
 * Line icons for every node kind, as SVG path data on a 24-unit grid (Lucide-like: round caps,
 * round joins, drawn to sit inside 2..22). Shared by the `clean` canvas look, the palette and,
 * for kinds without a bespoke rough.js glyph, the `sketch` look.
 */
import type { NodeKind } from "../model/doc";

/**
 * - `body`: closed shape filled with the icon's body colour (and stroked).
 * - `line`: stroke only.
 * - `solid`: filled with the stroke colour (small dots, bolts).
 */
export type IconPartKind = "body" | "line" | "solid";

export interface IconPart {
  readonly kind: IconPartKind;
  readonly d: string;
  /** Stroke dash pattern in grid units. */
  readonly dash?: readonly number[];
}

export const ICON_GRID = 24;

const n = (v: number): string => String(Math.round(v * 1000) / 1000);

function rect(x: number, y: number, w: number, h: number, r = 0): string {
  if (r <= 0) return `M${n(x)} ${n(y)}h${n(w)}v${n(h)}h${n(-w)}z`;
  return (
    `M${n(x + r)} ${n(y)}h${n(w - 2 * r)}a${n(r)} ${n(r)} 0 0 1 ${n(r)} ${n(r)}` +
    `v${n(h - 2 * r)}a${n(r)} ${n(r)} 0 0 1 ${n(-r)} ${n(r)}h${n(-(w - 2 * r))}` +
    `a${n(r)} ${n(r)} 0 0 1 ${n(-r)} ${n(-r)}v${n(-(h - 2 * r))}a${n(r)} ${n(r)} 0 0 1 ${n(r)} ${n(-r)}z`
  );
}

function circle(cx: number, cy: number, r: number): string {
  return `M${n(cx - r)} ${n(cy)}a${n(r)} ${n(r)} 0 1 0 ${n(2 * r)} 0a${n(r)} ${n(r)} 0 1 0 ${n(-2 * r)} 0z`;
}

function polygon(pts: readonly (readonly [number, number])[]): string {
  return `${pts.map(([x, y], i) => `${i === 0 ? "M" : "L"}${n(x)} ${n(y)}`).join("")}z`;
}

function ngon(cx: number, cy: number, r: number, sides: number): [number, number][] {
  return Array.from({ length: sides }, (_, i) => {
    const a = (i / sides) * Math.PI * 2 - Math.PI / 2;
    return [cx + Math.cos(a) * r, cy + Math.sin(a) * r];
  });
}

function gear(cx: number, cy: number, outer: number, inner: number, teeth: number): string {
  const pts: [number, number][] = [];
  const step = (Math.PI * 2) / teeth;
  const half = step * 0.22;
  for (let i = 0; i < teeth; i++) {
    const a = i * step - Math.PI / 2;
    for (const [ang, r] of [
      [a - step / 2 + half * 0.6, inner],
      [a - half, outer],
      [a + half, outer],
      [a + step / 2 - half * 0.6, inner],
    ] as const) {
      pts.push([cx + Math.cos(ang) * r, cy + Math.sin(ang) * r]);
    }
  }
  return polygon(pts);
}

const body = (d: string): IconPart => ({ kind: "body", d });
const line = (d: string, dash?: number[]): IconPart =>
  dash ? { kind: "line", d, dash } : { kind: "line", d };
const solid = (d: string): IconPart => ({ kind: "solid", d });
const dot = (x: number, y: number, r = 1): IconPart => solid(circle(x, y, r));

export const CLEAN_ICONS: Readonly<Record<NodeKind, readonly IconPart[]>> = {
  // ---------------------------------------------------------------- generic
  component: [
    body(rect(5, 4, 15, 16, 2)),
    body(rect(2.5, 7.5, 5, 3, 0.8)),
    body(rect(2.5, 13.5, 5, 3, 0.8)),
    line("M11 9.5h5.5M11 14.5h3.5"),
  ],

  // ---------------------------------------------------------------- compute
  service: [
    body(rect(2.5, 4.5, 19, 15, 3)),
    line(
      "M9.5 8.5c-1 0-1.6.5-1.6 1.5v.8c0 .8-.4 1.2-1.2 1.2.8 0 1.2.4 1.2 1.2v.8c0 1 .6 1.5 1.6 1.5",
    ),
    line(
      "M14.5 8.5c1 0 1.6.5 1.6 1.5v.8c0 .8.4 1.2 1.2 1.2-.8 0-1.2.4-1.2 1.2v.8c0 1-.6 1.5-1.6 1.5",
    ),
  ],
  server: [
    body(rect(3.5, 3, 17, 8, 2)),
    body(rect(3.5, 13, 17, 8, 2)),
    dot(7, 7),
    dot(7, 17),
    line("M11 7h6M11 17h6"),
  ],
  vm: [
    line("M7 7.5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-2.5"),
    body(rect(3, 7.5, 14, 13.5, 2)),
    line(rect(6, 11, 8, 6.5, 1), [1.6, 1.6]),
  ],
  container: [
    body("M12 2.5l8.5 4.75v9.5L12 21.5l-8.5-4.75v-9.5z"),
    line("M3.5 7.25L12 12l8.5-4.75M12 12v9.5"),
  ],
  pod: [
    body(polygon(ngon(12, 12, 10, 6))),
    line("M12 7.5l4.2 2.3v4.6L12 16.7l-4.2-2.3V9.8z"),
    line("M7.8 9.8L12 12.1l4.2-2.3M12 12.1v4.6"),
  ],
  k8s: [
    body(polygon(ngon(12, 12.3, 10, 7))),
    line(circle(12, 12.3, 4.2)),
    line(
      ngon(12, 12.3, 7.4, 7)
        .map(([x, y]) => {
          const k = 1.6 / 7.4;
          return `M${n(12 + (x - 12) * k)} ${n(12.3 + (y - 12.3) * k)}L${n(x)} ${n(y)}`;
        })
        .join(""),
    ),
    dot(12, 12.3, 1.3),
  ],
  function: [
    body(circle(12, 12, 9.5)),
    line("M7.5 7h1.2c1 0 1.6.5 2 1.4L15 17h1.5M11.6 10.8L8 17"),
  ],
  worker: [body(gear(12, 12, 10, 7.6, 8)), line(circle(12, 12, 3))],
  scheduler: [
    body(circle(12, 12.5, 9)),
    line("M12 7.5v5l3.5 2"),
    line("M4.5 3.5L2.5 5.5M19.5 3.5l2 2"),
  ],
  gpu: [
    body(rect(5.5, 5.5, 13, 13, 2)),
    line(rect(9, 9, 6, 6, 1)),
    line(
      "M9 2.5v3M12 2.5v3M15 2.5v3M9 18.5v3M12 18.5v3M15 18.5v3" +
        "M2.5 9h3M2.5 12h3M2.5 15h3M18.5 9h3M18.5 12h3M18.5 15h3",
    ),
  ],

  // ---------------------------------------------------------------- network
  proxy: [
    body("M12 2l10 10-10 10L2 12z"),
    line("M7.5 10h9M14.5 8l2 2-2 2"),
    line("M16.5 14h-9M9.5 12l-2 2 2 2"),
  ],
  "load-balancer": [
    body(circle(12, 12, 9.5)),
    line("M5.5 12h5M10.5 12l3-4.5h4M10.5 12l3 4.5h4M10.5 12h7"),
    line("M16 6l1.5 1.5L16 9M16 10.5l1.5 1.5-1.5 1.5M16 15l1.5 1.5L16 18"),
  ],
  "api-gateway": [
    body("M4 21V11a8 8 0 0 1 16 0v10z"),
    line("M2.5 21h19"),
    line("M10 12.5L7.5 15l2.5 2.5M14 12.5l2.5 2.5-2.5 2.5"),
  ],
  gateway: [
    body("M5 21V11a7 7 0 0 1 14 0v10z"),
    line("M3 21h18"),
    line("M1.5 15.5h19M17.5 12.5l3 3-3 3"),
  ],
  router: [
    body(circle(12, 12, 9.5)),
    line("M12 10V4.8M10 6.8l2-2 2 2M12 14v5.2M10 17.2l2 2 2-2"),
    line("M10 12H4.8M6.8 10l-2 2 2 2M14 12h5.2M17.2 10l2 2-2 2"),
  ],
  switch: [
    body(rect(2, 6.5, 20, 11, 2.5)),
    line("M6 10h11M15 8.3l2 1.7-2 1.7"),
    line("M18 14H7M9 12.3L7 14l2 1.7"),
  ],
  vpn: [
    body("M12 2.5l8 3v6c0 4.8-3.4 8.4-8 10-4.6-1.6-8-5.2-8-10v-6z"),
    line(rect(8.5, 11, 7, 5.5, 1)),
    line("M10 11V9.3a2 2 0 0 1 4 0V11"),
  ],
  cdn: [
    body(circle(12, 12, 9.5)),
    line(
      "M12 2.5c-2.6 2.6-3.9 5.8-3.9 9.5s1.3 6.9 3.9 9.5c2.6-2.6 3.9-5.8 3.9-9.5S14.6 5.1 12 2.5z",
    ),
    line("M2.5 12h19M4 7h16M4 17h16"),
  ],
  dns: [
    line("M12 2.5v19M8.5 21.5h7"),
    body("M5 4.5h11.5l2.8 2.5-2.8 2.5H5z"),
    body("M19 12H7.5l-2.8 2.5L7.5 17H19z"),
  ],

  // ---------------------------------------------------------------- data
  database: [
    body("M4 5.5c0-1.66 3.58-3 8-3s8 1.34 8 3v13c0 1.66-3.58 3-8 3s-8-1.34-8-3z"),
    line("M4 5.5c0 1.66 3.58 3 8 3s8-1.34 8-3M4 12c0 1.66 3.58 3 8 3s8-1.34 8-3"),
  ],
  cache: [
    body("M4 6.5c0-1.4 3.6-2.5 8-2.5s8 1.1 8 2.5v11c0 1.4-3.6 2.5-8 2.5s-8-1.1-8-2.5z"),
    line("M4 6.5c0 1.4 3.6 2.5 8 2.5s8-1.1 8-2.5"),
    solid("M13.4 10.5l-3.9 4.9h3l-1 3.6 3.9-4.9h-3z"),
  ],
  queue: [
    body(rect(2, 5.5, 20, 9.5, 2)),
    line("M7 5.5v9.5M12 5.5v9.5M17 5.5v9.5"),
    line("M3.5 19h17M18 16.8l2.5 2.2-2.5 2.2"),
  ],
  stream: [
    line("M2.5 7c1.9-1.6 3.8-1.6 5.7 0s3.8 1.6 5.7 0 3.8-1.6 5.7 0"),
    line("M2.5 12c1.9-1.6 3.8-1.6 5.7 0s3.8 1.6 5.7 0 3.8-1.6 5.7 0M19.5 9.8l2 2.2-2.4 1.6"),
    line("M2.5 17c1.9-1.6 3.8-1.6 5.7 0s3.8 1.6 5.7 0 3.8-1.6 5.7 0"),
  ],
  storage: [
    body(
      "M2 13.5L5.2 5.6A2 2 0 0 1 7 4.5h10a2 2 0 0 1 1.8 1.1L22 13.5v5a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2z",
    ),
    line("M2 13.5h20"),
    dot(6, 17),
    dot(9.5, 17),
  ],
  bucket: [
    body(
      "M3.5 6.5c0-1.4 3.8-2.5 8.5-2.5s8.5 1.1 8.5 2.5L18 19.3c-.2 1-1.1 1.7-2 1.7H8c-.9 0-1.8-.7-2-1.7z",
    ),
    line("M3.5 6.5c0 1.4 3.8 2.5 8.5 2.5s8.5-1.1 8.5-2.5"),
  ],
  warehouse: [
    body("M2.5 21V9.5L12 4l9.5 5.5V21z"),
    line("M6.5 21v-8.5h11V21M6.5 15.3h11M6.5 18.2h11"),
  ],
  search: [
    body(circle(10.5, 10.5, 7)),
    line("M15.5 15.5L21 21"),
    line("M7.3 8.7a3.6 3.6 0 0 1 3.2-1.9"),
  ],

  // ---------------------------------------------------------------- security
  firewall: [
    body(rect(3, 4, 18, 16, 1.5)),
    line("M3 9.3h18M3 14.7h18"),
    line("M9 4v5.3M15 4v5.3M6 9.3v5.4M12 9.3v5.4M18 9.3v5.4M9 14.7V20M15 14.7V20"),
  ],
  identity: [
    body(rect(2.5, 5, 19, 14, 2)),
    line(circle(8.5, 10.5, 2)),
    line("M5.5 15.8c.5-1.5 1.7-2.3 3-2.3s2.5.8 3 2.3"),
    line("M14 10h4.5M14 13.5h3"),
  ],
  secrets: [
    body(circle(7.5, 16, 5)),
    line("M11.1 12.5L20.5 3.1M17.3 6.3l2.6 2.6M14.8 8.8l2 2"),
    dot(6.3, 17.2, 1.2),
  ],

  // ---------------------------------------------------------------- operations
  monitoring: [body(rect(2.5, 4, 19, 16, 2.5)), line("M5.5 12.5h3l2-4.5 3 9 2-4.5h3")],
  logging: [
    body("M14.5 2.5h-8A1.5 1.5 0 0 0 5 4v16a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 20V7z"),
    line("M14.5 2.5V7H19"),
    line("M8.5 11h7M8.5 14h7M8.5 17h4.5"),
  ],
  "ci-cd": [
    line(
      "M12 12c-2-2.5-3.5-4-5.5-4a4 4 0 0 0 0 8c2 0 3.5-1.5 5.5-4s3.5-4 5.5-4a4 4 0 0 1 0 8c-2 0-3.5-1.5-5.5-4z",
    ),
    line("M15.2 6.3l2.3 1.7-2 2"),
  ],
  registry: [
    body(rect(2.5, 12.5, 8.5, 8.5, 1.2)),
    body(rect(13, 12.5, 8.5, 8.5, 1.2)),
    body(rect(7.75, 3, 8.5, 8.5, 1.2)),
    line("M2.5 15.5h8.5M13 15.5h8.5M7.75 6h8.5"),
  ],
  notification: [
    body("M6 16.5V11a6 6 0 0 1 12 0v5.5l1.8 2.2H4.2z"),
    line("M10 21a2.1 2.1 0 0 0 4 0"),
    line("M12 3.2V5"),
  ],

  // ---------------------------------------------------------------- clients
  user: [body(circle(12, 7.5, 4.2)), body("M4.5 21v-.8a6 6 0 0 1 6-6h3a6 6 0 0 1 6 6v.8z")],
  client: [
    body(rect(4.5, 4.5, 15, 11, 1.5)),
    body("M4 15.5h16l1.8 3.3a.8.8 0 0 1-.7 1.2H2.9a.8.8 0 0 1-.7-1.2z"),
  ],
  mobile: [body(rect(6.5, 2.5, 11, 19, 2.2)), line("M10.5 5.3h3"), dot(12, 18.3, 1)],
  browser: [
    body(rect(2.5, 4, 19, 16, 2)),
    line("M2.5 8.5h19M6.5 12h11M6.5 15.5h7"),
    dot(5.3, 6.25, 0.75),
    dot(7.7, 6.25, 0.75),
    dot(10.1, 6.25, 0.75),
  ],
  internet: [body("M17.5 19H9a7 7 0 1 1 6.7-9h1.8a4.5 4.5 0 1 1 0 9z")],
  external: [
    line("M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"),
    line("M14.5 3H21v6.5M21 3L11 13"),
  ],
};

/**
 * Rewrites path data with a uniform scale `k` and translation, keeping every command (rough.js
 * sketches paths point by point, so its jitter must be applied at the final size).
 */
export function transformPath(d: string, k: number, tx: number, ty: number): string {
  const tokens = d.match(/[a-zA-Z]|-?(?:\d+\.?\d*|\.\d+)(?:e-?\d+)?/g) ?? [];
  const out: string[] = [];
  let cmd = "";
  let i = 0;
  const num = () => Number(tokens[i++]);
  const x = (v: number, abs: boolean) => n(abs ? v * k + tx : v * k);
  const y = (v: number, abs: boolean) => n(abs ? v * k + ty : v * k);
  while (i < tokens.length) {
    const t = tokens[i] as string;
    if (/[a-zA-Z]/.test(t)) {
      cmd = t;
      i++;
      out.push(cmd);
      if (cmd === "z" || cmd === "Z") continue;
    }
    const abs = cmd === cmd.toUpperCase();
    switch (cmd.toUpperCase()) {
      case "M":
      case "L":
      case "T":
        out.push(x(num(), abs), y(num(), abs));
        break;
      case "H":
        out.push(x(num(), abs));
        break;
      case "V":
        out.push(y(num(), abs));
        break;
      case "C":
        for (let j = 0; j < 3; j++) out.push(x(num(), abs), y(num(), abs));
        break;
      case "S":
      case "Q":
        for (let j = 0; j < 2; j++) out.push(x(num(), abs), y(num(), abs));
        break;
      case "A":
        out.push(n(num() * k), n(num() * k), String(num()), String(num()), String(num()));
        out.push(x(num(), abs), y(num(), abs));
        break;
      default:
        i++;
    }
  }
  return out.join(" ");
}

const SVG_NS = "http://www.w3.org/2000/svg";

/** Paints for a clean node tile. Any CSS colour works, including `var(--…)` / `color-mix()`. */
export interface TilePaint {
  /** Tile background. */
  readonly tile: string;
  /** Tile border. */
  readonly border: string;
  /** Icon strokes and `solid` parts. */
  readonly stroke: string;
  /** Fill of the icon's `body` parts. */
  readonly body: string;
}

function svg<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs: Record<string, string | number>,
  style: Record<string, string> = {},
): SVGElementTagNameMap[K] {
  const e = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  // Inline styles (rather than presentation attributes) so CSS variables resolve.
  for (const [k, v] of Object.entries(style)) e.style.setProperty(k, v);
  return e;
}

/**
 * A rounded tile tinted with the node colour and the kind's line icon on top, centred on
 * (cx, cy) in an `s`×`s` box. `strokePx` is the icon stroke width in the box's units.
 */
export function cleanTile(
  kind: NodeKind,
  cx: number,
  cy: number,
  s: number,
  paint: TilePaint,
  strokePx = 2,
  borderPx = 1.5,
): SVGGElement {
  const g = svg("g", { class: "tile" });
  const inset = s * 0.06;
  const size = s - inset * 2;
  g.appendChild(
    svg(
      "rect",
      {
        x: n(cx - size / 2),
        y: n(cy - size / 2),
        width: n(size),
        height: n(size),
        rx: n(s * 0.2),
      },
      { fill: paint.tile, stroke: paint.border, "stroke-width": String(borderPx) },
    ),
  );
  g.appendChild(cleanGlyph(kind, cx, cy, s * 0.6, paint, strokePx));
  return g;
}

/** Just the line icon, `size` wide, centred on (cx, cy). */
export function cleanGlyph(
  kind: NodeKind,
  cx: number,
  cy: number,
  size: number,
  paint: Pick<TilePaint, "stroke" | "body">,
  strokePx = 2,
): SVGGElement {
  const k = size / ICON_GRID;
  const g = svg(
    "g",
    {
      transform: `translate(${n(cx - size / 2)} ${n(cy - size / 2)}) scale(${n(k)})`,
      "stroke-linecap": "round",
      "stroke-linejoin": "round",
    },
    { stroke: paint.stroke, "stroke-width": n(strokePx / k), fill: "none" },
  );
  for (const part of CLEAN_ICONS[kind]) {
    const fill = part.kind === "body" ? paint.body : part.kind === "solid" ? paint.stroke : "none";
    const style: Record<string, string> = { fill };
    if (part.dash) style["stroke-dasharray"] = part.dash.join(" ");
    g.appendChild(svg("path", { d: part.d }, style));
  }
  return g;
}
