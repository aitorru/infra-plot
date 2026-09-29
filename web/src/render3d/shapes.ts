/**
 * Geometry primitives for node models, in two flavours: `clean` builds smooth, bevelled
 * solids (rounded boxes, lathed cylinders with rounded rims, bevelled extrusions) and
 * `sketch` builds the faceted low-poly shapes the ink outlines are traced from.
 *
 * Every primitive stands on y = 0 at its (x, z) unless placed otherwise.
 */
import * as THREE from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import type { Look } from "../model/doc";

export interface Place {
  x?: number;
  y?: number;
  z?: number;
  /** Rotations in radians, applied X, then Z, then Y, before translation. */
  rx?: number;
  ry?: number;
  rz?: number;
}

/** Rotates, then translates `geo` in place. */
export function place(geo: THREE.BufferGeometry, p: Place = {}): THREE.BufferGeometry {
  if (p.rx) geo.rotateX(p.rx);
  if (p.rz) geo.rotateZ(p.rz);
  if (p.ry) geo.rotateY(p.ry);
  return geo.translate(p.x ?? 0, p.y ?? 0, p.z ?? 0);
}

export interface BoxOptions extends Place {
  /** Corner radius (clean look only). */
  r?: number;
}

export interface CylOptions extends Place {
  /** Radius at the top; defaults to the bottom radius. */
  top?: number;
  /** Rim rounding (clean look only). */
  bevel?: number;
  /** Facets for the sketch look (clean is always smooth). */
  sides?: number;
}

export interface ExtrudeOptions extends Place {
  bevel?: number;
}

/** Look-aware primitive builders. */
export class Kit {
  readonly clean: boolean;
  constructor(look: Look) {
    this.clean = look === "clean";
  }

  /** A box centred on (x, z) standing on y. */
  box(w: number, h: number, d: number, o: BoxOptions = {}): THREE.BufferGeometry {
    const r = Math.min(o.r ?? 2.5, w / 2 - 0.01, h / 2 - 0.01, d / 2 - 0.01);
    const geo =
      this.clean && r > 0.3
        ? new RoundedBoxGeometry(w, h, d, 2, r)
        : new THREE.BoxGeometry(w, h, d);
    geo.translate(0, h / 2, 0);
    return place(geo, o);
  }

  /** A vertical cylinder (or frustum) with rounded rims. */
  cyl(r: number, h: number, o: CylOptions = {}): THREE.BufferGeometry {
    const top = o.top ?? r;
    if (!this.clean) {
      const sides = o.sides ?? 12;
      const geo = new THREE.CylinderGeometry(top, r, h, sides)
        .rotateY(Math.PI / sides)
        .translate(0, h / 2, 0);
      return place(geo, o);
    }
    const b = Math.max(0, Math.min(o.bevel ?? 2, r / 3, top / 3, h / 3));
    const pts: THREE.Vector2[] = [new THREE.Vector2(0, 0)];
    if (b > 0.2) {
      // Short flat run next to each rim keeps the averaged lathe normals from tilting caps.
      pts.push(new THREE.Vector2(Math.max(r - b - 0.4, 0), 0));
      const steps = 4;
      for (let i = 0; i <= steps; i++) {
        const a = (i / steps) * (Math.PI / 2);
        pts.push(new THREE.Vector2(r - b + Math.sin(a) * b, b - Math.cos(a) * b));
      }
      for (let i = 0; i <= steps; i++) {
        const a = (i / steps) * (Math.PI / 2);
        pts.push(new THREE.Vector2(top - b + Math.cos(a) * b, h - b + Math.sin(a) * b));
      }
      pts.push(new THREE.Vector2(Math.max(top - b - 0.4, 0), h));
    } else {
      pts.push(new THREE.Vector2(r, 0), new THREE.Vector2(top, h));
    }
    pts.push(new THREE.Vector2(0, h));
    return place(new THREE.LatheGeometry(pts, 40), o);
  }

  /** A cylinder lying along X (rz) or Z (rx), e.g. dials and wheels facing the viewer. */
  disc(r: number, depth: number, o: CylOptions & { axis?: "x" | "z" } = {}): THREE.BufferGeometry {
    const geo = this.cyl(r, depth, { ...o, x: 0, y: 0, z: 0, rx: 0, ry: 0, rz: 0 });
    geo.translate(0, -depth / 2, 0);
    if (o.axis === "x") geo.rotateZ(-Math.PI / 2);
    else geo.rotateX(Math.PI / 2);
    return place(geo, o);
  }

  /** A regular prism with a flat side facing the viewer. */
  prism(sides: number, r: number, h: number, o: ExtrudeOptions = {}): THREE.BufferGeometry {
    const shape = new THREE.Shape();
    for (let i = 0; i < sides; i++) {
      const a = (i / sides) * Math.PI * 2 + Math.PI / sides;
      const px = Math.cos(a) * r;
      const py = Math.sin(a) * r;
      if (i === 0) shape.moveTo(px, py);
      else shape.lineTo(px, py);
    }
    shape.closePath();
    return this.flat(shape, h, o);
  }

