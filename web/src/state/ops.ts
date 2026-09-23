/** Document operations shared by the 2D and 3D editors. */
import { NODE_KINDS, ZONE_KINDS } from "../model/catalog";
import type { Doc, Edge, Node, NodeKind, Zone, ZoneKind } from "../model/doc";
import { findElement, uniqueId } from "../model/doc";
import { contains, nodeBox, noteBox, type Point, snap, zoneBox } from "../model/geometry";

export function addNode(doc: Doc, kind: NodeKind, [x, y]: Point): Node {
  const node: Node = {
    id: uniqueId(doc, kind),
    kind,
    label: NODE_KINDS[kind].label,
    x: snap(x),
    y: snap(y),
  };
  doc.nodes.push(node);
  return node;
}

export function addZone(
  doc: Doc,
  kind: ZoneKind,
  x: number,
  y: number,
  w: number,
  h: number,
): Zone {
  const zone: Zone = {
    id: uniqueId(doc, kind === "generic" ? "zone" : kind),
    kind,
    label: ZONE_KINDS[kind].label,
    x,
    y,
    w,
    h,
  };
  // Bigger zones first so smaller ones render on top of them.
  const idx = doc.zones.findIndex((z) => z.w * z.h < w * h);
  if (idx === -1) doc.zones.push(zone);
  else doc.zones.splice(idx, 0, zone);
  return zone;
}

export function addEdge(doc: Doc, from: string, to: string): Edge | undefined {
  if (from === to) return undefined;
  const existing = doc.edges.find((e) => e.from === from && e.to === to);
  if (existing) return existing;
  const edge: Edge = {
    id: uniqueId(doc, "edge"),
    from,
    to,
    style: "solid",
    arrow: "end",
    route: "straight",
  };
  doc.edges.push(edge);
  return edge;
}

/** Ids of everything fully inside zone `id`: what moving the zone drags along. */
export function zoneContents(doc: Doc, id: string): Set<string> {
  const zone = doc.zones.find((z) => z.id === id);
  const inside = new Set<string>();
  if (!zone) return inside;
  const outer = zoneBox(zone);
  for (const z of doc.zones) if (z !== zone && contains(outer, zoneBox(z))) inside.add(z.id);
  for (const n of doc.nodes) if (contains(outer, nodeBox(n))) inside.add(n.id);
  for (const n of doc.notes) if (contains(outer, noteBox(n))) inside.add(n.id);
  for (const l of doc.lines) {
    if (l.points.every(([x, y]) => contains(outer, { x, y, w: 0, h: 0 }))) inside.add(l.id);
  }
  return inside;
}

/**
 * Moves an element by (dx, dy). Moving a zone drags along its contents, so a
 * VPC can be repositioned with everything in it. Drags pass the contents taken
 * when they started, so the zone doesn't sweep up what it passes over.
 */
export function moveElement(
  doc: Doc,
  id: string,
  dx: number,
  dy: number,
  contents: ReadonlySet<string> = zoneContents(doc, id),
): void {
  const found = findElement(doc, id);
  if (!found) return;
  switch (found.type) {
    case "node":
    case "note":
      found.el.x += dx;
      found.el.y += dy;
      break;
    case "line":
      found.el.points = found.el.points.map(([x, y]) => [x + dx, y + dy]);
      break;
    case "edge":
      break;
    case "zone":
      for (const other of [...doc.zones, ...doc.nodes, ...doc.notes]) {
        if (contents.has(other.id)) {
          other.x += dx;
          other.y += dy;
        }
      }
      for (const l of doc.lines) {
        if (contents.has(l.id)) l.points = l.points.map(([x, y]) => [x + dx, y + dy]);
      }
      found.el.x += dx;
      found.el.y += dy;
      break;
  }
}

export function deleteElement(doc: Doc, id: string): void {
  doc.zones = doc.zones.filter((z) => z.id !== id);
  doc.nodes = doc.nodes.filter((n) => n.id !== id);
  doc.lines = doc.lines.filter((l) => l.id !== id);
  doc.notes = doc.notes.filter((n) => n.id !== id);
  doc.edges = doc.edges.filter((e) => e.id !== id && e.from !== id && e.to !== id);
}

/** Copies an element next to the original and returns the copy's id. */
export function duplicateElement(doc: Doc, id: string): string | undefined {
  const found = findElement(doc, id);
  if (!found || found.type === "edge") return undefined;
  const offset = 40;
  switch (found.type) {
    case "node": {
      const copy = { ...structuredClone(found.el), id: uniqueId(doc, found.el.kind) };
      copy.x += offset;
      copy.y += offset;
      doc.nodes.push(copy);
      return copy.id;
    }
    case "zone": {
      const copy = { ...structuredClone(found.el), id: uniqueId(doc, "zone") };
      copy.x += offset;
      copy.y += offset;
      doc.zones.push(copy);
      return copy.id;
    }
    case "note": {
      const copy = { ...structuredClone(found.el), id: uniqueId(doc, "note") };
      copy.x += offset;
      copy.y += offset;
      doc.notes.push(copy);
      return copy.id;
    }
    case "line": {
      const copy = { ...structuredClone(found.el), id: uniqueId(doc, "line") };
      copy.points = copy.points.map(([x, y]) => [x + offset, y + offset]);
      doc.lines.push(copy);
      return copy.id;
    }
  }
}

/** Renames an element and rewires edges pointing at it. Returns false if taken. */
export function renameId(doc: Doc, from: string, to: string): boolean {
  if (to === from) return true;
  if (!/^\S+$/.test(to) || findElement(doc, to)) return false;
  const found = findElement(doc, from);
  if (!found) return false;
  found.el.id = to;
  for (const e of doc.edges) {
    if (e.from === from) e.from = to;
    if (e.to === from) e.to = to;
  }
  return true;
}
