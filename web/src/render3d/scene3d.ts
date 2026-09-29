/**
 * The isometric 3D view: a three.js scene rebuilt from the document, in the document's look
 * (`clean` or `sketch`) and the current colour theme.
 */
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import { LineMaterial } from "three/addons/lines/LineMaterial.js";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";
import { fontFamily, GRID, NODE_KINDS, NODE_SIZE, TEXT_SIZES, ZONE_KINDS } from "../model/catalog";
import type { Arrow, Doc, Edge, Line, Look, Node, Note, StrokeStyle, Zone } from "../model/doc";
import { findElement } from "../model/doc";
import {
  type Box,
  docBounds,
  edgePath,
  hashSeed,
  nodeBox,
  noteBox,
  type Point,
  snap,
  zoneBox,
  zoneDepth,
} from "../model/geometry";
import { addEdge, addNode, moveElement, zoneContents } from "../state/ops";
import type { Store } from "../state/store";
import { mix, onThemeChange, surface, type Theme, theme } from "../ui/theme";
import { flatLabel, labelSprite } from "./labels";
import { creases, dashPattern, ink, nodeModel, rng } from "./models";
import { darker, outlineResolution, solid } from "./style";

/** Thickness of a zone slab; nested zones stack one slab per level. */
const SLAB = 12;
/** The true isometric elevation: the camera looks down the cube diagonal. */
const ISO_POLAR = Math.acos(1 / Math.sqrt(3));
const QUARTER = Math.PI / 2;
const CAMERA_DISTANCE = 8000;
const PACKET_SPEED = 120; // world units per second
const MAX_EXPORT_SIZE = 4096;

type ElementType = "zone" | "node" | "edge" | "line" | "note";

interface Packets {
  curve: THREE.Curve<THREE.Vector3>;
  meshes: THREE.Mesh[];
  length: number;
  reverse: boolean;
}

type Drag =
  | {
      kind: "move";
      id: string;
      plane: THREE.Plane;
      last: Point;
      moved: boolean;
      contents: ReadonlySet<string>;
    }
  | { kind: "connect"; from: string; start: THREE.Vector3 }
  | { kind: "click"; x: number; y: number };

interface Hit {
  id: string;
  type: ElementType;
  point: THREE.Vector3;
}

function isSelectable(type: ElementType): boolean {
  return type === "node" || type === "zone";
}

function pointBox([x, y]: Point): Box {
  return { x, y, w: 0, h: 0 };
}

function webglAvailable(): boolean {
  try {
    const c = document.createElement("canvas");
    return !!(c.getContext("webgl2") ?? c.getContext("webgl"));
  } catch {
    return false;
  }
}

function makeRenderer(preserveDrawingBuffer = false): THREE.WebGLRenderer {
  const r = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer });
  r.shadowMap.enabled = true;
  r.shadowMap.type = THREE.PCFShadowMap;
  // Shadows only change with the scene, not the camera; `#collect` asks for updates.
  r.shadowMap.autoUpdate = false;
  r.shadowMap.needsUpdate = true;
  r.setClearColor(theme().paper);
  return r;
}

/** A paper-coloured ground with the same dotted grid as the 2D canvas. */
function paperTexture(th: Theme): THREE.CanvasTexture {
  const c = document.createElement("canvas");
  c.width = c.height = 64;
  const ctx = c.getContext("2d");
  if (ctx) {
    ctx.fillStyle = th.paper;
    ctx.fillRect(0, 0, 64, 64);
    ctx.fillStyle = th.grid;
    ctx.beginPath();
    ctx.arc(4, 4, 2.6, 0, Math.PI * 2);
    ctx.fill();
  }
  const t = new THREE.CanvasTexture(c);
  t.colorSpace = THREE.SRGBColorSpace;
  t.wrapS = t.wrapT = THREE.RepeatWrapping;
  t.anisotropy = 8;
  return t;
}

/** Invisible material for hit boxes, shared by every element. */
const HIDDEN = new THREE.MeshBasicMaterial({ visible: false });
HIDDEN.userData.shared = true;
const PACKET = new THREE.IcosahedronGeometry(4.5, 1);
PACKET.userData.shared = true;

/** `#rrggbb` plus an alpha byte. */
function alpha(color: string, a: number): string {
  return `${color}${Math.round(a * 255)
    .toString(16)
    .padStart(2, "0")}`;
}

/** A polyline with its corners rounded off by up to `radius`. */
function roundedPath(v: THREE.Vector3[], radius: number): THREE.CurvePath<THREE.Vector3> {
  const path = new THREE.CurvePath<THREE.Vector3>();
  let from = v[0] as THREE.Vector3;
  for (let i = 1; i < v.length; i++) {
    const corner = v[i] as THREE.Vector3;
    const next = v[i + 1];
    if (!next || radius <= 0) {
      path.add(new THREE.LineCurve3(from, corner));
      from = corner;
      continue;
    }
    const inLen = corner.distanceTo(from);
    const outLen = next.distanceTo(corner);
    const r = Math.min(radius, inLen / 2, outLen / 2);
    const a = corner.clone().lerp(from, r / Math.max(inLen, 1e-6));
    const b = corner.clone().lerp(next, r / Math.max(outLen, 1e-6));
    if (from.distanceTo(a) > 0.01) path.add(new THREE.LineCurve3(from, a));
    if (r > 0.01) path.add(new THREE.QuadraticBezierCurve3(a, corner, b));
    from = b;
  }
  return path;
}

