/** The 2D, hand-drawn editor surface: an SVG scene sketched with rough.js. */
import rough from "roughjs";
import type { Options } from "roughjs/bin/core";
import type { RoughSVG } from "roughjs/bin/svg";
import { ACCENT, GRID, INK, NODE_KINDS, NODE_SIZE, TEXT_SIZES, ZONE_KINDS } from "../model/catalog";
import type { Arrow, Doc, Edge, Line, Node, Note, StrokeStyle, Zone } from "../model/doc";
import { findElement, uniqueId } from "../model/doc";
import {
  type Box,
  docBounds,
  edgePath,
  hashSeed,
  nodeBox,
  noteBox,
  type Point,
  pointAlong,
  snap,
  zoneBox,
} from "../model/geometry";
import { addEdge, addNode, addZone, moveElement } from "../state/ops";
import type { Store } from "../state/store";
import { arrowHead, drawGlyph, svgText } from "./glyphs";

const SVG_NS = "http://www.w3.org/2000/svg";

interface Camera {
  x: number;
  y: number;
  zoom: number;
}

type Handle = "nw" | "ne" | "sw" | "se";

type Drag =
  | { kind: "pan"; sx: number; sy: number; cam: Camera }
  | { kind: "move"; id: string; last: Point; moved: boolean; free: boolean }
  | { kind: "resize"; id: string; handle: Handle; orig: Box; start: Point; moved: boolean }
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
    this.#gridPattern.appendChild(el("circle", { cx: 1, cy: 1, r: 1, fill: "#d0d0d8" }));
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

  render(): void {
    const { doc, selection } = this.#store.state;
    const seen = new Set<string>();
    const place = (layer: LayerName, id: string, key: string, draw: () => SVGGElement) => {
      seen.add(id);
      const fullKey = `${this.#generation}|${key}`;
      let entry = this.#cache.get(id);
      if (!entry || entry.key !== fullKey) {
        entry?.g.remove();
        entry = { key: fullKey, g: draw() };
        this.#cache.set(id, entry);
      }
      this.#layers[layer].appendChild(entry.g); // re-appending keeps document order
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

  #base(id: string, color: string, extra: Options = {}): Options {
    return {
      seed: hashSeed(id),
      roughness: 1.1,
      bowing: 1,
      stroke: INK,
      strokeWidth: 1.6,
      fill: color,
      fillStyle: "hachure",
      hachureGap: 5,
      fillWeight: 1.4,
      ...extra,
    };
  }

  #group(type: string, id: string): SVGGElement {
    return el("g", { class: `el el-${type}`, "data-id": id, "data-type": type });
  }

  #drawZone(z: Zone): SVGGElement {
    const info = ZONE_KINDS[z.kind ?? "generic"];
    const color = z.color ?? info.color;
    const g = this.#group("zone", z.id);
    g.appendChild(
      el("rect", { x: z.x, y: z.y, width: z.w, height: z.h, fill: "transparent", class: "hit" }),
    );
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
            stroke: "#495057",
          }),
          z.style ?? info.style,
        ),
      ),
    );
    const tag = info.label.toUpperCase();
    g.appendChild(
      svgText(z.x + 14, z.y + 16, tag, 11, { anchor: "start", color: "#868e96", weight: 700 }),
    );
    g.appendChild(svgText(z.x + 14, z.y + 36, z.label ?? "", 20, { anchor: "start" }));
    return g;
  }

  #drawNode(n: Node): SVGGElement {
    const info = NODE_KINDS[n.kind];
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
    g.appendChild(
      drawGlyph(this.#rc, n.kind, n.x, n.y, NODE_SIZE, this.#base(n.id, n.color ?? info.color)),
    );
    const label = svgText(n.x, b.y + b.h + 16, n.label ?? "", 18);
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
    const g = this.#group("edge", e.id);
    g.appendChild(this.#hitPath(pts));
    // Matches the Rust model: edges point at `to` unless told otherwise.
    g.appendChild(this.#strokes(e.id, pts, e.style, e.arrow ?? "end", e.color ?? INK));
    if (e.label) {
      const [mx, my] = pointAlong(pts, 0.5);
      const w = e.label.length * 16 * 0.55 + 12;
      g.appendChild(
        el("rect", {
          x: mx - w / 2,
          y: my - 12,
          width: w,
          height: 24,
          rx: 6,
          fill: "#ffffff",
          opacity: 0.9,
        }),
      );
      g.appendChild(svgText(mx, my, e.label, 16, { color: e.color ?? "#495057" }));
    }
    return g;
  }

  #drawLine(l: Line): SVGGElement {
    const g = this.#group("line", l.id);
    g.appendChild(this.#hitPath(l.points));
    g.appendChild(this.#strokes(l.id, l.points, l.style, l.arrow, l.color ?? INK));
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
        svgText(n.x, n.y + size * 0.62 + i * size * 1.25, line, size, {
          anchor: "start",
          color: n.color ?? INK,
        }),
      );
    });
    return g;
  }

  #renderOverlay(doc: Doc, selection: string | null): void {
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
            stroke: ACCENT,
            "stroke-width": 1.5 / this.#camera.zoom,
            "stroke-dasharray": "6 4",
            rx: 4,
            class: "selection-box",
          }),
        );
      }
      if (found.type === "zone" && b) {
        const size = 10 / this.#camera.zoom;
        const corners: [Handle, number, number][] = [
          ["nw", b.x, b.y],
          ["ne", b.x + b.w, b.y],
          ["sw", b.x, b.y + b.h],
          ["se", b.x + b.w, b.y + b.h],
        ];
        for (const [h, x, y] of corners) {
          o.appendChild(
            el("rect", {
              x: x - size / 2,
              y: y - size / 2,
              width: size,
              height: size,
              fill: "#fff",
              stroke: ACCENT,
              "stroke-width": 1.5 / this.#camera.zoom,
              rx: 2,
              "data-handle": h,
              "data-testid": `handle-${h}`,
              class: `handle handle-${h}`,
            }),
          );
        }
      }
      if (found.type === "edge" || found.type === "line") {
        const pts = found.type === "edge" ? edgePath(doc, found.el) : found.el.points;
        if (pts) {
          o.appendChild(
            el("polyline", {
              points: pts.map((p) => p.join(",")).join(" "),
              fill: "none",
              stroke: ACCENT,
              "stroke-opacity": 0.35,
              "stroke-width": 8,
              "stroke-linecap": "round",
              "pointer-events": "none",
            }),
          );
        }
      }
    }

    const d = this.#drag;
    const preview = {
      fill: "none",
      stroke: ACCENT,
      "stroke-width": 1.5,
      "stroke-dasharray": "6 4",
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
  }): { id: string; type: string } | { handle: Handle } | null {
    const target = document.elementFromPoint(e.clientX, e.clientY);
    const handle = target?.closest<SVGElement>("[data-handle]");
    if (handle?.dataset.handle) return { handle: handle.dataset.handle as Handle };
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
          const z = sel ? this.#store.doc.zones.find((z) => z.id === sel) : undefined;
          if (z)
            this.#drag = {
              kind: "resize",
              id: z.id,
              handle: hit.handle,
              orig: zoneBox(z),
              start: world,
              moved: false,
            };
          return;
        }
        if (hit) {
          this.#store.set({ selection: hit.id });
          this.#drag = { kind: "move", id: hit.id, last: snapped, moved: false, free };
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
          this.#lineDraft.push(snapped);
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
        requestFocusLabel();
        return;
      }
    }
  }

  #onMove(e: PointerEvent): void {
    const world = this.toWorld(e.clientX, e.clientY);
    this.#cursor = [snap(world[0], !e.altKey), snap(world[1], !e.altKey)];
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
        this.#store.mutate((doc) => moveElement(doc, d.id, dx, dy, !e.shiftKey));
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
      case "zone":
      case "line-drag":
        d.cur = this.#cursor;
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
      fill: "#ffffff",
    });
    clone.insertBefore(bg, clone.querySelector(".viewport"));
    return new XMLSerializer().serializeToString(clone);
  }
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
