import { GRID, NODE_SIZE, TEXT_SIZES } from "./catalog";
import type { Doc, Edge, Node, Note, Zone } from "./doc";

export type Point = readonly [number, number];

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function snap(v: number, enabled = true): number {
  return enabled ? Math.round(v / GRID) * GRID : v;
}

export function nodeBox(n: Node): Box {
  return { x: n.x - NODE_SIZE / 2, y: n.y - NODE_SIZE / 2, w: NODE_SIZE, h: NODE_SIZE };
}

export function zoneBox(z: Zone): Box {
  return { x: z.x, y: z.y, w: z.w, h: z.h };
}

export function noteBox(n: Note): Box {
  const size = TEXT_SIZES[n.size ?? "m"];
  const lines = n.text.split("\n");
  const w = Math.max(...lines.map((l) => l.length), 1) * size * 0.55;
  return { x: n.x, y: n.y, w, h: lines.length * size * 1.25 };
}

export function center(b: Box): Point {
  return [b.x + b.w / 2, b.y + b.h / 2];
}

export function contains(outer: Box, inner: Box): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.w <= outer.x + outer.w &&
    inner.y + inner.h <= outer.y + outer.h
  );
}

export function containsPoint(b: Box, [x, y]: Point): boolean {
  return x >= b.x && y >= b.y && x <= b.x + b.w && y <= b.y + b.h;
}

export function targetBox(doc: Doc, id: string): Box | undefined {
  const n = doc.nodes.find((n) => n.id === id);
  if (n) return nodeBox(n);
  const z = doc.zones.find((z) => z.id === id);
  return z ? zoneBox(z) : undefined;
}

/** Where the ray from the centre of `b` towards `p` leaves `b`. */
export function clipToBox(b: Box, p: Point, pad = 6): Point {
  const [cx, cy] = center(b);
  const dx = p[0] - cx;
  const dy = p[1] - cy;
  if (dx === 0 && dy === 0) return [cx, cy];
  const hw = b.w / 2 + pad;
  const hh = b.h / 2 + pad;
  const t = Math.min(
    dx === 0 ? Infinity : hw / Math.abs(dx),
    dy === 0 ? Infinity : hh / Math.abs(dy),
  );
  if (t >= 1) return [cx, cy];
  return [cx + dx * t, cy + dy * t];
}

/**
 * How an orthogonal edge is laid out: the main axis it travels along, and the span (`lo` → `hi`,
 * in that axis' coordinate) over which `bend` slides the middle segment. When the boxes are
 * apart along the main axis the span is the gap between their facing sides, so the first and
 * last segments leave and enter the boxes perpendicular to their borders; otherwise it runs
 * between the centres.
 */
export interface OrthoLayout {
  axis: "x" | "y";
  lo: number;
  hi: number;
  /** Centres of the `from` and `to` boxes. */
  a: Point;
  b: Point;
  /** Whether the boxes are apart along the axis (`lo`/`hi` are then on their borders). */
  apart: boolean;
}

const EDGE_PAD = 6;

export function orthoLayout(from: Box, to: Box): OrthoLayout {
  const a = center(from);
  const b = center(to);
  const gapX = Math.max(to.x - (from.x + from.w), from.x - (to.x + to.w));
  const gapY = Math.max(to.y - (from.y + from.h), from.y - (to.y + to.h));
  let axis: "x" | "y";
  if (gapX > 0 && gapY <= 0) axis = "x";
  else if (gapY > 0 && gapX <= 0) axis = "y";
  else axis = Math.abs(b[0] - a[0]) >= Math.abs(b[1] - a[1]) ? "x" : "y";
  const i = axis === "x" ? 0 : 1;
  const gap = axis === "x" ? gapX : gapY;
  if (gap > EDGE_PAD * 2) {
    const dir = Math.sign(b[i] - a[i]) || 1;
    const halfA = (axis === "x" ? from.w : from.h) / 2 + EDGE_PAD;
    const halfB = (axis === "x" ? to.w : to.h) / 2 + EDGE_PAD;
    return { axis, lo: a[i] + dir * halfA, hi: b[i] - dir * halfB, a, b, apart: true };
  }
  return { axis, lo: a[i], hi: b[i], a, b, apart: false };
}

/** Clamps a bend fraction to 0..1, defaulting to the middle. */
export function bendOf(e: Pick<Edge, "bend">): number {
  const v = e.bend ?? 0.5;
  return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0.5;
}

/** The bend fraction that puts the middle segment at `coord` (along the layout's axis). */
export function bendAt(layout: OrthoLayout, coord: number): number {
  const span = layout.hi - layout.lo;
  if (span === 0) return 0.5;
  const t = Math.min(1, Math.max(0, (coord - layout.lo) / span));
  return Math.round(t * 10000) / 10000;
}