export class Scene3D {
  readonly host: HTMLElement;
  #store: Store;
  #renderer: THREE.WebGLRenderer | null = null;
  #scene = new THREE.Scene();
  #camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 1, CAMERA_DISTANCE * 3);
  #controls: OrbitControls | null = null;
  #root = new THREE.Group();
  #sun = new THREE.DirectionalLight("#ffffff", 1.4);
  #hemi = new THREE.HemisphereLight("#ffffff", "#d8d2c0", 1.9);
  #paper: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial> | null = null;
  #shadowCatcher: THREE.Mesh<THREE.PlaneGeometry, THREE.ShadowMaterial> | null = null;
  #lighting = "";
  #preview: THREE.Object3D | null = null;
  #cache = new Map<string, { key: string; obj: THREE.Object3D; type: ElementType }>();
  #pickables: THREE.Object3D[] = [];
  #packets: Packets[] = [];
  #bounds = new THREE.Box3();
  #raycaster = new THREE.Raycaster();
  #size = { w: 0, h: 0 };
  #visible = false;
  #stale = true;
  #dirty = true;
  #needsFit = true;
  #generation = 0;
  #drag: Drag | null = null;
  #spin: { start: number; from: THREE.Vector3; delta: number } | null = null;
  #timer = new THREE.Timer();
  #time = 0;

