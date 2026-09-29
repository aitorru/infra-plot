import type { Look, NodeKind, StrokeStyle, ZoneKind } from "./generated";

export type NodeGroup = "generic" | "compute" | "network" | "data" | "security" | "ops" | "clients";

/** Palette sections, in display order. */
export const NODE_GROUPS: Readonly<Record<NodeGroup, string>> = {
  generic: "Generic",
  compute: "Compute",
  network: "Network",
  data: "Data",
  security: "Security",
  ops: "Operations",
  clients: "Clients",
};

export interface NodeKindInfo {
  readonly label: string;
  readonly color: string;
  readonly group: NodeGroup;
  /** Height of the 3D model in world units. */
  readonly height: number;
}

export const NODE_SIZE = 72;
export const GRID = 20;

export const NODE_KINDS: Readonly<Record<NodeKind, NodeKindInfo>> = {
  component: { label: "Component", color: "#dee2e6", group: "generic", height: 36 },
  service: { label: "Service", color: "#a5d8ff", group: "compute", height: 40 },
  server: { label: "Server", color: "#b2f2bb", group: "compute", height: 80 },
  vm: { label: "VM", color: "#c3fae8", group: "compute", height: 56 },
  container: { label: "Container", color: "#99e9f2", group: "compute", height: 44 },
  pod: { label: "Pod", color: "#bac8ff", group: "compute", height: 36 },
  k8s: { label: "Kubernetes", color: "#74c0fc", group: "compute", height: 32 },
  function: { label: "Function", color: "#ffd8a8", group: "compute", height: 40 },
  worker: { label: "Worker", color: "#d8f5a2", group: "compute", height: 44 },
  scheduler: { label: "Scheduler", color: "#ffe8cc", group: "compute", height: 40 },
  gpu: { label: "GPU", color: "#b197fc", group: "compute", height: 36 },
  proxy: { label: "Proxy", color: "#d0bfff", group: "network", height: 28 },
  "load-balancer": { label: "Load balancer", color: "#eebefa", group: "network", height: 28 },
  "api-gateway": { label: "API gateway", color: "#e5dbff", group: "network", height: 52 },
  gateway: { label: "Gateway", color: "#dbe4ff", group: "network", height: 40 },
  router: { label: "Router", color: "#c5f6fa", group: "network", height: 24 },
  switch: { label: "Switch", color: "#d3f9d8", group: "network", height: 16 },
  vpn: { label: "VPN", color: "#e3fafc", group: "network", height: 40 },
  cdn: { label: "CDN", color: "#ffec99", group: "network", height: 64 },
  dns: { label: "DNS", color: "#d8f5a2", group: "network", height: 24 },
  database: { label: "Database", color: "#ffd43b", group: "data", height: 64 },
  cache: { label: "Cache", color: "#ff8787", group: "data", height: 36 },
  queue: { label: "Queue", color: "#ffa94d", group: "data", height: 28 },
  stream: { label: "Stream", color: "#ffc078", group: "data", height: 24 },
  storage: { label: "Storage", color: "#ced4da", group: "data", height: 48 },
  bucket: { label: "Bucket", color: "#ffe066", group: "data", height: 44 },
  warehouse: { label: "Warehouse", color: "#e9c46a", group: "data", height: 52 },
  search: { label: "Search", color: "#ffdeeb", group: "data", height: 40 },
  firewall: { label: "Firewall", color: "#ffc9c9", group: "security", height: 48 },
  identity: { label: "Identity", color: "#fcc2d7", group: "security", height: 44 },
  secrets: { label: "Secrets", color: "#ffd8a8", group: "security", height: 44 },
  monitoring: { label: "Monitoring", color: "#96f2d7", group: "ops", height: 56 },
  logging: { label: "Logging", color: "#c3fae8", group: "ops", height: 40 },
  "ci-cd": { label: "CI/CD", color: "#b2f2bb", group: "ops", height: 32 },
  registry: { label: "Registry", color: "#a5d8ff", group: "ops", height: 44 },
  notification: { label: "Notification", color: "#ffec99", group: "ops", height: 40 },
  user: { label: "User", color: "#fcc2d7", group: "clients", height: 64 },
  client: { label: "Client", color: "#dee2e6", group: "clients", height: 48 },
  mobile: { label: "Mobile", color: "#e5dbff", group: "clients", height: 56 },
  browser: { label: "Browser", color: "#d0ebff", group: "clients", height: 48 },
  internet: { label: "Internet", color: "#e7f5ff", group: "clients", height: 44 },
  external: { label: "External", color: "#f1f3f5", group: "clients", height: 40 },
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
  "availability-zone": { label: "Availability zone", color: "#fff9db", style: "dashed" },
  account: { label: "Account", color: "#f8f0fc", style: "solid" },
  "security-group": { label: "Security group", color: "#fff5f5", style: "dotted" },
  "k8s-cluster": { label: "K8s cluster", color: "#d0ebff", style: "solid" },
  namespace: { label: "Namespace", color: "#edf2ff", style: "dotted" },
  "on-prem": { label: "On-prem", color: "#f3f0ff", style: "solid" },
  "data-center": { label: "Data center", color: "#f1f3f5", style: "solid" },
};

export const INK = "#1e1e1e";
export const ACCENT = "#6965db";

export const TEXT_SIZES = { s: 16, m: 22, l: 32, xl: 48 } as const;

/** Hand-drawn lettering, for the `sketch` look. */
export const FONT_FAMILY = "Kalam, 'Comic Sans MS', cursive";
/** Neutral sans-serif, for the UI and the `clean` look. */
export const UI_FONT_FAMILY =
  "'Inter Variable', Inter, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif";

export function fontFamily(look: Look): string {
  return look === "sketch" ? FONT_FAMILY : UI_FONT_FAMILY;
}

export function nodeKinds(): NodeKind[] {
  return Object.keys(NODE_KINDS) as NodeKind[];
}

export function zoneKinds(): ZoneKind[] {
  return Object.keys(ZONE_KINDS) as ZoneKind[];
}