  /** `shape` (in the XZ plane, +y of the shape pointing to -z) extruded upwards by `h`. */
  flat(shape: THREE.Shape, h: number, o: ExtrudeOptions = {}): THREE.BufferGeometry {
    const geo = this.extrude(shape, h, o.bevel);
    geo.rotateX(-Math.PI / 2);
    return place(geo, o);
  }

  /** `shape` (in the XY plane) standing upright, `depth` thick along Z, centred on z = 0. */
  upright(shape: THREE.Shape, depth: number, o: ExtrudeOptions = {}): THREE.BufferGeometry {
    const geo = this.extrude(shape, depth, o.bevel);
    geo.translate(0, 0, -depth / 2);
    return place(geo, o);
  }

  /** `shape` extruded along +z from 0 to `depth`, with bevelled edges in the clean look. */
  extrude(shape: THREE.Shape, depth: number, bevel = 1.2): THREE.BufferGeometry {
    const b = this.clean ? Math.min(bevel, depth / 3) : 0;
    const geo = new THREE.ExtrudeGeometry(shape, {
      depth: Math.max(depth - 2 * b, 0.01),
      bevelEnabled: b > 0,
      bevelThickness: b,
      bevelSize: b * 0.8,
      bevelOffset: -b * 0.8,
      bevelSegments: 2,
      curveSegments: this.clean ? 20 : 6,
    });
    return geo.translate(0, 0, b);
  }

  ball(r: number, o: Place = {}): THREE.BufferGeometry {
    const geo = this.clean
      ? new THREE.SphereGeometry(r, 28, 18)
      : new THREE.IcosahedronGeometry(r, 1);
    return place(geo, o);
  }

  /** A torus in the XY plane (facing +z); `arc` < 2π draws part of it. */
  ring(radius: number, tube: number, o: Place & { arc?: number } = {}): THREE.BufferGeometry {
    const geo = this.clean
      ? new THREE.TorusGeometry(radius, tube, 12, 40, o.arc ?? Math.PI * 2)
      : new THREE.TorusGeometry(radius, tube, 4, 12, o.arc ?? Math.PI * 2);
    return place(geo, o);
  }

  tube(points: THREE.Vector3[], radius: number, closed = false): THREE.BufferGeometry {
    const curve = new THREE.CatmullRomCurve3(points, closed, "centripetal");
    return new THREE.TubeGeometry(
      curve,
      Math.max(8, points.length * (this.clean ? 8 : 3)),
      radius,
      this.clean ? 10 : 5,
      closed,
    );
  }

  cone(r: number, h: number, o: Place = {}): THREE.BufferGeometry {
    const geo = new THREE.ConeGeometry(r, h, this.clean ? 24 : 8).translate(0, h / 2, 0);
    return place(geo, o);
  }
}

// ---------------------------------------------------------------- 2D outlines

export function polygon(points: readonly [number, number][]): THREE.Shape {
  const s = new THREE.Shape();
  points.forEach(([x, y], i) => {
    if (i === 0) s.moveTo(x, y);
    else s.lineTo(x, y);
  });
  s.closePath();
  return s;
}

export function roundedRect(w: number, h: number, r: number, cx = 0, cy = 0): THREE.Shape {
  const x = cx - w / 2;
  const y = cy - h / 2;
  const s = new THREE.Shape();
  s.moveTo(x + r, y);
  s.lineTo(x + w - r, y);
  s.quadraticCurveTo(x + w, y, x + w, y + r);
  s.lineTo(x + w, y + h - r);
  s.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  s.lineTo(x + r, y + h);
  s.quadraticCurveTo(x, y + h, x, y + h - r);
  s.lineTo(x, y + r);
  s.quadraticCurveTo(x, y, x + r, y);
  return s;
}

/** An arrow pointing along +x, `length` long, centred on the origin. */
export function arrowShape(length: number, width: number): THREE.Shape {
  const head = Math.min(width * 1.1, length * 0.5);
  const half = width / 2;
  const l = length / 2;
  return polygon([
    [-l, -half * 0.45],
    [l - head, -half * 0.45],
    [l - head, -half],
    [l, 0],
    [l - head, half],
    [l - head, half * 0.45],
    [-l, half * 0.45],
  ]);
}

/** A toothed gear outline with a round hole. */
export function gearShape(r: number, teeth: number, hole: number): THREE.Shape {
  const s = new THREE.Shape();
  const inner = r * 0.8;
  const steps = teeth * 4;
  for (let i = 0; i < steps; i++) {
    const a0 = (i / steps) * Math.PI * 2;
    const rr = i % 4 === 1 || i % 4 === 2 ? r : inner;
    const px = Math.cos(a0) * rr;
    const py = Math.sin(a0) * rr;
    if (i === 0) s.moveTo(px, py);
    else s.lineTo(px, py);
  }
  s.closePath();
  if (hole > 0) {
    const h = new THREE.Path();
    h.absarc(0, 0, hole, 0, Math.PI * 2, true);
    s.holes.push(h);
  }
  return s;
}
