/** Hand-drawn text for the 3D view: Kalam rendered into canvas textures. */
import * as THREE from "three";
import { FONT_FAMILY, INK } from "../model/catalog";

/** Canvas pixels per world unit; labels stay crisp up to ~4× zoom. */
const RES = 4;

export interface TextLine {
  text: string;
  size: number;
  color?: string;
  weight?: number;
}

export interface TextOptions {
  align?: "left" | "center";
  /** Rounded background behind the text. */
  background?: string;
  padding?: number;
}

interface TextTexture {
  texture: THREE.CanvasTexture;
  /** Size in world units. */
  w: number;
  h: number;
}

function font(line: TextLine): string {
  return `${line.weight ?? 400} ${line.size * RES}px ${FONT_FAMILY}`;
}

function textTexture(lines: readonly TextLine[], opts: TextOptions): TextTexture {
  const pad = opts.padding ?? 4;
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) throw new Error("2D canvas not available");
  const widths = lines.map((l) => {
    ctx.font = font(l);
    return ctx.measureText(l.text).width / RES;
  });
  const w = Math.max(...widths, 1) + pad * 2;
  const h = lines.reduce((sum, l) => sum + l.size * 1.25, 0) + pad * 2;

  const canvas = ctx.canvas;
  canvas.width = Math.ceil(w * RES);
  canvas.height = Math.ceil(h * RES);
  ctx.scale(RES, RES);
  if (opts.background) {
    ctx.fillStyle = opts.background;
    ctx.beginPath();
    ctx.roundRect(0, 0, w, h, Math.min(8, h / 2));
    ctx.fill();
  }
  ctx.textBaseline = "middle";
  let y = pad;
  lines.forEach((l, i) => {
    ctx.save();
    ctx.scale(1 / RES, 1 / RES);
    ctx.font = font(l);
    ctx.fillStyle = l.color ?? INK;
    const x = opts.align === "left" ? pad : (w - (widths[i] ?? 0)) / 2;
    ctx.fillText(l.text, x * RES, (y + l.size * 0.625) * RES);
    ctx.restore();
    y += l.size * 1.25;
  });

  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 4;
  return { texture, w, h };
}

/**
 * A billboard that always faces the camera, anchored at its bottom centre.
 * Drawn on top of everything so labels never hide behind models.
 */
export function labelSprite(lines: readonly TextLine[], opts: TextOptions = {}): THREE.Sprite {
  const { texture, w, h } = textTexture(lines, opts);
  const sprite = new THREE.Sprite(
    new THREE.SpriteMaterial({ map: texture, depthTest: false, depthWrite: false }),
  );
  sprite.scale.set(w, h, 1);
  sprite.center.set(0.5, 0);
  sprite.renderOrder = 10;
  return sprite;
}

/** Text lying flat on the ground (or a zone), its top-left corner at the origin. */
export function flatLabel(lines: readonly TextLine[], opts: TextOptions = {}): THREE.Mesh {
  const { texture, w, h } = textTexture(lines, { align: "left", ...opts });
  const geo = new THREE.PlaneGeometry(w, h);
  geo.rotateX(-Math.PI / 2);
  geo.translate(w / 2, 0, h / 2);
  const mesh = new THREE.Mesh(
    geo,
    new THREE.MeshBasicMaterial({
      map: texture,
      transparent: true,
      depthWrite: false,
      polygonOffset: true,
      polygonOffsetFactor: -2,
      polygonOffsetUnits: -2,
    }),
  );
  mesh.renderOrder = 1;
  return mesh;
}
