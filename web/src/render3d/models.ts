/**
 * 3D models for every node kind.
 *
 * Each kind is described once as a list of parts (primitives from `Kit`, each in a `Tone`
 * of the node colour). The `clean` look renders them smooth and bevelled with a crisp
 * screen-space silhouette; the `sketch` look renders faceted toon solids traced with
 * jittered "ink" strokes. Geometry is merged per tone and cached per kind, so every node
 * of a kind shares it.
 */
import * as THREE from "three";
import { LineMaterial } from "three/addons/lines/LineMaterial.js";
import { LineSegments2 } from "three/addons/lines/LineSegments2.js";
import { LineSegmentsGeometry } from "three/addons/lines/LineSegmentsGeometry.js";
import { mergeGeometries, mergeVertices } from "three/addons/utils/BufferGeometryUtils.js";
import { NODE_SIZE } from "../model/catalog";
import type { Look, NodeKind, StrokeStyle } from "../model/doc";
import { theme } from "../ui/theme";
import { arrowShape, gearShape, Kit, polygon } from "./shapes";
import { contactShadow, darker, fill, outlineMaterial, solid, type Tone, toneColor } from "./style";

export { darker } from "./style";

/** Deterministic PRNG (mulberry32) so sketches don't change between renders. */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export interface InkOptions {
  color?: string;
  /** Stroke width in world units (or pixels with `pixels`). */
  width?: number;
  style?: StrokeStyle | undefined;
  /** How far strokes wander from the true edge; 0 draws each segment once, exactly. */
  jitter?: number;
  /** Width in screen pixels instead of world units. */
  pixels?: boolean;
}

export function dashPattern(style: StrokeStyle | undefined): [number, number] | undefined {
  if (style === "dashed") return [10, 8];
  if (style === "dotted") return [2, 7];
  return undefined;
}

/**
 * Strokes for a set of segments (pairs of points). With jitter, each segment is drawn
 * twice, slightly off and overshooting its ends, like a quick pen stroke.
 */
export function ink(segments: readonly number[], random: () => number, o: InkOptions = {}) {
  const jitter = o.jitter ?? 0.9;
  const out: number[] = [];
  if (jitter === 0) {
    out.push(...segments);
  } else {
    const j = () => (random() - 0.5) * 2 * jitter;
    for (let pass = 0; pass < 2; pass++) {
      for (let i = 0; i + 5 < segments.length; i += 6) {
        const a = new THREE.Vector3(segments[i], segments[i + 1], segments[i + 2]);
        const b = new THREE.Vector3(segments[i + 3], segments[i + 4], segments[i + 5]);
        const len = a.distanceTo(b);
        if (len < 0.01) continue;
        const dir = b.clone().sub(a).divideScalar(len);
        const over = Math.min(2, len * 0.08);
        a.addScaledVector(dir, -over * random());
        b.addScaledVector(dir, over * random());
        out.push(a.x + j(), a.y + j(), a.z + j(), b.x + j(), b.y + j(), b.z + j());
      }
    }
  }
  const geo = new LineSegmentsGeometry();
  geo.setPositions(out);
  const dash = dashPattern(o.style);
  const mat = new LineMaterial({
    color: o.color ?? theme().ink,
    linewidth: o.width ?? 1.6,
    worldUnits: !o.pixels,
    dashed: dash !== undefined,
    dashSize: dash?.[0] ?? 1,
    gapSize: dash?.[1] ?? 1,
  });
  const lines = new LineSegments2(geo, mat);
  if (dash) lines.computeLineDistances();
  lines.userData.ink = true;
  return lines;
}

/** Crease edges of `geo` as a flat segment list, for `ink`. */
export function creases(geo: THREE.BufferGeometry, threshold = 25): number[] {
  const edges = new THREE.EdgesGeometry(geo, threshold);
  const out = Array.from(edges.getAttribute("position").array);
  edges.dispose();
  return out;
}

// ---------------------------------------------------------------- parts

interface Part {
  geo: THREE.BufferGeometry;
  tone?: Tone;
  /** Whether the part contributes to the silhouette / ink outline (default true). */
  outline?: boolean;
  /** Crease angle for the sketch outline; lower draws more edges. */
  threshold?: number;
}

const P = (geo: THREE.BufferGeometry, tone: Tone = "base", outline = true): Part => ({
  geo,
  tone,
  outline,
});
/** A small surface detail: no outline, so it reads as printed on the body. */
const D = (geo: THREE.BufferGeometry, tone: Tone = "dark"): Part => ({ geo, tone, outline: false });

const TAU = Math.PI * 2;

/** A glyph given as unit-square polygon points, scaled to `w` × `h`. */
function glyph(points: [number, number][], w: number, h: number): THREE.Shape {
  return polygon(points.map(([x, y]) => [x * w, y * h]));
}