/** The polyline an edge follows, already clipped to its endpoints' borders. */
export function edgePath(doc: Doc, e: Edge): Point[] | undefined {
  const a = targetBox(doc, e.from);
  const b = targetBox(doc, e.to);
  if (!a || !b) return undefined;
  const ca = center(a);
  const cb = center(b);
  if (e.route !== "orthogonal") {
    return [clipToBox(a, cb), clipToBox(b, ca)];
  }
  const l = orthoLayout(a, b);
  const m = l.lo + (l.hi - l.lo) * bendOf(e);
  const [m0, m1]: [Point, Point] =
    l.axis === "x"
      ? [
          [m, ca[1]],
          [m, cb[1]],
        ]
      : [
          [ca[0], m],
          [cb[0], m],
        ];
  if (l.apart) {
    const start: Point = l.axis === "x" ? [l.lo, ca[1]] : [ca[0], l.lo];
    const end: Point = l.axis === "x" ? [l.hi, cb[1]] : [cb[0], l.hi];
    return simplify([start, m0, m1, end]);
  }
  return simplify([clipToBox(a, m0, EDGE_PAD), m0, m1, clipToBox(b, m1, EDGE_PAD)]);
}

/** The middle (bendable) segment of an orthogonal edge, if it has one. */
export function bendSegment(
  doc: Doc,
  e: Edge,
): { p: Point; q: Point; layout: OrthoLayout } | undefined {
  if (e.route !== "orthogonal") return undefined;
  const a = targetBox(doc, e.from);
  const b = targetBox(doc, e.to);
  if (!a || !b) return undefined;
  const layout = orthoLayout(a, b);
  const m = layout.lo + (layout.hi - layout.lo) * bendOf(e);
  const j = layout.axis === "x" ? 1 : 0;
  const p: Point = layout.axis === "x" ? [m, layout.a[j]] : [layout.a[j], m];
  const q: Point = layout.axis === "x" ? [m, layout.b[j]] : [layout.b[j], m];
  return { p, q, layout };
}

/** Drops repeated points and middle points of straight runs. */
export function simplify(pts: readonly Point[]): Point[] {
  const out: Point[] = [];
  for (const p of pts) {
    const last = out[out.length - 1];
    if (last && Math.abs(last[0] - p[0]) < 1e-6 && Math.abs(last[1] - p[1]) < 1e-6) continue;
    const prev = out[out.length - 2];
    if (prev && last) {
      const cross = (last[0] - prev[0]) * (p[1] - prev[1]) - (last[1] - prev[1]) * (p[0] - prev[0]);
      const dot = (last[0] - prev[0]) * (p[0] - last[0]) + (last[1] - prev[1]) * (p[1] - last[1]);
      if (Math.abs(cross) < 1e-6 && dot >= 0) out.pop();
    }
    out.push(p);
  }
  return out;
}

/** Whether segment p→q is horizontal (`h`), vertical (`v`) or neither. */
export function segmentAxis(p: Point, q: Point): "h" | "v" | null {
  if (p[0] === q[0] && p[1] === q[1]) return null;
  if (p[1] === q[1]) return "h";
  if (p[0] === q[0]) return "v";
  return null;
}

/** `p` pulled onto the horizontal or vertical through `from`, whichever is closer. */
export function constrainOrtho(from: Point, p: Point): Point {
  return Math.abs(p[0] - from[0]) >= Math.abs(p[1] - from[1]) ? [p[0], from[1]] : [from[0], p[1]];
}

export function midpoint(p: Point, q: Point): Point {
  return [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2];
}

export function pathLength(pts: readonly Point[]): number {
  let len = 0;
  for (let i = 1; i < pts.length; i++) {
    const p = pts[i - 1] as Point;
    const q = pts[i] as Point;
    len += Math.hypot(q[0] - p[0], q[1] - p[1]);
  }
  return len;
}

/** Point at fraction `t` (0..1) of the polyline's length. */
export function pointAlong(pts: readonly Point[], t: number): Point {
  const total = pathLength(pts);
  let remaining = total * t;
  for (let i = 1; i < pts.length; i++) {
    const p = pts[i - 1] as Point;
    const q = pts[i] as Point;
    const seg = Math.hypot(q[0] - p[0], q[1] - p[1]);
    if (remaining <= seg || i === pts.length - 1) {
      const k = seg === 0 ? 0 : Math.min(remaining / seg, 1);
      return [p[0] + (q[0] - p[0]) * k, p[1] + (q[1] - p[1]) * k];
    }
    remaining -= seg;
  }
  return pts[0] ?? [0, 0];
}

export function docBounds(doc: Doc): Box | undefined {
  const boxes: Box[] = [
    ...doc.zones.map(zoneBox),
    ...doc.nodes.map((n) => {
      const b = nodeBox(n);
      return { ...b, h: b.h + 30 }; // label
    }),
    ...doc.notes.map(noteBox),
    ...doc.lines.map((l) => {
      const xs = l.points.map((p) => p[0]);
      const ys = l.points.map((p) => p[1]);
      const x = Math.min(...xs);
      const y = Math.min(...ys);
      return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
    }),
  ];
  if (boxes.length === 0) return undefined;
  const x = Math.min(...boxes.map((b) => b.x));
  const y = Math.min(...boxes.map((b) => b.y));
  const r = Math.max(...boxes.map((b) => b.x + b.w));
  const btm = Math.max(...boxes.map((b) => b.y + b.h));
  return { x, y, w: r - x, h: btm - y };
}

/** How many zones enclose `b` — used to stack zones in 3D. */
export function zoneDepth(doc: Doc, b: Box, exceptId?: string): number {
  return doc.zones.filter((z) => z.id !== exceptId && contains(zoneBox(z), b)).length;
}

/** Stable 31-bit hash, used as rough.js seed so sketches are reproducible. */
export function hashSeed(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 1 || 1;
}
