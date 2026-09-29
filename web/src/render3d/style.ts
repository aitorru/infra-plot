/**
 * Look- and theme-dependent materials for the 3D view.
 *
 * `clean` uses lit PBR materials with a crisp screen-space silhouette; `sketch` keeps the
 * hand-drawn toon shading. Materials and textures made here are cached and flagged with
 * `userData.shared` so the scene never disposes them when an element is rebuilt.
 */
import * as THREE from "three";
import type { Look } from "../model/doc";
import { mix, surface, type Theme, theme } from "../ui/theme";

/** Named shades of a model's colour; `base` is the node colour itself. */
export type Tone =
  | "base"
  | "light"
  | "dark"
  | "deep"
  | "screen"
  | "metal"
  | "led"
  | "warn"
  | "alert"
  | "glass";

export function shade(color: string, amount: number): string {
  return `#${new THREE.Color(color).offsetHSL(0, 0, amount).getHexString()}`;
}

export function darker(color: string, amount = 0.18): string {
  return shade(color, -amount);
}

/** The colour of `tone` for a model whose (theme-adjusted) body colour is `base`. */
export function toneColor(base: string, tone: Tone, t: Theme = theme()): string {
  switch (tone) {
    case "base":
      return base;
    case "light":
      return t.dark ? shade(base, 0.1) : mix(base, "#ffffff", 0.55);
    case "dark":
      return darker(base, t.dark ? 0.08 : 0.16);
    case "deep":
      return darker(base, t.dark ? 0.16 : 0.34);
    case "screen":
      return t.dark ? "#0f1318" : "#2c333d";
    case "metal":
      return surface("#c9ced6", t);
    case "led":
      return "#37b24d";
    case "warn":
      return "#f59f00";
    case "alert":
      return "#f03e3e";
    case "glass":
      return t.dark ? mix(base, "#9fd3ff", 0.35) : mix(base, "#e7f5ff", 0.7);
  }
}

function shared<T extends { userData: Record<string, unknown> }>(o: T): T {
  o.userData.shared = true;
  return o;
}

const materials = new Map<string, THREE.Material>();

/** A shared, lit material for solid geometry in the given look. */
export function solid(color: string, look: Look, tone: Tone = "base"): THREE.Material {
  const glow = tone === "led" || tone === "alert" || tone === "warn";
  const key = `${look}|${color}|${glow ? "glow" : tone === "screen" ? "screen" : ""}`;
  let m = materials.get(key);
  if (!m) {
    if (look === "sketch") {
      m = new THREE.MeshToonMaterial({ color });
    } else {
      m = new THREE.MeshStandardMaterial({
        color,
        roughness: tone === "screen" ? 0.28 : 0.62,
        metalness: tone === "screen" ? 0.2 : 0,
        emissive: glow ? color : "#000000",
        emissiveIntensity: glow ? 0.45 : 0,
      });
    }
    materials.set(key, shared(m));
  }
  return m;
}

// ---------------------------------------------------------------- silhouettes

/** Canvas size in CSS pixels, shared by every outline material. */
export const outlineResolution = { value: new THREE.Vector2(1024, 768) };

const outlines = new Map<string, THREE.ShaderMaterial>();

/**
 * An inverted-hull silhouette of constant screen width: back faces pushed out along their
 * projected normals by `width` CSS pixels. Needs smooth normals (see `hullGeometry`).
 */
export function outlineMaterial(color: string, width: number): THREE.ShaderMaterial {
  const key = `${color}|${width}`;
  let m = outlines.get(key);
  if (!m) {
    m = new THREE.ShaderMaterial({
      uniforms: {
        color: { value: new THREE.Color(color) },
        width: { value: width },
        resolution: outlineResolution,
      },
      vertexShader: /* glsl */ `
        uniform float width;
        uniform vec2 resolution;
        void main() {
          vec4 clip = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
          vec3 n = normalMatrix * normal;
          vec2 dir = (projectionMatrix * vec4(n, 0.0)).xy;
          float len = length(dir);
          if (len > 1e-6) clip.xy += dir / len * width * 2.0 / resolution * clip.w;
          gl_Position = clip;
        }`,
      fragmentShader: /* glsl */ `
        uniform vec3 color;
        void main() {
          gl_FragColor = vec4(color, 1.0);
          #include <colorspace_fragment>
        }`,
      side: THREE.BackSide,
    });
    outlines.set(key, shared(m));
  }
  return m;
}

// ---------------------------------------------------------------- contact shadows

let blobTexture: THREE.Texture | null = null;
const blobs = new Map<string, THREE.MeshBasicMaterial>();
let blobGeometry: THREE.BufferGeometry | null = null;

/** A soft dark disc under a model, grounding it where the shadow map is too coarse. */
export function contactShadow(size: number): THREE.Mesh {
  if (!blobTexture) {
    const c = document.createElement("canvas");
    c.width = c.height = 64;
    const ctx = c.getContext("2d");
    if (ctx) {
      const g = ctx.createRadialGradient(32, 32, 0, 32, 32, 32);
      // Alpha maps read the green channel: white is opaque.
      g.addColorStop(0, "#ffffff");
      g.addColorStop(0.5, "#8c8c8c");
      g.addColorStop(1, "#000000");
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, 64, 64);
    }
    blobTexture = shared(new THREE.CanvasTexture(c));
  }
  blobGeometry ??= shared(new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2));
  const t = theme();
  let m = blobs.get(t.name);
  if (!m) {
    m = shared(
      new THREE.MeshBasicMaterial({
        color: "#000000",
        alphaMap: blobTexture,
        transparent: true,
        opacity: t.dark ? 0.45 : 0.22,
        depthWrite: false,
        polygonOffset: true,
        polygonOffsetFactor: -1,
        polygonOffsetUnits: -1,
      }),
    );
    blobs.set(t.name, m);
  }
  const mesh = new THREE.Mesh(blobGeometry, m);
  mesh.scale.set(size, 1, size);
  mesh.position.y = 0.15;
  mesh.renderOrder = 1;
  return mesh;
}

/** Fill colour for a user or catalogue colour on the current theme. */
export function fill(color: string): string {
  return surface(color);
}
