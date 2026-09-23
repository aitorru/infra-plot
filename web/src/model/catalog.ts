import type { NodeKind, StrokeStyle, ZoneKind } from "./generated";

export interface NodeKindInfo {
  readonly label: string;
  readonly color: string;
  readonly group: "compute" | "network" | "data" | "other";
  /** Height of the 3D model in world units. */
  readonly height: number;
}

export const NODE_SIZE = 72;
export const GRID = 20;

export const NODE_KINDS: Readonly<Record<NodeKind, NodeKindInfo>> = {
  service: { label: "Service", color: "#a5d8ff", group: "compute", height: 40 },
  server: { label: "Server", color: "#b2f2bb", group: "compute", height: 80 },
  vm: { label: "VM", color: "#c3fae8", group: "compute", height: 56 },
  container: { label: "Container", color: "#99e9f2", group: "compute", height: 44 },
  pod: { label: "Pod", color: "#bac8ff", group: "compute", height: 36 },
  k8s: { label: "Kubernetes", color: "#74c0fc", group: "compute", height: 32 },
  function: { label: "Function", color: "#ffd8a8", group: "compute", height: 40 },
  proxy: { label: "Proxy", color: "#d0bfff", group: "network", height: 28 },
  "load-balancer": { label: "Load balancer", color: "#eebefa", group: "network", height: 28 },
  "api-gateway": { label: "API gateway", color: "#e5dbff", group: "network", height: 52 },
  firewall: { label: "Firewall", color: "#ffc9c9", group: "network", height: 48 },
  cdn: { label: "CDN", color: "#ffec99", group: "network", height: 64 },
  dns: { label: "DNS", color: "#d8f5a2", group: "network", height: 24 },
  database: { label: "Database", color: "#ffd43b", group: "data", height: 64 },
  cache: { label: "Cache", color: "#ff8787", group: "data", height: 36 },
  queue: { label: "Queue", color: "#ffa94d", group: "data", height: 28 },
  storage: { label: "Storage", color: "#ced4da", group: "data", height: 48 },
  user: { label: "User", color: "#fcc2d7", group: "other", height: 64 },
  internet: { label: "Internet", color: "#e7f5ff", group: "other", height: 44 },
  monitoring: { label: "Monitoring", color: "#96f2d7", group: "other", height: 56 },
};

export interface ZoneKindInfo {
  readonly label: string;
  readonly color: string;
  readonly style: StrokeStyle;
}

export const ZONE_KINDS: Readonly<Record<ZoneKind, ZoneKindInfo>> = {
  generic: { label: "Zone", color: "#e9ecef", style: "dashed" },
  region: { label: "Region", color: "#fff3bf", style: "solid" },
  vpc: { label: "VPC", color: "#d3f9d8", style: "solid" },
  subnet: { label: "Subnet", color: "#e7f5ff", style: "dashed" },
  dmz: { label: "DMZ", color: "#ffe3e3", style: "dashed" },
  "k8s-cluster": { label: "K8s cluster", color: "#d0ebff", style: "solid" },
  namespace: { label: "Namespace", color: "#edf2ff", style: "dotted" },
  "on-prem": { label: "On-prem", color: "#f3f0ff", style: "solid" },
};

export const INK = "#1e1e1e";
export const ACCENT = "#6965db";

export const TEXT_SIZES = { s: 16, m: 22, l: 32, xl: 48 } as const;

export const FONT_FAMILY = "Kalam, 'Comic Sans MS', cursive";

export function nodeKinds(): NodeKind[] {
  return Object.keys(NODE_KINDS) as NodeKind[];
}

export function zoneKinds(): ZoneKind[] {
  return Object.keys(ZONE_KINDS) as ZoneKind[];
}
