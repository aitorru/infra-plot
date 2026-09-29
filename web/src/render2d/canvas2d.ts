/**
 * The 2D editor surface: an SVG scene drawn either as crisp vector graphics (the `clean` look)
 * or sketched with rough.js (the `sketch` look), coloured by the current theme.
 */
import rough from "roughjs";
import type { Options } from "roughjs/bin/core";
import type { RoughSVG } from "roughjs/bin/svg";
import { fontFamily, GRID, NODE_KINDS, NODE_SIZE, TEXT_SIZES, ZONE_KINDS } from "../model/catalog";
import type { Arrow, Doc, Edge, Line, Look, Node, Note, StrokeStyle, Zone } from "../model/doc";
import { findElement, uniqueId } from "../model/doc";
import {
  type Box,
  bendSegment,
  constrainOrtho,
  docBounds,
  edgePath,
  hashSeed,
  midpoint,
  nodeBox,
  noteBox,
  type Point,
  pointAlong,
  segmentAxis,
  snap,
  zoneBox,
} from "../model/geometry";
import {
  addEdge,
  addNode,
  addZone,
  bendEdgeTo,
  bendTarget,
  moveElement,
  moveLineSegment,
  moveLineVertex,
  zoneContents,
} from "../state/ops";
import type { Store } from "../state/store";
import { mix, onThemeChange, surface, theme } from "../ui/theme";
import { arrowHead, drawGlyph, svgText } from "./glyphs";
import { cleanTile } from "./icons-clean";

const SVG_NS = "http://www.w3.org/2000/svg";

interface Camera {
  x: number;
  y: number;
  zoom: number;
}

type Corner = "nw" | "ne" | "sw" | "se";
type Handle = Corner | "bend" | "vertex" | "segment";

type Drag =
  | { kind: "pan"; sx: number; sy: number; cam: Camera }
  | {
      kind: "move";
      id: string;
      last: Point;
      moved: boolean;
      free: boolean;
      contents: ReadonlySet<string>;
    }
  | { kind: "resize"; id: string; handle: Corner; orig: Box; start: Point; moved: boolean }
  | { kind: "bend"; id: string; moved: boolean }
  | { kind: "vertex"; id: string; index: number; moved: boolean }
  | {
      kind: "segment";
      id: string;
      index: number;
      orig: Point[];
      start: Point;
      moved: boolean;
    }
  | { kind: "zone"; start: Point; cur: Point }
  | { kind: "connect"; from: string; cur: Point }
  | { kind: "line-drag"; start: Point; cur: Point };

type LayerName = "zones" | "lines" | "edges" | "nodes" | "notes" | "overlay";

function el<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs: Record<string, string | number> = {},
): SVGElementTagNameMap[K] {
  const e = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
}

function dash(style: StrokeStyle | undefined): number[] | undefined {
  if (style === "dashed") return [10, 8];
  if (style === "dotted") return [2, 7];
  return undefined;
}

/** Dash arrays for the clean look (used with round caps, so `0` gives dots). */
function cleanDash(style: StrokeStyle | undefined, width: number): string | undefined {
  if (style === "dashed") return `${width * 4} ${width * 3.5}`;
  if (style === "dotted") return `0 ${width * 3}`;
  return undefined;
}

/** Path data for a polyline with its corners rounded by up to `r`. */
export function roundedPolyline(pts: readonly Point[], r: number): string {
  const f = (v: number) => String(Math.round(v * 100) / 100);
  const first = pts[0];
  if (!first) return "";
  let d = `M${f(first[0])} ${f(first[1])}`;
  for (let i = 1; i < pts.length; i++) {
    const p = pts[i] as Point;
    const prev = pts[i - 1] as Point;
    const next = pts[i + 1];
    if (!next) {
      d += `L${f(p[0])} ${f(p[1])}`;
      break;
    }
    const l1 = Math.hypot(p[0] - prev[0], p[1] - prev[1]);
    const l2 = Math.hypot(next[0] - p[0], next[1] - p[1]);
    const k = Math.min(r, l1 / 2, l2 / 2);
    if (k < 0.5 || l1 === 0 || l2 === 0) {
      d += `L${f(p[0])} ${f(p[1])}`;
      continue;
    }
    const a: Point = [p[0] + ((prev[0] - p[0]) * k) / l1, p[1] + ((prev[1] - p[1]) * k) / l1];
    const b: Point = [p[0] + ((next[0] - p[0]) * k) / l2, p[1] + ((next[1] - p[1]) * k) / l2];
    d += `L${f(a[0])} ${f(a[1])}Q${f(p[0])} ${f(p[1])} ${f(b[0])} ${f(b[1])}`;
  }
  return d;
}

/** Moves the end of a polyline back by `by` (so a stroke doesn't poke through an arrow tip). */
function trimEnd(pts: Point[], by: number): Point[] {
  const out = [...pts];
  const last = out[out.length - 1];
  const prev = out[out.length - 2];
  if (!last || !prev) return out;
  const len = Math.hypot(last[0] - prev[0], last[1] - prev[1]);
  if (len <= by) return out;
  out[out.length - 1] = [
    last[0] - ((last[0] - prev[0]) * by) / len,
    last[1] - ((last[1] - prev[1]) * by) / len,
  ];
  return out;
}