/** Rotates a geometry around the vertical axis through the origin. */
function spin(geo: THREE.BufferGeometry, angle: number): THREE.BufferGeometry {
  return geo.rotateY(angle);
}

function lathe(k: Kit, profile: [number, number][]): THREE.BufferGeometry {
  return new THREE.LatheGeometry(
    profile.map(([x, y]) => new THREE.Vector2(x, y)),
    k.clean ? 40 : 10,
  );
}

function parts(kind: NodeKind, H: number, k: Kit): Part[] {
  switch (kind) {
    // -------------------------------------------------------------- generic / compute
    case "component":
      return [
        P(k.box(54, H - 3, 50, { r: 6 })),
        D(k.box(38, 3, 34, { y: H - 3.5, r: 1.5 }), "light"),
        P(k.box(14, 6, 7, { x: -20, y: H * 0.22, z: 26, r: 1.5 }), "dark"),
        P(k.box(14, 6, 7, { x: -20, y: H * 0.56, z: 26, r: 1.5 }), "dark"),
      ];
    case "service":
      return [
        P(k.box(58, H - 5, 58, { r: 7 })),
        P(k.box(46, 5, 46, { y: H - 6, r: 3 }), "light"),
        D(k.prism(6, 11, 2, { y: H - 1.2 }), "dark"),
        D(k.box(14, 3, 1.5, { x: -16, y: H * 0.3, z: 29 }), "led"),
      ];
    case "server": {
      const bay = (H - 14) / 4;
      return [
        P(k.box(44, H, 54, { r: 4 })),
        D(k.box(36, 1.6, 46, { y: H - 0.8, r: 0.8 }), "light"),
        ...[0, 1, 2, 3].flatMap((i): Part[] => {
          const y = 7 + i * bay;
          return [
            D(k.box(36, bay - 3, 2, { y, z: 26.6, r: 1 }), "dark"),
            D(k.box(3, 3, 1.5, { x: 13, y: y + bay / 2 - 3, z: 27.8, r: 0.6 }), "led"),
            D(k.box(16, 1.2, 1.2, { x: -6, y: y + bay / 2 - 2.1, z: 27.8, r: 0.4 }), "deep"),
          ];
        }),
      ];
    }
    case "vm":
      return [
        P(k.box(62, 8, 62, { r: 3 }), "dark"),
        P(k.box(50, H - 12, 50, { y: 8, r: 6 })),
        P(k.box(40, 4, 40, { y: H - 4.5, r: 2 }), "light"),
        D(k.box(34, (H - 12) * 0.55, 1.6, { y: 8 + (H - 12) * 0.2, z: 25, r: 1.2 }), "screen"),
        D(k.box(6, 6, 1.6, { x: 0, y: 8 + (H - 12) * 0.36, z: 25.6, r: 0.8 }), "glass"),
      ];
    case "container":
      return [
        P(k.box(68, 3, 42, { r: 1 }), "dark"),
        P(k.box(66, H - 3, 40, { y: 3, r: 2 })),
        ...[-24, -16, -8, 0, 8, 16, 24].map((x) =>
          D(k.box(3, H - 10, 2, { x, y: 7, z: 20.2, r: 0.8 }), "dark"),
        ),
        D(k.box(2, H - 10, 34, { x: 33.2, y: 7, r: 0.8 }), "dark"),
        D(k.box(1.6, H - 14, 1.6, { x: 34.6, y: 9, z: -5, r: 0.5 }), "metal"),
        D(k.box(1.6, H - 14, 1.6, { x: 34.6, y: 9, z: 5, r: 0.5 }), "metal"),
      ];
    case "pod":
      return [
        P(k.prism(6, 28, H - 5, { bevel: 2 })),
        P(k.prism(6, 20, 5, { y: H - 5.5, bevel: 1.5 }), "light"),
        D(k.prism(6, 8, 2, { y: H - 1 }), "dark"),
      ];
    case "k8s": {
      const top = H - 4;
      return [
        P(k.prism(7, 34, top, { bevel: 2.5 })),
        P(k.ring(14, 2.4, { rx: -Math.PI / 2, y: top + 1.2 }), "light", false),
        P(k.cyl(4.5, 4.5, { y: top - 1 }), "light", false),
        ...Array.from({ length: 7 }, (_, i) =>
          D(spin(k.box(2.6, 2.4, 18, { y: top, z: 9 }), (i / 7) * TAU + 0.2), "light"),
        ),
      ];
    }
    case "function": {
      const lambda: [number, number][] = [
        [-0.32, 1],
        [-0.1, 1],
        [0.45, 0],
        [0.24, 0],
        [0.005, 0.42],
        [-0.24, 0],
        [-0.45, 0],
        [-0.107, 0.62],
      ];
      return [
        P(k.box(56, 12, 56, { r: 5 })),
        D(k.box(46, 1.5, 46, { y: 11.4, r: 1 }), "light"),
        P(k.upright(glyph(lambda, 36, H - 13), 10, { y: 13.5 }), "dark"),
      ];
    }
    case "worker":
      return [
        P(k.flat(gearShape(29, 10, 0), 12, { bevel: 1.5 })),
        P(k.cyl(10, 3, { y: 11.5 }), "light", false),
        P(k.upright(gearShape(15, 8, 4), 7, { y: 13 + 15, bevel: 1 }), "dark"),
      ];
    case "scheduler": {
      const c = H / 2 + 1;
      return [
        P(k.box(28, 3, 14, { r: 1.5 }), "dark"),
        P(k.disc(H / 2 - 2, 8, { y: c, bevel: 2.5 })),
        D(k.disc(H / 2 - 5.5, 1, { y: c, z: 4.2 }), "light"),
        ...[0, 1, 2, 3].map((i) =>
          D(
            k
              .box(1.6, 3.2, 0.8, { y: -1.6, z: 0 })
              .translate(0, H / 2 - 9, 0)
              .rotateZ((i * TAU) / 4)
              .translate(0, c, 4.9),
            "dark",
          ),
        ),
        D(k.box(2.2, H / 2 - 9, 1, { y: c, z: 5.1, r: 0.6 }), "dark"),
        D(k.box(9, 2.2, 1, { x: 4.5, y: c - 1.1, z: 5.1, r: 0.6 }), "dark"),
        P(k.ball(4, { x: -11, y: H - 3 }), "dark", false),
        P(k.ball(4, { x: 11, y: H - 3 }), "dark", false),
      ];
    }
    case "gpu":
      return [
        P(k.box(70, 3, 46, { r: 1 }), "deep"),
        P(k.box(66, H - 6, 44, { y: 3, r: 4 })),
        D(k.box(40, 2, 2.5, { x: -8, y: 0.5, z: 23.5, r: 0.5 }), "warn"),
        ...[-16, 16].flatMap((x): Part[] => [
          D(k.cyl(14, 1.4, { x, y: H - 3.4 }), "deep"),
          D(k.cyl(4, 2.4, { x, y: H - 3.2 }), "light"),
          ...Array.from({ length: 6 }, (_, i) =>
            D(
              spin(k.box(3.4, 1, 10, { y: H - 2.4, z: 6.5, rz: 0.3 }), (i / 6) * TAU).translate(
                x,
                0,
                0,
              ),
              "light",
            ),
          ),
        ]),
        ...[-14, -7, 0, 7, 14].map((z) => D(k.box(1.2, H - 12, 3, { x: 33.4, y: 6, z }), "dark")),
      ];

    // -------------------------------------------------------------- network
    case "proxy":
      return [
        P(k.cyl(32, H - 5, { bevel: 3 })),
        P(k.cyl(24, 3, { y: H - 6 }), "light"),
        D(k.flat(arrowShape(30, 10), 2.5, { y: H - 3, z: -7 }), "dark"),
        D(k.flat(arrowShape(30, 10), 2.5, { y: H - 3, z: 7, ry: Math.PI }), "dark"),
      ];
    case "load-balancer":
      return [
        P(k.cyl(34, H - 5, { bevel: 3 })),
        P(k.cyl(27, 3, { y: H - 6 }), "light"),
        D(k.cyl(5, 3, { x: -12, y: H - 3.2 }), "dark"),
        ...[-0.75, 0, 0.75].map((a) =>
          D(
            spin(k.flat(arrowShape(20, 8), 2.5, { x: 15, y: H - 3.2 }), a).translate(-12, 0, 0),
            "dark",
          ),
        ),
      ];
    case "api-gateway": {
      const chevron: [number, number][] = [
        [0, 0.5],
        [1, 0],
        [1, 0.28],
        [0.42, 0.5],
        [1, 0.72],
        [1, 1],
      ];
      const slash: [number, number][] = [
        [0.62, 1],
        [1, 1],
        [0.38, 0],
        [0, 0],
      ];
      const y = H - 10;
      return [
        P(k.box(16, H - 11, 22, { x: -24, r: 2.5 })),
        P(k.box(16, H - 11, 22, { x: 24, r: 2.5 })),
        P(k.box(68, 12, 26, { y: H - 12, r: 3 }), "light"),
        D(k.upright(glyph(chevron, 7, 8), 2, { x: -13, y, z: 13.4 }), "dark"),
        D(k.upright(glyph(slash, 6, 8), 2, { x: -3, y, z: 13.4 }), "dark"),
        D(k.upright(glyph(chevron, 7, 8), 2, { x: 13, y, z: 13.4, ry: Math.PI }), "dark"),
      ];
    }
    case "gateway": {
      const s = new THREE.Shape();
      const top = H - 10;
      s.moveTo(-30, 0);
      s.lineTo(-30, H - 10);
      s.quadraticCurveTo(-30, H, -20, H);
      s.lineTo(20, H);
      s.quadraticCurveTo(30, H, 30, H - 10);
      s.lineTo(30, 0);
      s.lineTo(15, 0);
      s.lineTo(15, top - 15);
      s.absarc(0, top - 15, 15, 0, Math.PI, false);
      s.lineTo(-15, 0);
      s.closePath();
      return [
        P(k.box(66, 3, 28, { r: 1.5 }), "dark"),
        P(k.upright(s, 22, { y: 3, bevel: 1.5 })),
        D(k.box(9, 8, 1.5, { y: H - 7, z: 11.2, r: 1 }), "light"),
      ];
    }
    case "router": {
      const top = H - 4;
      const arrows: [number, boolean][] = [
        [0, true],
        [Math.PI / 2, false],
        [Math.PI, true],
        [-Math.PI / 2, false],
      ];
      return [
        P(k.cyl(30, top, { bevel: 4 })),
        P(k.cyl(24, 2, { y: top - 1 }), "light"),
        ...arrows.map(([a, out]) =>
          D(
            spin(k.flat(arrowShape(13, 8), 2, { x: 13, y: top + 0.6, ry: out ? 0 : Math.PI }), a),
            "dark",
          ),
        ),
        P(k.cyl(1.4, 18, { x: -14, y: top - 2, z: -18 }), "metal", false),
        P(k.cyl(1.4, 18, { x: 14, y: top - 2, z: -18 }), "metal", false),
        P(k.ball(2.4, { x: -14, y: top + 16, z: -18 }), "led", false),
        P(k.ball(2.4, { x: 14, y: top + 16, z: -18 }), "led", false),
      ];
    }
    case "switch":
      return [
        P(k.box(70, H, 46, { r: 3 })),
        D(k.box(62, 1.2, 38, { y: H - 0.6, r: 0.6 }), "light"),
        ...Array.from({ length: 8 }, (_, i): Part[] => {
          const x = -27 + i * 7.2;
          return [
            D(k.box(5.2, 5, 1.6, { x, y: H / 2 - 3.5, z: 22.8, r: 0.6 }), "deep"),
            D(
              k.box(1.6, 1.4, 1, { x, y: H / 2 + 2.6, z: 23.2, r: 0.3 }),
              i % 3 === 2 ? "warn" : "led",
            ),
          ];
        }).flat(),
      ];
    case "vpn": {
      const s = new THREE.Shape();
      s.moveTo(0, 0);
      s.quadraticCurveTo(21, 7, 21, 24);
      s.lineTo(21, H - 2);
      s.quadraticCurveTo(10, H - 3, 0, H - 7);
      s.quadraticCurveTo(-10, H - 3, -21, H - 2);
      s.lineTo(-21, 24);
      s.quadraticCurveTo(-21, 7, 0, 0);
      const ly = H * 0.3;
      return [
        P(k.box(36, 3, 20, { r: 1.5 }), "dark"),
        P(k.upright(s, 11, { y: 2, bevel: 1.8 })),
        P(k.box(15, 12, 4, { y: ly, z: 6.5, r: 1.8 }), "light"),
        P(k.ring(4.6, 1.3, { arc: Math.PI, y: ly + 11.5, z: 6.5 }), "metal", false),
        D(k.disc(1.8, 1, { y: ly + 6.5, z: 8.6 }), "deep"),
        D(k.box(1.2, 3, 1, { y: ly + 3, z: 8.6 }), "deep"),
      ];
    }
    case "cdn": {
      const c = H - 24;
      return [
        P(k.cyl(17, 4, { bevel: 1.5 }), "dark"),
        P(k.cyl(3, c - 20, { y: 3 }), "metal", false),
        { geo: k.ball(23, { y: c }), tone: "base", threshold: 12 },
        D(k.ring(23.4, 1.1, { rx: Math.PI / 2, y: c }), "light"),
        D(k.ring(23.4, 1.1, { ry: Math.PI / 4, y: c }), "light"),
        D(k.ring(23.4, 1.1, { ry: -Math.PI / 4, y: c }), "light"),
        D(k.ring(17.5, 1, { rx: Math.PI / 2, y: c + 14 }), "light"),
        D(k.ring(17.5, 1, { rx: Math.PI / 2, y: c - 14 }), "light"),
      ];
    }
    case "dns":
      return [
        P(k.box(60, H - 5, 46, { r: 4 })),
        ...[-12, 0, 12].flatMap((z): Part[] => [
          D(k.box(40, 3, 8, { x: 5, y: H - 6, z, r: 1.4 }), "light"),
          D(k.cyl(3.4, 3.4, { x: -20, y: H - 6, z }), "dark"),
        ]),
      ];

    // -------------------------------------------------------------- data
    case "database": {
      const band = (H - 6) / 3;
      return [
        P(k.cyl(26, H - 1), "deep", false),
        ...[0, 1, 2].map((i) => P(k.cyl(28, band, { y: i * (band + 3), bevel: 3.5 }))),
        D(k.cyl(21, 1, { y: H - 0.2 }), "light"),
      ];
    }
    case "cache": {
      const bolt: [number, number][] = [
        [0.2, 1],
        [-0.38, 0.4],
        [-0.02, 0.4],
        [-0.22, 0],
        [0.4, 0.62],
        [0.05, 0.62],
      ];
      const disc = (H - 16) / 2 - 1;
      return [
        P(k.cyl(28, 2 * disc + 1), "deep", false),
        P(k.cyl(30, disc, { bevel: 3 })),
        P(k.cyl(30, disc, { y: disc + 2, bevel: 3 })),
        P(k.upright(glyph(bolt, 30, H - 2 * disc - 2), 7, { y: 2 * disc + 1.5 }), "light"),
      ];
    }
    case "queue":
      return [
        P(k.box(78, 3, 36, { r: 1.5 }), "dark"),
        ...[-26, -10, 6, 22].map((x, i) =>
          P(k.box(13, H - 5, 28, { x, y: 3, r: 3 }), i === 3 ? "light" : "base"),
        ),
        D(k.flat(arrowShape(10, 12), 2, { x: 34, y: 3 }), "deep"),
      ];
    case "stream": {
      // Three flat ribbons flowing along +x, each ending in an arrow head.
      const ribbon = (phase: number): THREE.Shape => {
        const upper: [number, number][] = [];
        const lower: [number, number][] = [];
        for (let x = -32; x <= 18; x += 2.5) {
          const c = Math.sin((x - 18) / 8 + phase) * 3 * Math.min(1, (18 - x) / 12);
          upper.push([x, c + 3.2]);
          lower.push([x, c - 3.2]);
        }
        return polygon([...upper, [18, 7.5], [31, 0], [18, -7.5], ...lower.reverse()]);
      };
      return [
        P(k.box(70, 3, 54, { r: 1.5 }), "dark"),
        ...[-16, 0, 16].map((z, i) =>
          P(
            k.flat(ribbon(i * 1.3), 5, { y: 3 + (i === 1 ? 9 : 1 + i * 2), z, bevel: 1.4 }),
            i === 1 ? "light" : "base",
          ),
        ),
      ];
    }
    case "storage":
      return [
        P(k.box(56, H, 42, { r: 4 })),
        D(k.box(48, 1.4, 34, { y: H - 0.8, r: 0.6 }), "light"),
        ...[0, 1, 2, 3].flatMap((i): Part[] => {
          const x = -18 + i * 12;
          return [
            D(k.box(10, H - 14, 1.6, { x, y: 6, z: 21, r: 1 }), "dark"),
            D(k.box(2.4, 2.4, 1, { x, y: H - 12, z: 22, r: 0.5 }), "led"),
          ];
        }),
      ];
    case "bucket": {
      const rb = 20;
      const rt = 29;
      const r = (y: number) => rb + ((rt - rb) * y) / H;
      return [
        P(
          lathe(k, [
            [0, 0],
            [rb - 1.5, 0],
            [rb, 1.5],
            [rt - 0.2, H - 2],
            [rt + 1.2, H - 1],
            [rt, H],
            [rt - 2, H - 0.6],
            [r(3) - 2, 3],
            [0, 3],
          ]),
        ),
        D(
          lathe(k, [
            [0, 3.2],
            [r(3) - 2.2, 3.2],
            [rt - 2.2, H - 0.8],
          ]),
          "deep",
        ),
        D(k.ring(r(H * 0.32) + 0.4, 1.2, { rx: Math.PI / 2, y: H * 0.32 }), "dark"),
        P(k.ring(rt, 1.2, { arc: Math.PI, y: H - 1, rx: -0.55 }), "metal", false),
      ];
    }
    case "warehouse": {
      const wall = H * 0.56;
      const roof = polygon([
        [-28, 0],
        [28, 0],
        [0, H - wall],
      ]);
      return [
        P(k.box(64, wall, 48, { r: 2 })),
        P(k.upright(roof, 70, { y: wall - 0.5, ry: Math.PI / 2, bevel: 1.5 }), "dark"),
        D(k.box(26, wall * 0.78, 1.6, { x: -10, z: 24, r: 0.8 }), "deep"),
        ...[0.2, 0.4, 0.6].map((f) =>
          D(k.box(24, 0.8, 1, { x: -10, y: wall * 0.78 * f + 2, z: 24.9 }), "dark"),
        ),
        D(k.box(10, 8, 1.4, { x: 18, y: wall * 0.4, z: 24.2, r: 0.8 }), "glass"),
        D(k.box(1.4, 8, 12, { x: 32.2, y: wall * 0.4, z: 8, r: 0.8 }), "glass"),
        D(k.box(1.4, 8, 12, { x: 32.2, y: wall * 0.4, z: -10, r: 0.8 }), "glass"),
      ];
    }
    case "search": {
      const c = new THREE.Vector3(-7, H - 19, 0);
      const d = new THREE.Vector3(1, -1, 0).normalize();
      const a = c.clone().addScaledVector(d, 18);
      const b = c.clone().addScaledVector(d, 31);
      const lean = (g: THREE.BufferGeometry) => g.rotateX(-0.25).translate(0, 1, 3);
      return [
        P(lean(k.ring(15, 3.6, { x: c.x, y: c.y })), "base"),
        D(lean(k.disc(13.5, 1.6, { x: c.x, y: c.y })), "glass"),
        P(lean(k.tube([a, b], 4.2)), "dark"),
      ];
    }
    case "firewall": {
      const row = (H - 2) / 3;
      return [
        P(k.box(60, H - 5, 12, { r: 1 }), "dark", false),
        ...[0, 1, 2].flatMap((i): Part[] =>
          (i % 2 === 0
            ? [
                [-15.5, 30],
                [15.5, 30],
              ]
            : [
                [-23.5, 14],
                [0, 30],
                [23.5, 14],
              ]
          ).map(([x = 0, w = 0], j) =>
            P(
              k.box(w - 1.4, row - 1.4, 20, { x, y: i * row + 0.7, r: 2 }),
              (i + j) % 3 === 1 ? "dark" : "base",
            ),
          ),
        ),
      ];
    }

    // -------------------------------------------------------------- security / ops
    case "identity":
      return [
        P(k.box(28, 3, 16, { r: 1.5 }), "dark"),
        P(k.box(38, H - 6, 5, { y: 2, r: 4 })),
        P(k.box(12, 5, 3, { y: H - 6, r: 1.5 }), "metal", false),
        D(k.disc(7.5, 1.2, { y: H * 0.58, z: 2.8 }), "light"),
        D(k.ball(3.2, { y: H * 0.6 + 1, z: 3.2 }), "dark"),
        D(k.box(22, 2.4, 1.2, { y: H * 0.3, z: 2.8, r: 0.6 }), "dark"),
        D(k.box(15, 2.4, 1.2, { x: -3.5, y: H * 0.2, z: 2.8, r: 0.6 }), "dark"),
      ];
    case "secrets": {
      const cy = 4 + (H - 4) / 2;
      return [
        ...[-20, 20].flatMap((x) =>
          [-17, 17].map((z) => P(k.box(6, 4, 6, { x, z, r: 1 }), "deep", false)),
        ),
        P(k.box(52, H - 4, 46, { y: 4, r: 4 })),
        D(k.box(42, H - 14, 1.6, { y: 9, z: 23, r: 2 }), "light"),
        P(k.disc(7.5, 3, { y: cy, z: 24.5, bevel: 1 }), "metal", false),
        D(k.disc(3.5, 1.4, { y: cy, z: 26.4 }), "deep"),
        D(k.box(12, 1.8, 1.8, { x: 13, y: cy - 0.9, z: 25 }), "metal"),
        D(k.box(2.4, 6, 2.6, { x: -21.5, y: 11, z: 24 }), "dark"),
        D(k.box(2.4, 6, 2.6, { x: -21.5, y: H - 13, z: 24 }), "dark"),
      ];
    }
    case "monitoring": {
      const bars = [14, 24, 19, 34];
      const pulse: [number, number][] = [
        [-27, 32],
        [-12, 32],
        [-8, 42],
        [-3, 22],
        [2, 48],
        [7, 32],
        [27, 32],
      ];
      return [
        P(k.box(64, 4, 46, { r: 2 }), "dark"),
        P(k.box(62, H - 4, 3, { y: 4, z: -19, r: 1.5 }), "light"),
        ...bars.map((h, i) => P(k.box(9, h, 9, { x: -21 + i * 14, y: 4, z: 4, r: 2 }))),
        D(
          k.tube(
            pulse.map(([x, y]) => new THREE.Vector3(x, (y * (H - 4)) / 52, -17)),
            1.3,
          ),
          "led",
        ),
      ];
    }
    case "logging":
      return [
        P(k.box(40, H - 6, 3, { x: -9, y: 6, z: -12, r: 2 }), "dark"),
        P(k.box(40, H - 3, 3, { x: -4.5, y: 3, z: -5, r: 2 }), "base"),
        P(k.box(40, H, 3, { z: 2, r: 2 }), "light"),
        ...[0.78, 0.62, 0.46, 0.3, 0.14].map((f, i) =>
          D(
            k.box(i === 0 ? 18 : 28 - (i % 2) * 8, 1.8, 1, {
              x: i === 0 ? -5 : -(i % 2) * 4,
              y: H * f,
              z: 3.8,
              r: 0.5,
            }),
            "dark",
          ),
        ),
      ];
    case "ci-cd": {
      const cy = 4 + 14;
      return [
        P(k.box(62, 4, 26, { r: 2 }), "dark"),
        P(k.ring(11, 3.2, { x: -11, y: cy })),
        P(k.ring(11, 3.2, { x: 11, y: cy })),
        P(k.cone(4.8, 9, { rz: -Math.PI / 2, x: 6, y: cy + 11 }), "light"),
        P(k.cone(4.8, 9, { rz: Math.PI / 2, x: -6, y: cy - 11 }), "light"),
      ];
    }
    case "registry": {
      const s = (H - 2) / 2;
      const crate = (x: number, y: number, z: number, tone: Tone): Part[] => [
        P(k.box(28, s - 1, 28, { x, y, z, r: 2 }), tone),
        D(k.box(29.4, 3, 29.4, { x, y: y + (s - 1) / 2 - 1.5, z, r: 1 }), "dark"),
      ];
      return [...crate(-15, 0, 0, "base"), ...crate(15, 0, 0, "base"), ...crate(0, s, 0, "light")];
    }
    case "notification":
      return [
        P(
          lathe(k, [
            [0, 5],
            [18, 5],
            [19.5, 6.4],
            [17.5, 8.4],
            [15, 13],
            [13.8, 21],
            [12.6, 28],
            [10, 32.5],
            [5.5, 34.6],
            [0, 35.2],
          ]),
        ),
        D(k.ring(17.8, 1.4, { rx: Math.PI / 2, y: 6.6 }), "dark"),
        P(k.ball(4.2, { y: 4.2 }), "dark", false),
        P(k.ball(2.8, { y: 36.6 }), "dark", false),
        P(k.ball(5, { x: 12, y: 31, z: 7 }), "alert", false),
      ];

    // -------------------------------------------------------------- clients
    case "user":
      return [
        P(
          lathe(k, [
            [0, 0],
            [19, 0],
            [21, 2],
            [20.5, H * 0.4],
            [17, H * 0.52],
            [9, H * 0.58],
            [0, H * 0.59],
          ]),
        ),
        P(k.ball(12, { y: H - 12.5 }), "light"),
      ];
    case "client":
      return [
        P(k.box(26, 3, 16, { z: -5, r: 1.5 }), "dark"),
        P(k.box(6, 12, 4, { y: 2, z: -7, r: 1 }), "dark"),
        P(k.box(66, H - 12, 4, { y: 12, z: -3.5, r: 2.5 })),
        D(k.box(60, H - 18, 1, { y: 15, z: -1.2, r: 0.6 }), "screen"),
        D(k.box(18, 10, 0.8, { x: -16, y: H - 16, z: -0.6, r: 0.6 }), "glass"),
        P(k.box(50, 2.2, 14, { z: 17, r: 1 }), "dark"),
      ];
    case "mobile":
      return [
        P(k.box(24, 3, 16, { r: 1.5 }), "dark"),
        P(k.box(32, H - 2, 6, { y: 2, r: 5 })),
        D(k.box(27, H - 13, 1, { y: 7.5, z: 3.1, r: 3 }), "screen"),
        D(k.box(8, 1.2, 0.8, { y: H - 4.5, z: 3.2, r: 0.5 }), "deep"),
        D(k.box(12, 7, 0.6, { x: -4, y: H - 20, z: 3.7, r: 1.2 }), "glass"),
      ];
    case "browser": {
      const z = 2.4;
      return [
        P(k.box(40, 3, 16, { r: 1.5 }), "dark"),
        P(k.box(66, H - 4, 4, { y: 3, r: 3 })),
        D(k.box(62, 7, 1.2, { y: H - 10, z, r: 1 }), "dark"),
        D(k.disc(1.6, 1, { x: -26, y: H - 6.5, z: z + 0.8 }), "alert"),
        D(k.disc(1.6, 1, { x: -21, y: H - 6.5, z: z + 0.8 }), "warn"),
        D(k.disc(1.6, 1, { x: -16, y: H - 6.5, z: z + 0.8 }), "led"),
        D(k.box(22, H - 22, 1.2, { x: -17, y: 7, z, r: 1 }), "light"),
        ...[0, 1, 2].map((i) =>
          D(k.box(28 - i * 6, 3.2, 1.2, { x: 14 - i * 3, y: H - 18 - i * 7, z, r: 1 }), "light"),
        ),
      ];
    }
    case "internet":
      return [
        { geo: k.ball(15, { x: -17, y: 16 }), threshold: 12 },
        { geo: k.ball(20, { x: 2, y: 23, z: -4 }), threshold: 12 },
        { geo: k.ball(14, { x: 20, y: 15, z: 3 }), threshold: 12 },
        { geo: k.ball(13, { x: 1, y: 13, z: 11 }), threshold: 12 },
        P(k.cyl(22, 4, { y: 2, bevel: 1.5 }), "light", false),
      ];
    case "external": {
      const body = H * 0.7;
      return [
        P(k.box(44, body, 44, { r: 4 })),
        D(k.box(34, 1.2, 34, { y: body - 0.3, r: 0.6 }), "light"),
        P(
          k.upright(arrowShape(34, 15), 8, {
            x: 3,
            y: body + 7,
            z: 3,
            rz: Math.PI / 4,
            ry: Math.PI / 4,
          }),
          "screen",
        ),
      ];
    }
  }
}

