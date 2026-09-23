import Ajv2020 from "ajv/dist/2020";
import { parse as parseToml, stringify as stringifyToml } from "smol-toml";
import schema from "../../../schema/diagram.schema.json";
import type { Diagram, Edge, Line, Node, Note, Zone } from "./generated";

export type { Arrow, Edge, Line, Node, NodeKind, Note, Route, StrokeStyle, TextSize, Zone, ZoneKind } from "./generated";

export const FORMAT_VERSION = 1;

/** A diagram with every optional collection filled in; what the editor works on. */
export interface Doc {
  version: number;
  title: string;
  description: string;
  zones: Zone[];
  nodes: Node[];
  edges: Edge[];
  lines: Line[];
  notes: Note[];
}

export type Element =
  | { type: "zone"; el: Zone }
  | { type: "node"; el: Node }
  | { type: "edge"; el: Edge }
  | { type: "line"; el: Line }
  | { type: "note"; el: Note };

export type ElementType = Element["type"];

export type Format = "json" | "toml";

export interface Issue {
  path: string;
  message: string;
}

export type ParseResult = { ok: true; doc: Doc } | { ok: false; issues: Issue[] };

export function emptyDoc(title = "Untitled diagram"): Doc {
  return {
    version: FORMAT_VERSION,
    title,
    description: "",
    zones: [],
    nodes: [],
    edges: [],
    lines: [],
    notes: [],
  };
}

export function normalize(d: Diagram): Doc {
  return {
    version: d.version,
    title: d.title,
    description: d.description ?? "",
    zones: d.zones ?? [],
    nodes: d.nodes ?? [],
    edges: d.edges ?? [],
    lines: d.lines ?? [],
    notes: d.notes ?? [],
  };
}

/** Drops empty/null values so files stay small and TOML-friendly. */
function prune(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(prune);
  if (value !== null && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      if (v === null || v === undefined) continue;
      if (Array.isArray(v) && v.length === 0) continue;
      if (typeof v === "object" && !Array.isArray(v) && Object.keys(v).length === 0) continue;
      if (v === "" && (k === "description" || k === "label")) continue;
      out[k] = prune(v);
    }
    return out;
  }
  return value;
}

export function toDiagram(doc: Doc): Diagram {
  // Fixed key order keeps saved files diff-friendly.
  const ordered = {
    version: doc.version,
    title: doc.title,
    description: doc.description,
    zones: doc.zones,
    nodes: doc.nodes,
    edges: doc.edges,
    lines: doc.lines,
    notes: doc.notes,
  };
  return prune(ordered) as Diagram;
}

export function serialize(doc: Doc, format: Format): string {
  const d = toDiagram(doc);
  return format === "json"
    ? `${JSON.stringify(d, null, 2)}\n`
    : stringifyToml(d as Record<string, unknown>);
}

const ajv = new Ajv2020({ allErrors: true, strict: false, validateFormats: false });
const validateSchema = ajv.compile<Diagram>(schema);

export function detectFormat(name: string, text: string): Format {
  if (name.endsWith(".toml")) return "toml";
  if (name.endsWith(".json")) return "json";
  return text.trimStart().startsWith("{") ? "json" : "toml";
}

export function parseDocument(text: string, format: Format): ParseResult {
  let raw: unknown;
  try {
    raw = format === "json" ? JSON.parse(text) : parseToml(text);
  } catch (e) {
    return { ok: false, issues: [{ path: "", message: `invalid ${format.toUpperCase()}: ${String(e)}` }] };
  }
  if (!validateSchema(raw)) {
    const issues = (validateSchema.errors ?? []).map((err) => ({
      path: err.instancePath.replace(/^\//, "").replaceAll("/", ".") || "(root)",
      message: err.message ?? "invalid",
    }));
    return { ok: false, issues };
  }
  const doc = normalize(raw);
  const issues = validate(doc);
  return issues.length > 0 ? { ok: false, issues } : { ok: true, doc };
}

const COLOR = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i;

/** Semantic checks, mirroring `Diagram::validate` in the Rust model crate. */
export function validate(doc: Doc): Issue[] {
  const issues: Issue[] = [];
  const seen = new Map<string, string>();
  const push = (path: string, message: string) => issues.push({ path, message });
  const check = (path: string, el: { id: string; color?: string | null }, coords: number[]) => {
    if (el.id === "" || /\s/.test(el.id)) {
      push(`${path}.id`, "ids must be non-empty and contain no whitespace");
    } else if (seen.has(el.id)) {
      push(`${path}.id`, `duplicate id \`${el.id}\` (also used by ${seen.get(el.id)})`);
    } else {
      seen.set(el.id, path);
    }
    if (coords.some((n) => !Number.isFinite(n))) push(path, "coordinates must be finite numbers");
    if (el.color != null && !COLOR.test(el.color)) {
      push(`${path}.color`, `\`${el.color}\` is not a #rgb or #rrggbb colour`);
    }
  };

  if (doc.version !== FORMAT_VERSION) {
    push("version", `unsupported version ${doc.version}, expected ${FORMAT_VERSION}`);
  }
  doc.zones.forEach((z, i) => {
    check(`zones[${i}]`, z, [z.x, z.y, z.w, z.h]);
    if (z.w <= 0 || z.h <= 0) push(`zones[${i}]`, "zone width and height must be positive");
  });
  doc.nodes.forEach((n, i) => {
    check(`nodes[${i}]`, n, [n.x, n.y]);
  });
  const targets = new Set([...doc.zones.map((z) => z.id), ...doc.nodes.map((n) => n.id)]);
  doc.edges.forEach((e, i) => {
    check(`edges[${i}]`, e, []);
    if (!targets.has(e.from)) push(`edges[${i}].from`, `unknown node or zone \`${e.from}\``);
    if (!targets.has(e.to)) push(`edges[${i}].to`, `unknown node or zone \`${e.to}\``);
    if (e.from === e.to) push(`edges[${i}]`, "an edge cannot connect an element to itself");
  });
  doc.lines.forEach((l, i) => {
    check(`lines[${i}]`, l, l.points.flat());
    if (l.points.length < 2) push(`lines[${i}].points`, "a line needs at least two points");
  });
  doc.notes.forEach((n, i) => {
    check(`notes[${i}]`, n, [n.x, n.y]);
  });
  return issues;
}

export function findElement(doc: Doc, id: string): Element | undefined {
  const zone = doc.zones.find((z) => z.id === id);
  if (zone) return { type: "zone", el: zone };
  const node = doc.nodes.find((n) => n.id === id);
  if (node) return { type: "node", el: node };
  const edge = doc.edges.find((e) => e.id === id);
  if (edge) return { type: "edge", el: edge };
  const line = doc.lines.find((l) => l.id === id);
  if (line) return { type: "line", el: line };
  const note = doc.notes.find((n) => n.id === id);
  if (note) return { type: "note", el: note };
  return undefined;
}

export function allIds(doc: Doc): Set<string> {
  return new Set(
    [doc.zones, doc.nodes, doc.edges, doc.lines, doc.notes].flatMap((list) =>
      list.map((el: { id: string }) => el.id),
    ),
  );
}

export function uniqueId(doc: Doc, prefix: string): string {
  const ids = allIds(doc);
  const base = prefix.toLowerCase().replace(/[^a-z0-9_-]+/g, "-") || "el";
  for (let n = 1; ; n++) {
    const id = `${base}-${n}`;
    if (!ids.has(id)) return id;
  }
}
