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
  const horizontal = Math.abs(cb[0] - ca[0]) >= Math.abs(cb[1] - ca[1]);
  const mid: Point[] = horizontal
    ? [
        [(ca[0] + cb[0]) / 2, ca[1]],
        [(ca[0] + cb[0]) / 2, cb[1]],
      ]
    : [
        [ca[0], (ca[1] + cb[1]) / 2],
        [cb[0], (ca[1] + cb[1]) / 2],
      ];
  const [m0, m1] = mid as [Point, Point];
  return [clipToBox(a, m0), m0, m1, clipToBox(b, m1)];
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