/** Corner radius of orthogonal routes in the clean look. */
const CORNER = 10;

/** Crisp strokes with filled arrowheads, for the clean look. */
function cleanStrokes(
  pts: readonly Point[],
  style: StrokeStyle | undefined,
  arrow: Arrow | undefined,
  color: string,
  width = 1.6,
): SVGGElement {
  const g = el("g");
  const head = 10;
  const atEnd = arrow === "end" || arrow === "both";
  const atStart = arrow === "start" || arrow === "both";
  let line = [...pts];
  if (atEnd) line = trimEnd(line, head * 0.8);
  if (atStart) line = trimEnd(line.reverse(), head * 0.8).reverse();
  const path = el("path", {
    d: roundedPolyline(line, CORNER),
    fill: "none",
    stroke: color,
    "stroke-width": width,
    "stroke-linecap": "round",
    "stroke-linejoin": "round",
  });
  const d = cleanDash(style, width);
  if (d) path.setAttribute("stroke-dasharray", d);
  g.appendChild(path);
  const tip = (from: Point | undefined, to: Point | undefined) => {
    if (!from || !to) return;
    const a = Math.atan2(to[1] - from[1], to[0] - from[0]);
    const bx = to[0] - Math.cos(a) * head;
    const by = to[1] - Math.sin(a) * head;
    const px = Math.sin(a) * head * 0.42;
    const py = -Math.cos(a) * head * 0.42;
    g.appendChild(
      el("path", {
        d: `M${to[0]} ${to[1]}L${bx + px} ${by + py}L${bx - px} ${by - py}Z`,
        fill: color,
        stroke: color,
        "stroke-width": 1,
        "stroke-linejoin": "round",
      }),
    );
  };
  if (atEnd) tip(pts[pts.length - 2], pts[pts.length - 1]);
  if (atStart) tip(pts[1], pts[0]);
  return g;
}

function withDash(o: Options, style: StrokeStyle | undefined): Options {
  const d = dash(style);
  return d ? { ...o, strokeLineDash: d } : o;
}

export class Canvas2D {
  readonly svg: SVGSVGElement;
  #store: Store;
  #rc: RoughSVG;
  #camera: Camera = { x: 0, y: 0, zoom: 1 };
  #viewport: SVGGElement;
  #gridPattern: SVGPatternElement;
  #layers: Record<LayerName, SVGGElement>;
  #cache = new Map<string, { key: string; g: SVGGElement }>();
  #drag: Drag | null = null;
  #lineDraft: Point[] | null = null;
  #cursor: Point = [0, 0];
  #space = false;
  #generation = 0;
  #gridDot: SVGCircleElement;