  constructor(host: HTMLElement, store: Store) {
    this.host = host;
    this.#store = store;
    if (!webglAvailable()) {
      host.dataset.webgl = "unavailable";
      host.append("WebGL is not available in this browser, so the 3D view cannot be shown.");
      return;
    }
    const renderer = makeRenderer();
    this.#renderer = renderer;
    const canvas = renderer.domElement;
    canvas.classList.add("canvas3d");
    canvas.dataset.testid = "canvas-3d";
    host.prepend(canvas);

    this.#setupScene();
    onThemeChange(() => {
      this.#applyTheme();
      this.invalidate();
    });

    // Registered before OrbitControls so a press on a node can disable orbiting.
    canvas.addEventListener("pointerdown", (e) => this.#onDown(e));
    canvas.addEventListener("pointermove", (e) => this.#onMove(e));
    canvas.addEventListener("pointerup", (e) => this.#onUp(e));
    canvas.addEventListener("dblclick", (e) => this.#onDoubleClick(e));
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());

    const controls = new OrbitControls(this.#camera, canvas);
    controls.mouseButtons = {
      LEFT: THREE.MOUSE.PAN,
      MIDDLE: THREE.MOUSE.DOLLY,
      RIGHT: THREE.MOUSE.ROTATE,
    };
    controls.touches = { ONE: THREE.TOUCH.PAN, TWO: THREE.TOUCH.DOLLY_ROTATE };
    controls.screenSpacePanning = false; // pan along the ground
    controls.zoomToCursor = true;
    controls.enableDamping = true;
    controls.dampingFactor = 0.2;
    controls.minZoom = 0.1;
    controls.maxZoom = 5;
    controls.minPolarAngle = 0.35;
    controls.maxPolarAngle = 1.25;
    controls.addEventListener("change", () => {
      this.#dirty = true;
    });
    this.#controls = controls;
    this.#camera.position.setFromSphericalCoords(CAMERA_DISTANCE, ISO_POLAR, Math.PI / 4);
    this.#camera.lookAt(0, 0, 0);

    new ResizeObserver(() => this.#resize()).observe(host);
  }

  get available(): boolean {
    return this.#renderer !== null;
  }

  #setupScene(): void {
    this.#scene.add(this.#hemi);
    this.#sun.castShadow = true;
    this.#sun.shadow.mapSize.set(2048, 2048);
    this.#sun.shadow.radius = 5;
    this.#sun.shadow.blurSamples = 12;
    this.#sun.shadow.bias = -0.0005;
    this.#sun.shadow.normalBias = 0.5;
    this.#scene.add(this.#sun, this.#sun.target);

    const size = 60000;
    const paper = new THREE.Mesh(
      new THREE.PlaneGeometry(size, size).rotateX(-Math.PI / 2),
      new THREE.MeshBasicMaterial(),
    );
    // The grid dot sits at the texel origin; align it with the 2D grid.
    paper.position.set(-2, -0.05, -2);
    paper.userData.ground = true;
    const shadows = new THREE.Mesh(
      new THREE.PlaneGeometry(size, size).rotateX(-Math.PI / 2),
      new THREE.ShadowMaterial({ opacity: 0.16 }),
    );
    shadows.receiveShadow = true;
    this.#paper = paper;
    this.#shadowCatcher = shadows;
    this.#scene.add(paper, shadows, this.#root);
    this.#applyTheme();
  }

  /** Background, ground grid and shadow strength for the current theme. */
  #applyTheme(): void {
    const t = theme();
    this.#scene.background = new THREE.Color(t.paper);
    this.#renderer?.setClearColor(t.paper);
    const paper = this.#paper;
    if (paper) {
      const size = 60000;
      paper.material.map?.dispose();
      const map = paperTexture(t);
      map.repeat.set(size / GRID, size / GRID);
      paper.material.map = map;
      paper.material.needsUpdate = true;
    }
    if (this.#shadowCatcher) this.#shadowCatcher.material.opacity = t.dark ? 0.32 : 0.14;
    this.#lighting = "";
    this.#dirty = true;
  }

  /**
   * Light rig per look and theme. Clean: a soft key light and a strong sky fill, tuned so
   * top, left and right faces read as three distinct tones like an isometric illustration.
   * Sketch: the brighter rig the two-tone toon shading was designed for.
   */
  #applyLighting(look: Look): void {
    const t = theme();
    const key = `${look}|${t.name}`;
    if (key === this.#lighting) return;
    this.#lighting = key;
    if (look === "clean") {
      this.#hemi.color.set("#ffffff");
      this.#hemi.groundColor.set(t.dark ? "#5b6270" : mix("#ffffff", t.paper, 0.6));
      this.#hemi.intensity = t.dark ? 1.85 : 2.05;
      this.#sun.intensity = t.dark ? 1.25 : 1.15;
    } else {
      this.#hemi.color.set("#ffffff");
      this.#hemi.groundColor.set(t.dark ? "#555b66" : "#d8d2c0");
      this.#hemi.intensity = 1.9;
      this.#sun.intensity = 1.4;
    }
  }

  // ---------------------------------------------------------------- visibility

  setVisible(visible: boolean): void {
    if (!this.#renderer || visible === this.#visible) return;
    this.#visible = visible;
    if (visible) {
      this.#resize();
      this.render();
      this.#timer.reset();
      this.#renderer.setAnimationLoop((t) => this.#tick(t));
    } else {
      this.#renderer.setAnimationLoop(null);
      this.#drag = null;
    }
  }

  #resize(): void {
    const r = this.#renderer;
    if (!r) return;
    const w = this.host.clientWidth;
    const h = this.host.clientHeight;
    if (w === 0 || h === 0 || (w === this.#size.w && h === this.#size.h)) return;
    this.#size = { w, h };
    r.setPixelRatio(window.devicePixelRatio);
    r.setSize(w, h);
    this.#frustum(this.#camera, w, h);
    this.#lineResolution(w, h);
    if (this.#needsFit) this.fit();
    this.#dirty = true;
  }

  #frustum(cam: THREE.OrthographicCamera, w: number, h: number): void {
    cam.left = -w / 2;
    cam.right = w / 2;
    cam.top = h / 2;
    cam.bottom = -h / 2;
    cam.updateProjectionMatrix();
  }

  #lineResolution(w: number, h: number): void {
    outlineResolution.value.set(w, h);
    this.#scene.traverse((o) => {
      const m = (o as THREE.Mesh).material;
      if (m instanceof LineMaterial) m.resolution.set(w, h);
    });
  }

  // ---------------------------------------------------------------- camera

  /** Frames the whole diagram from the current angle. */
  fit(): void {
    const controls = this.#controls;
    if (!controls || !this.#visible || this.#size.w === 0) {
      this.#needsFit = true;
      return;
    }
    this.#needsFit = false;
    this.#finishSpin();
    this.#sync();
    const cam = this.#camera;
    const box = this.#bounds.isEmpty()
      ? new THREE.Box3(new THREE.Vector3(-200, 0, -200), new THREE.Vector3(200, 0, 200))
      : this.#bounds;
    const center = box.getCenter(new THREE.Vector3());
    const dir = cam.position.clone().sub(controls.target).normalize();
    controls.target.copy(center);
    cam.position.copy(center).addScaledVector(dir, CAMERA_DISTANCE);
    cam.lookAt(center);
    cam.updateMatrixWorld();

    const view = new THREE.Box3();
    const { min, max } = box;
    for (const x of [min.x, max.x])
      for (const y of [min.y, max.y])
        for (const z of [min.z, max.z])
          view.expandByPoint(new THREE.Vector3(x, y, z).applyMatrix4(cam.matrixWorldInverse));

    // Recentre on the projected box, which is not centred on the 3D centre.
    const shift = new THREE.Vector3(
      (view.min.x + view.max.x) / 2,
      (view.min.y + view.max.y) / 2,
      0,
    ).applyQuaternion(cam.quaternion);
    controls.target.add(shift);
    cam.position.add(shift);

    const pad = 60;
    const { w, h } = this.#size;
    const zoom = Math.min(
      (w - pad * 2) / Math.max(view.max.x - view.min.x, 1),
      (h - pad * 2) / Math.max(view.max.y - view.min.y, 1),
    );
    cam.zoom = THREE.MathUtils.clamp(zoom, controls.minZoom, 2);
    cam.updateProjectionMatrix();
    controls.update();
    this.#dirty = true;
  }

  /** Turns the camera 90° around the diagram, snapping to the isometric diagonals. */
  rotate(direction: 1 | -1): void {
    const controls = this.#controls;
    if (!controls) return;
    this.#finishSpin();
    const az = controls.getAzimuthalAngle();
    const snapped = Math.round((az - Math.PI / 4) / QUARTER) * QUARTER + Math.PI / 4;
    const delta = snapped + direction * QUARTER - az;
    this.#spin = {
      start: performance.now(),
      from: this.#camera.position.clone().sub(controls.target),
      delta,
    };
  }

  #spinTo(k: number): void {
    const spin = this.#spin;
    const target = this.#controls?.target;
    if (!spin || !target) return;
    this.#camera.position
      .copy(spin.from)
      .applyAxisAngle(new THREE.Vector3(0, 1, 0), spin.delta * k)
      .add(target);
    this.#camera.lookAt(target);
    this.#dirty = true;
  }

  #finishSpin(): void {
    this.#spinTo(1);
    this.#spin = null;
    this.#controls?.update();
  }

