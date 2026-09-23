/** Low-poly models for every node kind, outlined with jittered "ink" strokes. */
import * as THREE from "three";
import { LineMaterial } from "three/addons/lines/LineMaterial.js";
import { LineSegments2 } from "three/addons/lines/LineSegments2.js";
import { LineSegmentsGeometry } from "three/addons/lines/LineSegmentsGeometry.js";
import { INK } from "../model/catalog";
import type { NodeKind, StrokeStyle } from "../model/doc";

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
  /** Stroke width in world units. */
  width?: number;
  style?: StrokeStyle | undefined;
  /** How far strokes wander from the true edge. */
  jitter?: number;
}

export function dashPattern(style: StrokeStyle | undefined): [number, number] | undefined {
  if (style === "dashed") return [10, 8];
  if (style === "dotted") return [2, 7];
  return undefined;
}

/**
 * Sketchy strokes for a set of segments (pairs of points): each segment is drawn
 * twice, slightly off and overshooting its ends, like a quick pen stroke.
 */
export function ink(segments: readonly number[], random: () => number, o: InkOptions = {}) {
  const jitter = o.jitter ?? 0.9;
  const out: number[] = [];
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
  const geo = new LineSegmentsGeometry();
  geo.setPositions(out);
  const dash = dashPattern(o.style);
  const mat = new LineMaterial({
    color: o.color ?? INK,
    linewidth: o.width ?? 1.6,
    worldUnits: true,
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
  return Array.from(new THREE.EdgesGeometry(geo, threshold).getAttribute("position").array);
}

interface Part {
  geo: THREE.BufferGeometry;
  /** Colour override; `dark` shades the node colour. */
  color?: string | "dark";
  /** Crease angle for the outline; lower draws more edges. */
  threshold?: number;
}

function box(w: number, h: number, d: number, x = 0, y = 0, z = 0): THREE.BufferGeometry {
  return new THREE.BoxGeometry(w, h, d).translate(x, y + h / 2, z);
}

function prism(r: number, h: number, sides: number, y = 0, rTop = r): THREE.BufferGeometry {
  // Flat side facing the viewer reads better than a corner.
  return new THREE.CylinderGeometry(rTop, r, h, sides)
    .rotateY(Math.PI / sides)
    .translate(0, y + h / 2, 0);
}

function ball(r: number, x: number, y: number, z: number, detail = 0): THREE.BufferGeometry {
  return new THREE.IcosahedronGeometry(r, detail).translate(x, y, z);
}

function parts(kind: NodeKind, H: number): Part[] {
  switch (kind) {
    case "service":
      return [{ geo: box(56, H - 4, 56) }, { geo: box(44, 4, 44, 0, H - 4), color: "dark" }];
    case "server":
      return [
        { geo: box(42, H, 52) },
        ...[1, 2, 3, 4].map(
          (i): Part => ({ geo: box(30, 3, 2, 0, (i * H) / 5 - 1.5, 26), color: "dark" }),
        ),
      ];
    case "vm":
      return [{ geo: box(60, H * 0.35, 60) }, { geo: box(46, H * 0.65, 46, 0, H * 0.35) }];
    case "container":
      return [
        { geo: box(64, H, 40) },
        ...[-21, -7, 7, 21].map((x): Part => ({ geo: box(3, H - 8, 2, x, 4, 20), color: "dark" })),
      ];
    case "pod":
      return [{ geo: prism(26, H, 6) }];
    case "k8s":
      return [{ geo: prism(32, H, 7) }, { geo: prism(9, 5, 7, H), color: "dark" }];
    case "function":
      return [{ geo: prism(32, H, 3) }];
    case "proxy":
      return [{ geo: prism(30, H, 8) }];
    case "load-balancer":
      return [{ geo: prism(34, H, 12) }, { geo: prism(16, 6, 12, H), color: "dark" }];
    case "api-gateway":
      return [
        { geo: box(14, H - 12, 18, -22) },
        { geo: box(14, H - 12, 18, 22) },
        { geo: box(64, 12, 22, 0, H - 12) },
      ];
    case "firewall": {
      const row = H / 3;
      return [0, 1, 2].flatMap((i): Part[] =>
        (i % 2 === 0
          ? [
              [-15, 30],
              [15, 30],
            ]
          : [
              [-22.5, 15],
              [0, 30],
              [22.5, 15],
            ]
        ).map(([x = 0, w = 0]) => ({ geo: box(w, row, 20, x, i * row) })),
      );
    }
    case "cdn":
      return [
        { geo: prism(16, 4, 12), color: "dark" },
        { geo: prism(3, H - 50, 8, 4), color: "dark" },
        { geo: ball(26, 0, H - 26, 0, 1), threshold: 12 },
      ];
    case "dns":
      return [{ geo: box(60, H, 44) }, { geo: prism(8, 4, 8, H, 8), color: "dark" }];
    case "database": {
      const band = (H - 4) / 3;
      return [0, 1, 2].map((i) => ({ geo: prism(28, band, 16, i * (band + 2)) }));
    }
    case "cache":
      return [{ geo: prism(30, H / 2 - 1, 16) }, { geo: prism(30, H / 2 - 1, 16, H / 2 + 1) }];
    case "queue":
      return [-24, -8, 8, 24].map((x) => ({ geo: box(14, H, 30, x) }));
    case "storage":
      return [{ geo: prism(22, H, 16, 0, 30) }];
    case "user":
      return [{ geo: prism(20, H * 0.55, 10, 0, 10) }, { geo: ball(12, 0, H * 0.55 + 12, 0) }];
    case "internet":
      return [
        { geo: ball(18, -16, 18, 0) },
        { geo: ball(22, 4, 24, -4) },
        { geo: ball(16, 22, 16, 4) },
        { geo: ball(14, 0, 14, 14) },
      ];
    case "monitoring":
      return [
        { geo: box(30, 4, 20), color: "dark" },
        { geo: box(6, 18, 6, 0, 4), color: "dark" },
        { geo: box(60, H - 22, 6, 0, 22) },
      ];
  }
}

const toonCache = new Map<string, THREE.MeshToonMaterial>();

/** Two-tone shading, shared between models of the same colour. */
export function toon(color: string): THREE.MeshToonMaterial {
  let m = toonCache.get(color);
  if (!m) {
    m = new THREE.MeshToonMaterial({ color });
    toonCache.set(color, m);
  }
  return m;
}

export function darker(color: string, amount = 0.18): string {
  return `#${new THREE.Color(color).offsetHSL(0, 0, -amount).getHexString()}`;
}

/** A node's model standing on y = 0, centred on the origin. */
export function nodeModel(
  kind: NodeKind,
  color: string,
  height: number,
  random: () => number,
  outline: InkOptions,
): THREE.Group {
  const g = new THREE.Group();
  const segments: number[] = [];
  for (const p of parts(kind, height)) {
    const c = p.color === "dark" ? darker(color) : (p.color ?? color);
    const mesh = new THREE.Mesh(p.geo, toon(c));
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    mesh.userData.pick = true;
    g.add(mesh);
    segments.push(...creases(p.geo, p.threshold));
  }
  g.add(ink(segments, random, outline));
  return g;
}