// ---------------------------------------------------------------- assembly

interface ModelGeometry {
  tones: [Tone, THREE.BufferGeometry][];
  /** Positions with smooth normals, for the clean silhouette. */
  hull: THREE.BufferGeometry | null;
  /** Crease segments, for the sketch ink. */
  creases: number[];
}

const geometryCache = new Map<string, ModelGeometry>();

function normalized(geo: THREE.BufferGeometry): THREE.BufferGeometry {
  const g = geo.index ? geo.toNonIndexed() : geo;
  for (const name of Object.keys(g.attributes)) {
    if (name !== "position" && name !== "normal" && name !== "uv") g.deleteAttribute(name);
  }
  if (!g.getAttribute("uv")) {
    g.setAttribute(
      "uv",
      new THREE.BufferAttribute(new Float32Array(g.getAttribute("position").count * 2), 2),
    );
  }
  g.clearGroups();
  return g;
}

function modelGeometry(kind: NodeKind, height: number, look: Look): ModelGeometry {
  const key = `${kind}|${height}|${look}`;
  let m = geometryCache.get(key);
  if (m) return m;
  const kit = new Kit(look);
  const list = parts(kind, height, kit);
  const byTone = new Map<Tone, THREE.BufferGeometry[]>();
  const hull: THREE.BufferGeometry[] = [];
  const segs: number[] = [];
  for (const p of list) {
    const geo = normalized(p.geo);
    const tone = p.tone ?? "base";
    byTone.set(tone, [...(byTone.get(tone) ?? []), geo]);
    if (p.outline === false) continue;
    if (kit.clean) {
      const h = new THREE.BufferGeometry();
      h.setAttribute("position", geo.getAttribute("position").clone());
      hull.push(h);
    } else {
      segs.push(...creases(geo, p.threshold));
    }
  }
  const tones: [Tone, THREE.BufferGeometry][] = [];
  for (const [tone, geos] of byTone) {
    const merged = mergeGeometries(geos);
    if (!merged) continue;
    merged.computeBoundingSphere();
    merged.userData.shared = true;
    tones.push([tone, merged]);
  }
  let hullGeo: THREE.BufferGeometry | null = null;
  if (hull.length > 0) {
    const merged = mergeGeometries(hull);
    if (merged) {
      hullGeo = mergeVertices(merged, 0.01);
      hullGeo.computeVertexNormals();
      hullGeo.userData.shared = true;
    }
  }
  m = { tones, hull: hullGeo, creases: segs };
  geometryCache.set(key, m);
  return m;
}