  /** Azimuth in degrees, 0..360; 45 is the default isometric angle. */
  get azimuth(): number {
    const a = THREE.MathUtils.radToDeg(this.#controls?.getAzimuthalAngle() ?? Math.PI / 4);
    return Math.round((a + 360) % 360);
  }

  // ---------------------------------------------------------------- loop

  #tick(time?: number): void {
    const r = this.#renderer;
    const controls = this.#controls;
    if (!r || !controls) return;
    this.#timer.update(time);
    const dt = Math.min(this.#timer.getDelta(), 0.1);

    if (this.#spin) {
      const k = Math.min(1, (performance.now() - this.#spin.start) / 350);
      this.#spinTo(1 - (1 - k) ** 3);
      if (k === 1) this.#spin = null;
    }
    controls.update();
    this.#clampTarget();

    if (this.#stale) this.#sync();
    if (this.#store.state.animate && this.#packets.length > 0) {
      this.#time += dt;
      this.#movePackets();
      this.#dirty = true;
    }
    if (this.#dirty) {
      this.#dirty = false;
      r.render(this.#scene, this.#camera);
      this.host.dataset.zoom = this.#camera.zoom.toFixed(2);
      this.host.dataset.azimuth = String(this.azimuth);
      this.host.dataset.rendered = String(Number(this.host.dataset.rendered ?? 0) + 1);
    }
  }

  /** Keeps panning within reach of the diagram. */
  #clampTarget(): void {
    const controls = this.#controls;
    if (!controls || this.#bounds.isEmpty()) return;
    const limit = this.#bounds.clone().expandByScalar(600);
    const t = controls.target;
    const clamped = t.clone().clamp(limit.min, limit.max);
    if (!clamped.equals(t)) {
      const d = clamped.sub(t);
      t.add(d);
      this.#camera.position.add(d);
    }
  }

  #movePackets(): void {
    for (const p of this.#packets) {
      const n = p.meshes.length;
      p.meshes.forEach((m, i) => {
        let t = ((this.#time * PACKET_SPEED) / p.length + i / n) % 1;
        if (p.reverse) t = 1 - t;
        p.curve.getPointAt(t, m.position);
      });
    }
  }

  // ---------------------------------------------------------------- scene sync

  /** Called on every store change; the scene is rebuilt lazily while visible. */
  render(): void {
    this.#stale = true;
    this.#dirty = true;
    for (const p of this.#packets) for (const m of p.meshes) m.visible = this.#store.state.animate;
  }

  /** Forces every element to be rebuilt (e.g. once web fonts load). */
  invalidate(): void {
    this.#generation++;
    this.render();
  }

  /** Rebuilds and draws right away, for screenshots and tests. */
  flush(): void {
    if (!this.#renderer || !this.#visible) return;
    this.#sync();
    this.#dirty = true;
    this.#tick();
  }

  #sync(selection = this.#store.state.selection): void {
    this.#stale = false;
    const doc = this.#store.doc;
    const look = doc.look;
    this.#applyLighting(look);
    const seen = new Set<string>();
    const zoneBase = new Map<string, number>();
    doc.zones.forEach((z, i) => {
      // A hair of offset keeps overlapping, non-nested zones from z-fighting.
      zoneBase.set(z.id, zoneDepth(doc, zoneBox(z), z.id) * SLAB + i * 0.02);
    });
    const baseAt = (b: Box) => zoneDepth(doc, b) * SLAB;

    const place = (type: ElementType, id: string, key: unknown, build: () => THREE.Object3D) => {
      seen.add(id);
      const fullKey = JSON.stringify([this.#generation, look, id === selection, key]);
      let entry = this.#cache.get(id);
      if (!entry || entry.key !== fullKey) {
        if (entry) this.#dispose(entry.obj);
        const obj = build();
        Object.assign(obj.userData, { id, type });
        this.#root.add(obj);
        entry = { key: fullKey, obj, type };
        this.#cache.set(id, entry);
      }
    };

    for (const z of doc.zones) {
      const base = zoneBase.get(z.id) ?? 0;
      place("zone", z.id, [z, base], () => this.#buildZone(z, base, z.id === selection, look));
    }
    for (const n of doc.nodes) {
      const base = baseAt(nodeBox(n));
      place("node", n.id, [n, base], () => this.#buildNode(n, base, n.id === selection, look));
    }
    for (const e of doc.edges) {
      const pts = edgePath(doc, e);
      if (!pts) continue;
      const ends = [e.from, e.to].map((id) => this.#anchorHeight(doc, id, zoneBase));
      place("edge", e.id, [e, pts, ends], () =>
        this.#buildEdge(e, pts, ends as [number, number], e.id === selection, look),
      );
    }
    for (const l of doc.lines) {
      const bases = l.points.map((p) => baseAt(pointBox(p)));
      place("line", l.id, [l, bases], () => this.#buildLine(l, bases, l.id === selection, look));
    }
    for (const n of doc.notes) {
      const base = baseAt(noteBox(n));
      place("note", n.id, [n, base], () => this.#buildNote(n, base, n.id === selection, look));
    }

    for (const [id, entry] of this.#cache) {
      if (!seen.has(id)) {
        this.#dispose(entry.obj);
        this.#cache.delete(id);
      }
    }
    this.#collect();
  }

  /** Refreshes pick targets, packets, bounds, shadows and the test hooks. */
  #collect(): void {
    this.#pickables = [];
    this.#packets = [];
    const counts: Record<ElementType, number> = { zone: 0, node: 0, edge: 0, line: 0, note: 0 };
    let meshes = 0;
    for (const { obj, type } of this.#cache.values()) {
      counts[type]++;
      obj.traverse((o) => {
        if (o.userData.pick) this.#pickables.push(o);
        if (o instanceof THREE.Mesh && o.userData.solid) meshes++;
      });
      const packets = obj.userData.packets as Packets | undefined;
      if (packets) this.#packets.push(packets);
    }
    if (this.#store.state.animate) this.#movePackets();

    const doc = this.#store.doc;
    const b = docBounds(doc);
    this.#bounds.makeEmpty();
    if (b) {
      let top = 0;
      for (const n of doc.nodes) top = Math.max(top, NODE_KINDS[n.kind].height + 60);
      top += doc.zones.length > 0 ? SLAB * 4 : 0;
      this.#bounds.set(
        new THREE.Vector3(b.x, 0, b.y),
        new THREE.Vector3(b.x + b.w, top, b.y + b.h),
      );
    }
    this.#aimSun();
    if (this.#renderer) this.#renderer.shadowMap.needsUpdate = true;
    this.#lineResolution(this.#size.w, this.#size.h);

    const ds = this.host.dataset;
    ds.zones = String(counts.zone);
    ds.nodes = String(counts.node);
    ds.edges = String(counts.edge);
    ds.lines = String(counts.line);
    ds.notes = String(counts.note);
    ds.meshes = String(meshes);
    ds.packets = String(this.#packets.reduce((n, p) => n + p.meshes.length, 0));
  }

  /** Fits the shadow camera around the diagram so shadows stay sharp. */
  #aimSun(): void {
    const box = this.#bounds.isEmpty()
      ? new THREE.Box3(new THREE.Vector3(-400, 0, -400), new THREE.Vector3(400, 100, 400))
      : this.#bounds;
    const center = box.getCenter(new THREE.Vector3());
    const radius = box.getBoundingSphere(new THREE.Sphere()).radius + 100;
    this.#sun.target.position.copy(center);
    this.#sun.position.copy(center).add(new THREE.Vector3(-0.4, 1, 0.5).setLength(radius * 2));
    const cam = this.#sun.shadow.camera;
    cam.left = cam.bottom = -radius;
    cam.right = cam.top = radius;
    cam.near = 1;
    cam.far = radius * 4;
    cam.updateProjectionMatrix();
    this.#sun.target.updateMatrixWorld();
  }

  #dispose(obj: THREE.Object3D): void {
    obj.removeFromParent();
    obj.traverse((o) => {
      const mesh = o as THREE.Mesh;
      // Model geometry and materials are shared through caches (see style.ts, models.ts).
      if (mesh.geometry && !mesh.geometry.userData.shared) mesh.geometry.dispose();
      const mats = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
      for (const m of mats) {
        if (!m || m.userData.shared) continue;
        (m as THREE.MeshBasicMaterial).map?.dispose();
        m.dispose();
      }
    });
  }

  /** Height at which an edge meets element `id`. */
  #anchorHeight(doc: Doc, id: string, zoneBase: Map<string, number>): number {
    const zb = zoneBase.get(id);
    if (zb !== undefined) return zb + SLAB;
    const n = doc.nodes.find((n) => n.id === id);
    if (!n) return 0;
    return zoneDepth(doc, nodeBox(n)) * SLAB + Math.min(NODE_KINDS[n.kind].height / 2, 24);
  }

  // ---------------------------------------------------------------- builders

  #buildZone(z: Zone, base: number, selected: boolean, look: Look): THREE.Object3D {
    const t = theme();
    const info = ZONE_KINDS[z.kind ?? "generic"];
    // Zones are large areas: on dark themes they sink further into the paper than nodes.
    const raw = z.color ?? info.color;
    const color = t.dark ? mix(raw, t.paper, 0.76) : surface(raw);
    const style = z.style ?? info.style;
    const clean = look === "clean";
    const g = new THREE.Group();
    const geo = clean
      ? new RoundedBoxGeometry(z.w, SLAB, z.h, 2, 4)
      : new THREE.BoxGeometry(z.w, SLAB, z.h);
    geo.translate(z.x + z.w / 2, base + SLAB / 2, z.y + z.h / 2);
    const side = solid(darker(color, t.dark ? 0.05 : clean ? 0.12 : 0.08), look);
    const top = solid(color, look);
    // BoxGeometry face groups: +x, -x, +y, -y, +z, -z.
    const slab = new THREE.Mesh(geo, [side, side, top, top, side, side]);
    slab.receiveShadow = true;
    slab.castShadow = true;
    slab.userData.pick = true;
    slab.userData.solid = true;
    g.add(slab);

    if (clean) {
      // A crisp inset border on the flat top, in a deeper shade of the zone colour.
      const inset = 6;
      const x0 = z.x + inset;
      const x1 = z.x + z.w - inset;
      const z0 = z.y + inset;
      const z1 = z.y + z.h - inset;
      const y = base + SLAB + 0.25;
      const corners = [
        new THREE.Vector3(x0, y, z0),
        new THREE.Vector3(x1, y, z0),
        new THREE.Vector3(x1, y, z1),
        new THREE.Vector3(x0, y, z1),
        new THREE.Vector3(x0, y, z0),
      ];
      const loop = roundedPath(corners, 8);
      const pts = loop.getSpacedPoints(Math.max(64, Math.round(loop.getLength() / 6)));
      const segs: number[] = [];
      for (let i = 1; i < pts.length; i++) {
        const a = pts[i - 1] as THREE.Vector3;
        const b = pts[i] as THREE.Vector3;
        segs.push(a.x, a.y, a.z, b.x, b.y, b.z);
      }
      const stroke = selected
        ? t.accent
        : t.dark
          ? mix(color, t.zoneStroke, 0.55)
          : mix(darker(color, 0.4), t.zoneStroke, 0.35);
      g.add(
        ink(segs, () => 0.5, {
          color: stroke,
          width: selected ? 2.6 : 1.4,
          style,
          jitter: 0,
          pixels: true,
        }),
      );
    } else {
      g.add(
        ink(creases(geo), rng(hashSeed(z.id)), {
          color: selected ? t.accent : t.zoneStroke,
          width: selected ? 3.6 : 2,
          style,
          jitter: 0.7,
        }),
      );
    }
    const font = fontFamily(look);
    const label = flatLabel(
      [
        {
          text: info.label.toUpperCase(),
          size: clean ? 11 : 13,
          color: t.muted,
          weight: clean ? 600 : 700,
          letterSpacing: clean ? 1.2 : 0,
        },
        { text: z.label ?? "", size: clean ? 22 : 26, weight: clean ? 600 : 400 },
      ],
      { padding: 0, font },
    );
    label.position.set(z.x + (clean ? 16 : 12), base + SLAB + 0.3, z.y + (clean ? 14 : 8));
    g.add(label);
    return g;
  }

  #buildNode(n: Node, base: number, selected: boolean, look: Look): THREE.Object3D {
    const t = theme();
    const info = NODE_KINDS[n.kind];
    const g = nodeModel(n.kind, n.color ?? info.color, info.height, {
      look,
      selected,
      seed: hashSeed(n.id),
    });
    g.position.set(n.x, base, n.y);
    if (n.label) {
      const clean = look === "clean";
      const label = labelSprite(
        [{ text: n.label, size: clean ? 15 : 18, weight: clean ? 600 : 400 }],
        clean
          ? { background: alpha(t.panel, 0.92), border: t.line, padding: 5, font: fontFamily(look) }
          : { background: alpha(t.panel, 0.7), font: fontFamily(look) },
      );
      label.position.set(0, info.height + (clean ? 12 : 10), 0);
      label.userData.pick = true;
      g.add(label);
    }
    // Generous invisible hit box so small models are easy to grab.
    const hit = new THREE.Mesh(
      new THREE.BoxGeometry(NODE_SIZE, info.height + 10, NODE_SIZE).translate(
        0,
        (info.height + 10) / 2,
        0,
      ),
      HIDDEN,
    );
    hit.userData.pick = true;
    g.add(hit);
    return g;
  }

  /** A tube along `curve`, broken up for dashed / dotted styles. */
  #tube(
    curve: THREE.Curve<THREE.Vector3>,
    from: number,
    to: number,
    radius: number,
    style: StrokeStyle | undefined,
    look: Look,
  ): THREE.BufferGeometry | null {
    const clean = look === "clean";
    const length = curve.getLength();
    const span = (to - from) * length;
    if (span <= 0.5) return null;
    const piece = (a: number, b: number) => {
      const steps = Math.max(2, Math.ceil(((b - a) * length) / 8));
      const pts = Array.from({ length: steps + 1 }, (_, i) =>
        curve.getPointAt(a + ((b - a) * i) / steps),
      );
      return new THREE.TubeGeometry(new THREE.CatmullRomCurve3(pts), steps, radius, clean ? 8 : 6);
    };
    if (style === "dotted") {
      const pieces: THREE.BufferGeometry[] = [];
      for (let d = 2; d < span; d += 9) {
        const p = curve.getPointAt(from + d / length);
        pieces.push(
          (clean
            ? new THREE.SphereGeometry(radius * 1.2, 8, 6)
            : new THREE.IcosahedronGeometry(radius * 1.1, 0)
          ).translate(p.x, p.y, p.z),
        );
      }
      return pieces.length > 0 ? mergeGeometries(pieces) : null;
    }
    const dash = dashPattern(style);
    if (!dash) return piece(from, to);
    const pieces: THREE.BufferGeometry[] = [];
    for (let d = 0; d < span; d += dash[0] + dash[1]) {
      const end = Math.min(d + dash[0], span);
      pieces.push(piece(from + d / length, from + end / length));
    }
    return mergeGeometries(pieces);
  }

  /** Cone arrow heads at the requested ends; returns how much of the curve they cover. */
  #arrowHeads(
    g: THREE.Group,
    curve: THREE.Curve<THREE.Vector3>,
    arrow: Arrow | undefined,
    material: THREE.Material,
    look: Look,
    size = 14,
  ): [number, number] {
    const length = curve.getLength();
    const cut = Math.min(size / length, 0.4);
    const head = (t: number, towards: 1 | -1) => {
      const tip = curve.getPointAt(t);
      const dir = curve.getTangentAt(t).multiplyScalar(towards);
      const cone = new THREE.Mesh(
        look === "clean"
          ? new THREE.ConeGeometry(size * 0.36, size, 20)
          : new THREE.ConeGeometry(size * 0.42, size, 10),
        material,
      );
      cone.position.copy(tip).addScaledVector(dir, -size / 2);
      cone.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), dir);
      cone.castShadow = true;
      cone.userData.solid = true;
      g.add(cone);
    };
    const start = arrow === "start" || arrow === "both";
    const end = arrow === "end" || arrow === "both";
    if (start) head(0, -1);
    if (end) head(1, 1);
    return [start ? cut : 0, end ? 1 - cut : 1];
  }

  #buildEdge(
    e: Edge,
    pts: Point[],
    [ya, yb]: [number, number],
    selected: boolean,
    look: Look,
  ): THREE.Object3D {
    const t = theme();
    const clean = look === "clean";
    const g = new THREE.Group();
    const color = selected ? t.accent : (e.color ?? t.ink);
    const material = solid(color, look);
    const [a, b] = [pts[0] as Point, pts[pts.length - 1] as Point];
    let curve: THREE.Curve<THREE.Vector3>;
    if (pts.length > 2 || e.route === "orthogonal") {
      // Follows the 2D route (bends included), level at the higher end, with short drops
      // to each endpoint; clean corners are rounded off.
      const top = Math.max(ya, yb);
      const v = pts.map(
        ([x, y], i) => new THREE.Vector3(x, i === 0 ? ya : i === pts.length - 1 ? yb : top, y),
      );
      curve = roundedPath(v, clean ? 14 : 0);
    } else {
      const start = new THREE.Vector3(a[0], ya, a[1]);
      const end = new THREE.Vector3(b[0], yb, b[1]);
      const mid = start.clone().lerp(end, 0.5);
      mid.y += THREE.MathUtils.clamp(start.distanceTo(end) * 0.22, 16, 120);
      curve = new THREE.QuadraticBezierCurve3(start, mid, end);
    }
    // Matches the 2D view and the Rust model: edges point at `to` by default.
    const arrow = e.arrow ?? "end";
    const [from, to] = this.#arrowHeads(g, curve, arrow, material, look, clean ? 13 : 14);
    const radius = clean ? (selected ? 2.6 : 1.6) : selected ? 3.2 : 2.2;
    const tube = this.#tube(curve, from, to, radius, e.style, look);
    if (tube) {
      const mesh = new THREE.Mesh(tube, material);
      mesh.castShadow = true;
      mesh.userData.solid = true;
      g.add(mesh);
    }
    const hit = new THREE.Mesh(new THREE.TubeGeometry(curve, 24, 9, 5), HIDDEN);
    hit.userData.pick = true;
    g.add(hit);

    if (e.label) {
      const label = labelSprite(
        [
          {
            text: e.label,
            size: clean ? 13 : 16,
            color: e.color ?? (clean ? t.muted : t.ink),
            weight: clean ? 500 : 400,
          },
        ],
        {
          background: alpha(t.panel, clean ? 0.94 : 0.9),
          border: clean ? t.line : undefined,
          padding: 5,
          font: fontFamily(look),
        },
      );
      label.position.copy(curve.getPointAt(0.5)).add(new THREE.Vector3(0, 6, 0));
      label.userData.pick = true;
      g.add(label);
    }

    const length = curve.getLength();
    const count = THREE.MathUtils.clamp(Math.round(length / 160), 1, 4);
    const packetMat = new THREE.MeshBasicMaterial({ color: e.color ?? t.accent });
    const meshes = Array.from({ length: count }, (_, i) => {
      const m = new THREE.Mesh(PACKET, packetMat);
      // Start on the edge, never at the origin, even before the first animation step.
      curve.getPointAt(i / count, m.position);
      m.visible = this.#store.state.animate;
      g.add(m);
      return m;
    });
    g.userData.packets = { curve, meshes, length, reverse: arrow === "start" } satisfies Packets;
    return g;
  }

  #buildLine(l: Line, bases: number[], selected: boolean, look: Look): THREE.Object3D {
    const t = theme();
    const g = new THREE.Group();
    const color = selected ? t.accent : (l.color ?? t.ink);
    const v = l.points.map(([x, y], i) => new THREE.Vector3(x, (bases[i] ?? 0) + 0.6, y));
    const path = new THREE.CurvePath<THREE.Vector3>();
    for (let i = 1; i < v.length; i++)
      path.add(new THREE.LineCurve3(v[i - 1] as THREE.Vector3, v[i] as THREE.Vector3));
    const material = solid(color, look);
    const [from, to] = this.#arrowHeads(g, path, l.arrow, material, look, 12);
    for (const c of g.children) {
      // Flat arrow heads lie on the ground, like ink on paper.
      c.scale.set(1, 1, 0.25);
    }
    const tube = this.#tube(path, from, to, selected ? 2.2 : 1.4, l.style, look);
    if (tube) g.add(new THREE.Mesh(tube, material));
    const hit = new THREE.Mesh(
      mergeGeometries(
        path.curves.map((c) => new THREE.TubeGeometry(c as THREE.LineCurve3, 1, 8, 4)),
      ),
      HIDDEN,
    );
    hit.userData.pick = true;
    g.add(hit);
    return g;
  }

  #buildNote(n: Note, base: number, selected: boolean, look: Look): THREE.Object3D {
    const t = theme();
    const size = TEXT_SIZES[n.size ?? "m"];
    const color = selected ? t.accent : (n.color ?? t.ink);
    const label = flatLabel(
      n.text.split("\n").map((text) => ({ text, size, color })),
      { padding: 0, font: fontFamily(look) },
    );
    label.position.set(n.x, base + 0.4, n.y);
    label.userData.pick = true;
    const g = new THREE.Group();
    g.add(label);
    return g;
  }

  // ---------------------------------------------------------------- input

  #ndc(e: { clientX: number; clientY: number }): THREE.Vector2 {
    const r = this.host.getBoundingClientRect();
    return new THREE.Vector2(
      ((e.clientX - r.left) / r.width) * 2 - 1,
      -((e.clientY - r.top) / r.height) * 2 + 1,
    );
  }

  #hit(e: { clientX: number; clientY: number }): Hit | null {
    this.#raycaster.setFromCamera(this.#ndc(e), this.#camera);
    for (const i of this.#raycaster.intersectObjects(this.#pickables, false)) {
      let o: THREE.Object3D | null = i.object;
      while (o && !o.userData.type) o = o.parent;
      if (o)
        return {
          id: o.userData.id as string,
          type: o.userData.type as ElementType,
          point: i.point,
        };
    }
    return null;
  }

  /** Where the pointer meets the horizontal plane at height `y`. */
  #onPlane(e: { clientX: number; clientY: number }, plane: THREE.Plane): THREE.Vector3 | null {
    this.#raycaster.setFromCamera(this.#ndc(e), this.#camera);
    return this.#raycaster.ray.intersectPlane(plane, new THREE.Vector3());
  }