  constructor(host: HTMLElement, store: Store) {
    this.#store = store;
    this.svg = el("svg", { class: "canvas2d", "data-testid": "canvas-2d" });
    this.#rc = rough.svg(this.svg);

    const defs = el("defs");
    this.#gridPattern = el("pattern", {
      id: "grid",
      width: GRID,
      height: GRID,
      patternUnits: "userSpaceOnUse",
    });
    this.#gridDot = el("circle", { cx: 1, cy: 1, r: 1, fill: theme().grid });
    this.#gridPattern.appendChild(this.#gridDot);
    defs.appendChild(this.#gridPattern);
    this.svg.appendChild(defs);
    this.svg.appendChild(
      el("rect", { class: "grid-bg", width: "100%", height: "100%", fill: "url(#grid)" }),
    );

    this.#viewport = el("g", { class: "viewport" });
    this.svg.appendChild(this.#viewport);
    const names: LayerName[] = ["zones", "lines", "edges", "nodes", "notes", "overlay"];
    this.#layers = Object.fromEntries(
      names.map((n) => {
        const g = el("g", { class: `layer-${n}` });
        this.#viewport.appendChild(g);
        return [n, g];
      }),
    ) as Record<LayerName, SVGGElement>;
    host.appendChild(this.svg);

    this.svg.addEventListener("pointerdown", (e) => this.#onDown(e));
    this.svg.addEventListener("pointermove", (e) => this.#onMove(e));
    this.svg.addEventListener("pointerup", (e) => this.#onUp(e));
    this.svg.addEventListener("dblclick", (e) => this.#onDoubleClick(e));
    this.svg.addEventListener("wheel", (e) => this.#onWheel(e), { passive: false });
    window.addEventListener("keydown", (e) => {
      if (e.code === "Space" && !isTyping(e)) {
        this.#space = true;
        this.svg.classList.add("panning");
      }
      if (this.#lineDraft && (e.key === "Enter" || e.key === "Escape")) {
        this.#finishLine(e.key === "Enter");
        e.stopPropagation();
      }
    });
    window.addEventListener("keyup", (e) => {
      if (e.code === "Space") {
        this.#space = false;
        this.svg.classList.remove("panning");
      }
    });
    onThemeChange(() => this.invalidate());
  }

  /** Forces every element to be re-sketched (e.g. once web fonts load). */
  invalidate(): void {
    this.#generation++;
    this.render();
  }

  // ---------------------------------------------------------------- camera

  get zoom(): number {
    return this.#camera.zoom;
  }

  #applyCamera(): void {
    const { x, y, zoom } = this.#camera;
    this.#viewport.setAttribute("transform", `translate(${x} ${y}) scale(${zoom})`);
    this.#gridPattern.setAttribute("patternTransform", `translate(${x} ${y}) scale(${zoom})`);
    this.svg.dataset.zoom = zoom.toFixed(2);
  }

  toWorld(clientX: number, clientY: number): Point {
    const r = this.svg.getBoundingClientRect();
    return [
      (clientX - r.left - this.#camera.x) / this.#camera.zoom,
      (clientY - r.top - this.#camera.y) / this.#camera.zoom,
    ];
  }

  toScreen([x, y]: Point): Point {
    const r = this.svg.getBoundingClientRect();
    return [
      x * this.#camera.zoom + this.#camera.x + r.left,
      y * this.#camera.zoom + this.#camera.y + r.top,
    ];
  }

  zoomAt(factor: number, clientX?: number, clientY?: number): void {
    const r = this.svg.getBoundingClientRect();
    const sx = (clientX ?? r.left + r.width / 2) - r.left;
    const sy = (clientY ?? r.top + r.height / 2) - r.top;
    const zoom = Math.min(4, Math.max(0.1, this.#camera.zoom * factor));
    const k = zoom / this.#camera.zoom;
    this.#camera = { zoom, x: sx - (sx - this.#camera.x) * k, y: sy - (sy - this.#camera.y) * k };
    this.#applyCamera();
  }

  fit(): void {
    const r = this.svg.getBoundingClientRect();
    const b = docBounds(this.#store.doc);
    if (!b || r.width === 0) {
      this.#camera = { x: r.width / 2, y: r.height / 2, zoom: 1 };
    } else {
      const pad = 80;
      const zoom = Math.min(
        2,
        Math.max(0.1, Math.min((r.width - pad * 2) / b.w, (r.height - pad * 2) / b.h)),
      );
      this.#camera = {
        zoom,
        x: r.width / 2 - (b.x + b.w / 2) * zoom,
        y: r.height / 2 - (b.y + b.h / 2) * zoom,
      };
    }
    this.#applyCamera();
  }

  // ---------------------------------------------------------------- render

  get #look(): Look {
    return this.#store.doc.look;
  }

  render(): void {
    const { doc, selection } = this.#store.state;
    this.#gridDot.setAttribute("fill", theme().grid);
    const seen = new Set<string>();
    const next: Record<LayerName, number> = {
      zones: 0,
      lines: 0,
      edges: 0,
      nodes: 0,
      notes: 0,
      overlay: 0,
    };
    const place = (layer: LayerName, id: string, key: string, draw: () => SVGGElement) => {
      seen.add(id);
      const fullKey = `${this.#generation}|${doc.look}|${key}`;
      let entry = this.#cache.get(id);
      if (!entry || entry.key !== fullKey) {
        entry?.g.remove();
        entry = { key: fullKey, g: draw() };
        this.#cache.set(id, entry);
      }
      // Keep document order, but only move nodes that are out of place: detaching the
      // element under the pointer makes the browser drop the next dblclick.
      const g = this.#layers[layer];
      const at = g.children[next[layer]++] ?? null;
      if (at !== entry.g) g.insertBefore(entry.g, at);
    };

    for (const z of doc.zones) place("zones", z.id, JSON.stringify(z), () => this.#drawZone(z));
    for (const l of doc.lines) place("lines", l.id, JSON.stringify(l), () => this.#drawLine(l));
    for (const e of doc.edges) {
      const pts = edgePath(doc, e);
      if (pts) place("edges", e.id, JSON.stringify([e, pts]), () => this.#drawEdge(e, pts));
    }
    for (const n of doc.nodes) place("nodes", n.id, JSON.stringify(n), () => this.#drawNode(n));
    for (const n of doc.notes) place("notes", n.id, JSON.stringify(n), () => this.#drawNote(n));

    for (const [id, entry] of this.#cache) {
      if (!seen.has(id)) {
        entry.g.remove();
        this.#cache.delete(id);
      }
    }
    for (const [id, entry] of this.#cache) entry.g.classList.toggle("selected", id === selection);
    this.#renderOverlay(doc, selection);
    this.#applyCamera();
  }

  /** rough.js options for the sketch look. */
  #base(id: string, color: string, extra: Options = {}): Options {
    return {
      seed: hashSeed(id),
      roughness: 1.1,
      bowing: 1,
      stroke: theme().ink,
      strokeWidth: 1.6,
      fill: surface(color),
      fillStyle: "hachure",
      hachureGap: 5,
      fillWeight: 1.4,
      ...extra,
    };
  }

  #text(
    x: number,
    y: number,
    text: string,
    size: number,
    opts: {
      anchor?: "start" | "middle" | "end";
      color?: string;
      weight?: number;
      spacing?: number;
    } = {},
  ): SVGTextElement {
    return svgText(x, y, text, size, { ...opts, font: fontFamily(this.#look) });
  }

  #group(type: string, id: string): SVGGElement {
    return el("g", { class: `el el-${type}`, "data-id": id, "data-type": type });
  }

  #drawZone(z: Zone): SVGGElement {
    const t = theme();
    const info = ZONE_KINDS[z.kind ?? "generic"];
    const color = z.color ?? info.color;
    const style = z.style ?? info.style;
    const g = this.#group("zone", z.id);
    g.appendChild(
      el("rect", { x: z.x, y: z.y, width: z.w, height: z.h, fill: "transparent", class: "hit" }),
    );
    const tag = info.label.toUpperCase();
    if (this.#look === "clean") {
      const width = 1.5;
      const border = mix(color, t.zoneStroke, t.dark ? 0.45 : 0.6);
      const rect = el("rect", {
        x: z.x,
        y: z.y,
        width: z.w,
        height: z.h,
        rx: 12,
        fill: surface(color, t),
        "fill-opacity": 0.45,
        stroke: border,
        "stroke-width": width,
        "stroke-linecap": "round",
      });
      const d = cleanDash(style, width);
      if (d) rect.setAttribute("stroke-dasharray", d);
      g.appendChild(rect);
      g.appendChild(
        this.#text(z.x + 14, z.y + 17, tag, 10, {
          anchor: "start",
          color: t.muted,
          weight: 650,
          spacing: 0.8,
        }),
      );
      g.appendChild(
        this.#text(z.x + 14, z.y + 35, z.label ?? "", 15, { anchor: "start", weight: 600 }),
      );
      return g;
    }
    g.appendChild(
      this.#rc.rectangle(
        z.x,
        z.y,
        z.w,
        z.h,
        withDash(
          this.#base(z.id, color, {
            fillStyle: "solid",
            strokeWidth: 2,
            roughness: 0.8,
            stroke: t.zoneStroke,
          }),
          style,
        ),
      ),
    );
    g.appendChild(
      this.#text(z.x + 14, z.y + 16, tag, 11, { anchor: "start", color: t.muted, weight: 700 }),
    );
    g.appendChild(this.#text(z.x + 14, z.y + 36, z.label ?? "", 20, { anchor: "start" }));
    return g;
  }

  #drawNode(n: Node): SVGGElement {
    const t = theme();
    const info = NODE_KINDS[n.kind];
    const color = n.color ?? info.color;
    const g = this.#group("node", n.id);
    const b = nodeBox(n);
    g.appendChild(
      el("rect", {
        x: b.x,
        y: b.y,
        width: b.w,
        height: b.h + 28,
        fill: "transparent",
        class: "hit",
      }),
    );
    let label: SVGTextElement;
    if (this.#look === "clean") {
      const tile = surface(color, t);
      g.appendChild(
        cleanTile(n.kind, n.x, n.y, NODE_SIZE, {
          tile,
          border: mix(tile, t.ink, 0.28),
          stroke: mix(color, t.ink, 0.72),
          body: mix(t.panel, color, 0.14),
        }),
      );
      label = this.#text(n.x, b.y + b.h + 14, n.label ?? "", 14, { weight: 500 });
      // A paper-coloured halo keeps labels legible where edges pass underneath.
      for (const [k, v] of Object.entries({
        stroke: t.paper,
        "stroke-width": 4,
        "stroke-linejoin": "round",
        "paint-order": "stroke",
      })) {
        label.setAttribute(k, String(v));
      }
    } else {
      g.appendChild(drawGlyph(this.#rc, n.kind, n.x, n.y, NODE_SIZE, this.#base(n.id, color)));
      label = this.#text(n.x, b.y + b.h + 16, n.label ?? "", 18);
    }
    label.classList.add("label");
    g.appendChild(label);
    return g;
  }

  #strokes(
    id: string,
    pts: readonly Point[],
    style: StrokeStyle | undefined,
    arrow: Arrow | undefined,
    color: string,
  ): SVGGElement {
    return this.#look === "clean"
      ? cleanStrokes(pts, style, arrow, color)
      : this.#sketchStrokes(id, pts, style, arrow, color);
  }

  #sketchStrokes(
    id: string,
    pts: readonly Point[],
    style: StrokeStyle | undefined,
    arrow: Arrow | undefined,
    color: string,
  ): SVGGElement {
    const g = el("g");
    const o = withDash(
      this.#base(id, "none", {
        stroke: color,
        strokeWidth: 2,
        roughness: 0.9,
        fill: undefined as never,
      }),
      style,
    );
    delete o.fill;
    g.appendChild(this.#rc.linearPath(pts as [number, number][], o));
    const first = pts[0];
    const second = pts[1];
    const last = pts[pts.length - 1];
    const beforeLast = pts[pts.length - 2];
    if (first && second && last && beforeLast) {
      if (arrow === "end" || arrow === "both")
        g.appendChild(arrowHead(this.#rc, beforeLast[0], beforeLast[1], last[0], last[1], o));
      if (arrow === "start" || arrow === "both")
        g.appendChild(arrowHead(this.#rc, second[0], second[1], first[0], first[1], o));
    }
    return g;
  }

  #hitPath(pts: readonly Point[]): SVGPolylineElement {
    return el("polyline", {
      points: pts.map((p) => p.join(",")).join(" "),
      fill: "none",
      stroke: "transparent",
      "stroke-width": 16,
      class: "hit",
    });
  }

  #drawEdge(e: Edge, pts: Point[]): SVGGElement {
    const t = theme();
    const g = this.#group("edge", e.id);
    g.appendChild(this.#hitPath(pts));
    // Matches the Rust model: edges point at `to` unless told otherwise.
    g.appendChild(this.#strokes(e.id, pts, e.style, e.arrow ?? "end", e.color ?? t.ink));
    if (e.label) {
      const [mx, my] = pointAlong(pts, 0.5);
      const clean = this.#look === "clean";
      const size = clean ? 12 : 16;
      const w = e.label.length * size * (clean ? 0.58 : 0.55) + (clean ? 18 : 12);
      const h = clean ? 22 : 24;
      g.appendChild(
        el("rect", {
          x: mx - w / 2,
          y: my - h / 2,
          width: w,
          height: h,
          rx: clean ? h / 2 : 6,
          fill: clean ? t.panel : t.paper,
          ...(clean ? { stroke: t.line, "stroke-width": 1 } : { opacity: 0.9 }),
        }),
      );
      g.appendChild(
        this.#text(mx, my, e.label, size, {
          color: e.color ?? t.ink,
          ...(clean ? { weight: 500 } : {}),
        }),
      );
    }
    return g;
  }

  #drawLine(l: Line): SVGGElement {
    const g = this.#group("line", l.id);
    g.appendChild(this.#hitPath(l.points));
    g.appendChild(this.#strokes(l.id, l.points, l.style, l.arrow, l.color ?? theme().ink));
    return g;
  }

  #drawNote(n: Note): SVGGElement {
    const g = this.#group("note", n.id);
    const size = TEXT_SIZES[n.size ?? "m"];
    const b = noteBox(n);
    g.appendChild(
      el("rect", { x: b.x, y: b.y, width: b.w, height: b.h, fill: "transparent", class: "hit" }),
    );
    n.text.split("\n").forEach((line, i) => {
      g.appendChild(
        this.#text(n.x, n.y + size * 0.62 + i * size * 1.25, line, size, {
          anchor: "start",
          color: n.color ?? theme().ink,
        }),
      );
    });
    return g;
  }

  /** A draggable overlay handle; sizes are in screen pixels. */
  #handle(
    kind: Handle,
    [x, y]: Point,
    w: number,
    h: number,
    extra: Record<string, string | number> = {},
  ): SVGRectElement {
    const t = theme();
    const z = this.#camera.zoom;
    return el("rect", {
      x: x - w / z / 2,
      y: y - h / z / 2,
      width: w / z,
      height: h / z,
      rx: Math.min(w, h) / z / 2,
      fill: t.panel,
      stroke: t.accent,
      "stroke-width": 1.5 / z,
      "data-handle": kind,
      class: `handle handle-${kind}`,
      ...extra,
    });
  }

  #renderOverlay(doc: Doc, selection: string | null): void {
    const t = theme();
    const z = this.#camera.zoom;
    const o = this.#layers.overlay;
    o.replaceChildren();
    const found = selection ? findElement(doc, selection) : undefined;
    if (found) {
      let b: Box | undefined;
      if (found.type === "node") b = { ...nodeBox(found.el), h: NODE_SIZE + 28 };
      else if (found.type === "zone") b = zoneBox(found.el);
      else if (found.type === "note") b = noteBox(found.el);
      if (b) {
        const pad = 6;
        o.appendChild(
          el("rect", {
            x: b.x - pad,
            y: b.y - pad,
            width: b.w + pad * 2,
            height: b.h + pad * 2,
            fill: "none",
            stroke: t.accent,
            "stroke-width": 1.5 / z,
            "stroke-dasharray": `${6 / z} ${4 / z}`,
            rx: found.type === "zone" ? 14 : 8,
            class: "selection-box",
          }),
        );
      }
      if (found.type === "zone" && b) {
        const corners: [Corner, number, number][] = [
          ["nw", b.x, b.y],
          ["ne", b.x + b.w, b.y],
          ["sw", b.x, b.y + b.h],
          ["se", b.x + b.w, b.y + b.h],
        ];
        for (const [h, x, y] of corners) {
          const r = this.#handle(h, [x, y], 10, 10, { "data-testid": `handle-${h}` });
          r.setAttribute("rx", String(2 / z));
          o.appendChild(r);
        }
      }
      if (found.type === "edge" || found.type === "line") {
        const pts = found.type === "edge" ? edgePath(doc, found.el) : found.el.points;
        if (pts) {
          o.appendChild(
            el("path", {
              d: roundedPolyline(pts, this.#look === "clean" ? CORNER : 0),
              fill: "none",
              stroke: t.accent,
              "stroke-opacity": 0.3,
              "stroke-width": 8,
              "stroke-linecap": "round",
              "stroke-linejoin": "round",
              "pointer-events": "none",
            }),
          );
        }
      }
      if (found.type === "edge") {
        const seg = bendSegment(doc, found.el);
        if (seg && Math.hypot(seg.q[0] - seg.p[0], seg.q[1] - seg.p[1]) > 1) {
          // The middle segment runs across the main axis and slides along it.
          const across = seg.layout.axis === "x";
          o.appendChild(
            this.#handle("bend", midpoint(seg.p, seg.q), across ? 8 : 22, across ? 22 : 8, {
              "data-testid": "bend-handle",
              cursor: across ? "ew-resize" : "ns-resize",
            }),
          );
        }
      }
      if (found.type === "line") {
        const pts = found.el.points;
        pts.forEach((p, i) => {
          const q = pts[i + 1];
          const axis = q ? segmentAxis(p, q) : null;
          if (!q || !axis) return;
          const len = Math.hypot(q[0] - p[0], q[1] - p[1]) * z;
          if (len < 36) return; // leave short segments to the vertex handles
          o.appendChild(
            this.#handle("segment", midpoint(p, q), axis === "h" ? 18 : 8, axis === "h" ? 8 : 18, {
              "data-index": i,
              "data-testid": "segment-handle",
              cursor: axis === "h" ? "ns-resize" : "ew-resize",
            }),
          );
        });
        pts.forEach((p, i) => {
          o.appendChild(
            this.#handle("vertex", p, 10, 10, {
              "data-index": i,
              "data-testid": "vertex-handle",
              cursor: "move",
            }),
          );
        });
      }
    }

    const d = this.#drag;
    const preview = {
      fill: "none",
      stroke: t.accent,
      "stroke-width": 1.5 / z,
      "stroke-dasharray": `${6 / z} ${4 / z}`,
      "pointer-events": "none",
    };
    if (d?.kind === "zone") {
      const [x, y, w, h] = rectFrom(d.start, d.cur);
      o.appendChild(el("rect", { x, y, width: w, height: h, ...preview, class: "zone-preview" }));
    }
    if (d?.kind === "connect") {
      const b = findElement(doc, d.from);
      const from =
        b?.type === "node"
          ? ([b.el.x, b.el.y] as Point)
          : b?.type === "zone"
            ? ([b.el.x + b.el.w / 2, b.el.y + b.el.h / 2] as Point)
            : d.cur;
      o.appendChild(
        el("line", { x1: from[0], y1: from[1], x2: d.cur[0], y2: d.cur[1], ...preview }),
      );
    }
    if (d?.kind === "line-drag") {
      o.appendChild(
        el("line", { x1: d.start[0], y1: d.start[1], x2: d.cur[0], y2: d.cur[1], ...preview }),
      );
    }
    if (this.#lineDraft) {
      const pts = [...this.#lineDraft, this.#cursor];
      o.appendChild(
        el("polyline", {
          points: pts.map((p) => p.join(",")).join(" "),
          ...preview,
          class: "line-preview",
        }),
      );
    }
  }

  // ---------------------------------------------------------------- input

  #hit(e: {
    clientX: number;
    clientY: number;
  }): { id: string; type: string } | { handle: Handle; index: number } | null {
    const target = document.elementFromPoint(e.clientX, e.clientY);
    const handle = target?.closest<SVGElement>("[data-handle]");
    if (handle?.dataset.handle) {
      return { handle: handle.dataset.handle as Handle, index: Number(handle.dataset.index ?? -1) };
    }
    const g = target?.closest<SVGGElement>("[data-id]");
    if (!g || !this.svg.contains(g)) return null;
    return { id: g.dataset.id ?? "", type: g.dataset.type ?? "" };
  }

  #onDown(e: PointerEvent): void {
    const tool = this.#store.state.tool;
    const world = this.toWorld(e.clientX, e.clientY);
    const free = e.altKey;
    const snapped: Point = [snap(world[0], !free), snap(world[1], !free)];
    this.svg.setPointerCapture(e.pointerId);

    if (e.button === 1 || this.#space || tool.type === "hand") {
      this.#drag = { kind: "pan", sx: e.clientX, sy: e.clientY, cam: { ...this.#camera } };
      return;
    }
    if (e.button !== 0) return;
    const hit = this.#hit(e);

    switch (tool.type) {
      case "select": {
        if (hit && "handle" in hit) {
          const sel = this.#store.state.selection;
          const found = sel ? findElement(this.#store.doc, sel) : undefined;
          const { handle, index } = hit;
          if (found?.type === "zone" && isCorner(handle)) {
            this.#drag = {
              kind: "resize",
              id: found.el.id,
              handle,
              orig: zoneBox(found.el),
              start: world,
              moved: false,
            };
          } else if (found?.type === "edge" && handle === "bend") {
            this.#drag = { kind: "bend", id: found.el.id, moved: false };
          } else if (found?.type === "line" && handle === "vertex") {
            this.#drag = { kind: "vertex", id: found.el.id, index, moved: false };
          } else if (found?.type === "line" && handle === "segment") {
            this.#drag = {
              kind: "segment",
              id: found.el.id,
              index,
              orig: found.el.points.map(([x, y]) => [x, y] as Point),
              start: world,
              moved: false,
            };
          }
          return;
        }
        if (hit) {
          this.#store.set({ selection: hit.id });
          this.#drag = {
            kind: "move",
            id: hit.id,
            last: snapped,
            moved: false,
            free,
            contents: zoneContents(this.#store.doc, hit.id),
          };
        } else {
          this.#store.set({ selection: null });
        }
        return;
      }
      case "node": {
        const kind = tool.kind;
        let id = "";
        this.#store.edit((doc) => {
          id = addNode(doc, kind, snapped).id;
        });
        this.#store.set({ selection: id, tool: { type: "select" } });
        return;
      }
      case "zone":
        this.#drag = { kind: "zone", start: snapped, cur: snapped };
        return;
      case "connect":
        if (hit && "id" in hit && (hit.type === "node" || hit.type === "zone")) {
          this.#drag = { kind: "connect", from: hit.id, cur: world };
        }
        return;
      case "line":
        if (this.#lineDraft) {
          const last = this.#lineDraft[this.#lineDraft.length - 1];
          this.#lineDraft.push(e.shiftKey && last ? constrainOrtho(last, snapped) : snapped);
        } else {
          this.#drag = { kind: "line-drag", start: snapped, cur: snapped };
        }
        this.render();
        return;
      case "note": {
        let id = "";
        this.#store.edit((doc) => {
          id = uniqueId(doc, "note");
          doc.notes.push({ id, x: snapped[0], y: snapped[1], text: "Text", size: "m" });
        });
        this.#store.set({ selection: id, tool: { type: "select" } });
        // After the mousedown that follows, which would otherwise blur the field.
        setTimeout(requestFocusLabel);
        return;
      }
    }
  }

  #onMove(e: PointerEvent): void {
    const world = this.toWorld(e.clientX, e.clientY);
    const snapped: Point = [snap(world[0], !e.altKey), snap(world[1], !e.altKey)];
    const draftEnd = this.#lineDraft?.[this.#lineDraft.length - 1];
    // Shift keeps the segment being drawn horizontal or vertical.
    this.#cursor = e.shiftKey && draftEnd ? constrainOrtho(draftEnd, snapped) : snapped;
    const d = this.#drag;
    if (!d) {
      if (this.#lineDraft) this.render();
      return;
    }
    switch (d.kind) {
      case "pan":
        this.#camera = { ...d.cam, x: d.cam.x + e.clientX - d.sx, y: d.cam.y + e.clientY - d.sy };
        this.#applyCamera();
        return;
      case "move": {
        const p: Point = [snap(world[0], !e.altKey), snap(world[1], !e.altKey)];
        const dx = p[0] - d.last[0];
        const dy = p[1] - d.last[1];
        if (dx === 0 && dy === 0) return;
        if (!d.moved) this.#store.checkpoint();
        d.moved = true;
        d.last = p;
        this.#store.mutate((doc) =>
          moveElement(doc, d.id, dx, dy, e.shiftKey ? new Set() : d.contents),
        );
        return;
      }
      case "resize": {
        const dx = snap(world[0] - d.start[0], !e.altKey);
        const dy = snap(world[1] - d.start[1], !e.altKey);
        const b = { ...d.orig };
        if (d.handle.includes("w")) {
          b.x = d.orig.x + Math.min(dx, d.orig.w - GRID);
          b.w = d.orig.w - (b.x - d.orig.x);
        } else {
          b.w = Math.max(GRID, d.orig.w + dx);
        }
        if (d.handle.includes("n")) {
          b.y = d.orig.y + Math.min(dy, d.orig.h - GRID);
          b.h = d.orig.h - (b.y - d.orig.y);
        } else {
          b.h = Math.max(GRID, d.orig.h + dy);
        }
        if (!d.moved) this.#store.checkpoint();
        d.moved = true;
        this.#store.mutate((doc) => {
          const z = doc.zones.find((z) => z.id === d.id);
          if (z) Object.assign(z, b);
        });
        return;
      }
      case "bend": {
        const bend = bendTarget(this.#store.doc, d.id, world, !e.altKey);
        if (bend === undefined) return;
        if (!d.moved) this.#store.checkpoint();
        d.moved = true;
        this.#store.mutate((doc) => {
          bendEdgeTo(doc, d.id, world, !e.altKey);
        });
        return;
      }
      case "vertex": {
        const line = this.#store.doc.lines.find((l) => l.id === d.id);
        const cur = line?.points[d.index];
        if (!line || !cur) return;
        const neighbour = line.points[d.index - 1] ?? line.points[d.index + 1];
        const p = e.shiftKey && neighbour ? constrainOrtho(neighbour, snapped) : snapped;
        if (p[0] === cur[0] && p[1] === cur[1]) return;
        if (!d.moved) this.#store.checkpoint();
        d.moved = true;
        this.#store.mutate((doc) => moveLineVertex(doc, d.id, d.index, p));
        return;
      }
      case "segment": {
        const p = d.orig[d.index];
        const q = d.orig[d.index + 1];
        const line = this.#store.doc.lines.find((l) => l.id === d.id);
        const cur = line?.points[d.index];
        if (!p || !q || !cur) return;
        const axis = segmentAxis(p, q);
        const dx = axis === "h" ? 0 : snap(p[0] + world[0] - d.start[0], !e.altKey) - p[0];
        const dy = axis === "v" ? 0 : snap(p[1] + world[1] - d.start[1], !e.altKey) - p[1];
        if (p[0] + dx === cur[0] && p[1] + dy === cur[1]) return;
        if (!d.moved) this.#store.checkpoint();
        d.moved = true;
        this.#store.mutate((doc) => moveLineSegment(doc, d.id, d.index, d.orig, dx, dy));
        return;
      }
      case "zone":
        d.cur = this.#cursor;
        this.render();
        return;
      case "line-drag":
        d.cur = e.shiftKey ? constrainOrtho(d.start, snapped) : snapped;
        this.render();
        return;
      case "connect":
        d.cur = world;
        this.render();
        return;
    }
  }

  #onUp(e: PointerEvent): void {
    const d = this.#drag;
    this.#drag = null;
    if (this.svg.hasPointerCapture(e.pointerId)) this.svg.releasePointerCapture(e.pointerId);
    if (!d) return;
    switch (d.kind) {
      case "zone": {
        let [x, y, w, h] = rectFrom(d.start, d.cur);
        if (w < GRID * 2 || h < GRID * 2) {
          [x, y, w, h] = [d.start[0], d.start[1], 320, 220];
        }
        const tool = this.#store.state.tool;
        const kind = tool.type === "zone" ? tool.kind : "generic";
        let id = "";
        this.#store.edit((doc) => {
          id = addZone(doc, kind, x, y, w, h).id;
        });
        this.#store.set({ selection: id, tool: { type: "select" } });
        return;
      }
      case "connect": {
        const hit = this.#hit(e);
        if (
          hit &&
          "id" in hit &&
          (hit.type === "node" || hit.type === "zone") &&
          hit.id !== d.from
        ) {
          let id = "";
          this.#store.edit((doc) => {
            id = addEdge(doc, d.from, hit.id)?.id ?? "";
          });
          this.#store.set({ selection: id || null });
        } else {
          this.render();
        }
        return;
      }
      case "line-drag": {
        const [a, b] = [d.start, d.cur];
        if (Math.hypot(b[0] - a[0], b[1] - a[1]) >= GRID) {
          this.#commitLine([a, b]);
        } else {
          // A click: start a multi-point polyline, finished with Enter / double-click.
          this.#lineDraft = [a];
          this.render();
        }
        return;
      }
      default:
        this.render();
    }
  }

  #onDoubleClick(e: MouseEvent): void {
    if (this.#lineDraft) {
      this.#finishLine(true);
      return;
    }
    const hit = this.#hit(e);
    if (hit && "id" in hit) {
      this.#store.set({ selection: hit.id });
      requestFocusLabel();
    }
  }

  #finishLine(commit: boolean): void {
    const pts = this.#lineDraft ?? [];
    this.#lineDraft = null;
    // A double-click adds the same point twice.
    const dedup = pts.filter(
      (p, i) => i === 0 || p[0] !== pts[i - 1]?.[0] || p[1] !== pts[i - 1]?.[1],
    );
    if (commit && dedup.length >= 2) this.#commitLine(dedup);
    else this.render();
  }

  #commitLine(pts: Point[]): void {
    let id = "";
    this.#store.edit((doc) => {
      id = uniqueId(doc, "line");
      doc.lines.push({ id, points: pts.map((p) => [p[0], p[1]]), style: "solid", arrow: "end" });
    });
    this.#store.set({ selection: id, tool: { type: "select" } });
  }

  #onWheel(e: WheelEvent): void {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      this.zoomAt(Math.exp(-e.deltaY * 0.01), e.clientX, e.clientY);
    } else {
      const dx = e.shiftKey && e.deltaX === 0 ? e.deltaY : e.deltaX;
      const dy = e.shiftKey && e.deltaX === 0 ? 0 : e.deltaY;
      this.#camera = { ...this.#camera, x: this.#camera.x - dx, y: this.#camera.y - dy };
      this.#applyCamera();
    }
  }

  cancelDrafts(): boolean {
    if (!this.#lineDraft && !this.#drag) return false;
    this.#lineDraft = null;
    this.#drag = null;
    this.render();
    return true;
  }

  // ---------------------------------------------------------------- export

  /** A standalone SVG of the whole diagram, cropped to its content. */
  exportSvg(fontCss: string): string {
    const b = docBounds(this.#store.doc) ?? { x: 0, y: 0, w: 400, h: 300 };
    const pad = 40;
    const clone = this.svg.cloneNode(true) as SVGSVGElement;
    clone.querySelector(".layer-overlay")?.replaceChildren();
    clone.querySelector(".grid-bg")?.remove();
    for (const s of clone.querySelectorAll(".selected")) s.classList.remove("selected");
    clone.querySelector(".viewport")?.removeAttribute("transform");
    clone.setAttribute("xmlns", SVG_NS);
    clone.setAttribute("viewBox", `${b.x - pad} ${b.y - pad} ${b.w + pad * 2} ${b.h + pad * 2}`);
    clone.setAttribute("width", String(b.w + pad * 2));
    clone.setAttribute("height", String(b.h + pad * 2));
    const style = el("style");
    style.textContent = fontCss;
    clone.insertBefore(style, clone.firstChild);
    const bg = el("rect", {
      x: b.x - pad,
      y: b.y - pad,
      width: b.w + pad * 2,
      height: b.h + pad * 2,
      fill: theme().paper,
    });
    clone.insertBefore(bg, clone.querySelector(".viewport"));
    return new XMLSerializer().serializeToString(clone);
  }
}

function isCorner(h: Handle): h is Corner {
  return h === "nw" || h === "ne" || h === "sw" || h === "se";
}

function rectFrom(a: Point, b: Point): [number, number, number, number] {
  return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.abs(b[0] - a[0]), Math.abs(b[1] - a[1])];
}

export function isTyping(e: Event): boolean {
  const t = e.target;
  return (
    t instanceof HTMLInputElement ||
    t instanceof HTMLTextAreaElement ||
    t instanceof HTMLSelectElement
  );
}

export function requestFocusLabel(): void {
  window.dispatchEvent(new CustomEvent("infraplot:focus-label"));
}