export interface ModelOptions {
  look: Look;
  selected: boolean;
  /** Seeds the sketch jitter so each node wobbles differently but stably. */
  seed: number;
}

/** A node's model standing on y = 0, centred on the origin. */
export function nodeModel(
  kind: NodeKind,
  color: string,
  height: number,
  o: ModelOptions,
): THREE.Group {
  const t = theme();
  const base = fill(color);
  const geo = modelGeometry(kind, height, o.look);
  const g = new THREE.Group();
  for (const [tone, tg] of geo.tones) {
    const mesh = new THREE.Mesh(tg, solid(toneColor(base, tone, t), o.look, tone));
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    mesh.userData.pick = true;
    mesh.userData.solid = true;
    g.add(mesh);
  }
  if (o.look === "clean") {
    if (geo.hull) {
      const line = o.selected ? t.accent : darker(base, t.dark ? 0.22 : 0.42);
      const hull = new THREE.Mesh(geo.hull, outlineMaterial(line, o.selected ? 2.6 : 1.1));
      g.add(hull);
    }
    g.add(contactShadow(NODE_SIZE * 1.15));
  } else {
    g.add(
      ink(geo.creases, rng(o.seed), {
        color: o.selected ? t.accent : t.ink,
        width: o.selected ? 3.4 : 1.8,
      }),
    );
  }
  return g;
}