  /** The spot on the ground or a zone under the pointer. */
  #groundAt(e: PointerEvent): Point | null {
    const hit = this.#hit(e);
    const p =
      hit?.type === "zone"
        ? hit.point
        : this.#onPlane(e, new THREE.Plane(new THREE.Vector3(0, 1, 0), 0));
    return p ? [p.x, p.z] : null;
  }

  #lock(e: PointerEvent): void {
    if (this.#controls) this.#controls.enabled = false;
    this.#renderer?.domElement.setPointerCapture(e.pointerId);
  }

  #onDown(e: PointerEvent): void {
    if (this.#controls) this.#controls.enabled = true;
    const tool = this.#store.state.tool;
    if (e.button !== 0 || tool.type === "hand") return;
    const hit = this.#hit(e);

    if (tool.type === "node") {
      const at = this.#groundAt(e);
      if (!at) return;
      this.#lock(e);
      let id = "";
      this.#store.edit((doc) => {
        id = addNode(doc, tool.kind, [snap(at[0]), snap(at[1])]).id;
      });
      this.#store.set({ selection: id, tool: { type: "select" } });
      return;
    }
    if (tool.type === "connect") {
      if (hit && isSelectable(hit.type)) {
        this.#lock(e);
        this.#drag = { kind: "connect", from: hit.id, start: hit.point };
      }
      return;
    }
    // Every other tool behaves as "select" here; zones, lines and notes are drawn in 2D.
    if (!hit) {
      this.#drag = { kind: "click", x: e.clientX, y: e.clientY };
      return;
    }
    this.#store.set({ selection: hit.id });
    if (!isSelectable(hit.type)) return;
    this.#lock(e);
    const doc = this.#store.doc;
    const found = findElement(doc, hit.id);
    const base =
      found?.type === "zone"
        ? zoneDepth(doc, zoneBox(found.el), hit.id) * SLAB
        : found?.type === "node"
          ? zoneDepth(doc, nodeBox(found.el)) * SLAB
          : 0;
    const plane = new THREE.Plane(new THREE.Vector3(0, 1, 0), -base);
    const p = this.#onPlane(e, plane);
    if (!p) return;
    this.#drag = {
      kind: "move",
      id: hit.id,
      plane,
      last: [snap(p.x), snap(p.z)],
      moved: false,
      contents: zoneContents(doc, hit.id),
    };
  }

  #onMove(e: PointerEvent): void {
    const d = this.#drag;
    if (d?.kind === "move") {
      const p = this.#onPlane(e, d.plane);
      if (!p) return;
      const at: Point = [snap(p.x, !e.altKey), snap(p.z, !e.altKey)];
      const dx = at[0] - d.last[0];
      const dy = at[1] - d.last[1];
      if (dx === 0 && dy === 0) return;
      if (!d.moved) this.#store.checkpoint();
      d.moved = true;
      d.last = at;
      this.#store.mutate((doc) =>
        moveElement(doc, d.id, dx, dy, e.shiftKey ? new Set() : d.contents),
      );
    } else if (d?.kind === "connect") {
      const p =
        this.#hit(e)?.point ?? this.#onPlane(e, new THREE.Plane(new THREE.Vector3(0, 1, 0), 0));
      if (p) this.#showPreview(d.start, p);
    }
  }

  #onUp(e: PointerEvent): void {
    const d = this.#drag;
    this.#drag = null;
    this.#showPreview(null);
    const canvas = this.#renderer?.domElement;
    if (canvas?.hasPointerCapture(e.pointerId)) canvas.releasePointerCapture(e.pointerId);
    if (this.#controls) this.#controls.enabled = true;
    if (d?.kind === "click" && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 4) {
      this.#store.set({ selection: null });
    } else if (d?.kind === "connect") {
      const hit = this.#hit(e);
      if (hit && isSelectable(hit.type) && hit.id !== d.from) {
        let id = "";
        this.#store.edit((doc) => {
          id = addEdge(doc, d.from, hit.id)?.id ?? "";
        });
        this.#store.set({ selection: id || null });
      }
    }
  }

  #onDoubleClick(e: MouseEvent): void {
    const hit = this.#hit(e);
    if (!hit) return;
    this.#store.set({ selection: hit.id });
    window.dispatchEvent(new CustomEvent("infraplot:focus-label"));
  }

  #showPreview(from: THREE.Vector3 | null, to?: THREE.Vector3): void {
    if (this.#preview) this.#dispose(this.#preview);
    this.#preview = null;
    if (from && to) {
      this.#preview = ink([from.x, from.y, from.z, to.x, to.y, to.z], () => 0.5, {
        color: theme().accent,
        width: 2,
        style: "dashed",
        jitter: 0,
      });
      this.#scene.add(this.#preview);
      this.#lineResolution(this.#size.w, this.#size.h);
    }
    this.#dirty = true;
  }

  // ---------------------------------------------------------------- export

  /**
   * A high-resolution "photo" of the current view: same framing, `scale`× the
   * pixels (capped at 4096 px), without the selection highlight.
   */
  async exportPng(scale = 3): Promise<Blob> {
    if (!this.#renderer || this.#size.w === 0) throw new Error("the 3D view is not ready");
    const { w, h } = this.#size;
    const k = Math.min(scale, MAX_EXPORT_SIZE / Math.max(w, h));
    const width = Math.round(w * k);
    const height = Math.round(h * k);
    const renderer = makeRenderer(true);
    try {
      renderer.setPixelRatio(1);
      renderer.setSize(width, height, false);
      this.#sync(null);
      // Pixel-wide strokes and outlines scale with the photo, like everything else.
      this.#lineResolution(w, h);
      renderer.render(this.#scene, this.#camera);
      return await new Promise<Blob>((resolve, reject) =>
        renderer.domElement.toBlob(
          (b) => (b ? resolve(b) : reject(new Error("PNG encoding failed"))),
          "image/png",
        ),
      );
    } finally {
      renderer.dispose();
      renderer.forceContextLoss();
      this.#sync();
      this.#lineResolution(w, h);
      this.#dirty = true;
    }
  }
}
