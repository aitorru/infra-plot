/* Generated from schema/diagram.schema.json by `gen-schema`. Do not edit. */

/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "NodeKind".
 */
export type NodeKind =
  | "component"
  | "service"
  | "server"
  | "vm"
  | "container"
  | "pod"
  | "k8s"
  | "function"
  | "worker"
  | "scheduler"
  | "gpu"
  | "proxy"
  | "load-balancer"
  | "api-gateway"
  | "gateway"
  | "router"
  | "switch"
  | "vpn"
  | "cdn"
  | "dns"
  | "database"
  | "cache"
  | "queue"
  | "stream"
  | "storage"
  | "bucket"
  | "warehouse"
  | "search"
  | "firewall"
  | "identity"
  | "secrets"
  | "monitoring"
  | "logging"
  | "ci-cd"
  | "registry"
  | "notification"
  | "user"
  | "client"
  | "mobile"
  | "browser"
  | "internet"
  | "external";
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "StrokeStyle".
 */
export type StrokeStyle = "solid" | "dashed" | "dotted";
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "Arrow".
 */
export type Arrow = "none" | "end" | "start" | "both";
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "Look".
 */
export type Look = "clean" | "sketch";
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "Route".
 */
export type Route = "straight" | "orthogonal";
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "TextSize".
 */
export type TextSize = "s" | "m" | "l" | "xl";
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "ZoneKind".
 */
export type ZoneKind =
  | "generic"
  | "region"
  | "vpc"
  | "subnet"
  | "dmz"
  | "availability-zone"
  | "account"
  | "security-group"
  | "k8s-cluster"
  | "namespace"
  | "on-prem"
  | "data-center";

/**
 * An infrastructure diagram.
 */
export interface Diagram {
  description?: string;
  /**
   * Connections between nodes and/or zones.
   */
  edges?: Edge[];
  /**
   * Free-form polylines.
   */
  lines?: Line[];
  /**
   * How the diagram is drawn: crisp vector shapes (`clean`) or hand-drawn (`sketch`).
   */
  look?: "clean" | "sketch";
  /**
   * Infrastructure components.
   */
  nodes?: Node[];
  /**
   * Free-floating text.
   */
  notes?: Note[];
  title: string;
  /**
   * Format version. Must be `1`.
   */
  version: number;
  /**
   * Rectangular areas that group nodes (networks, VPCs, clusters...).
   */
  zones?: Zone[];
}
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "Edge".
 */
export interface Edge {
  arrow?: "none" | "end" | "start" | "both";
  /**
   * Where the middle segment of an `orthogonal` route sits, as a fraction (0–1) of the
   * gap between the facing sides of `from` and `to` (or between their centres when they
   * overlap along the main axis). Defaults to `0.5`, halfway.
   */
  bend?: number | null;
  color?: string | null;
  /**
   * Id of a node or zone.
   */
  from: string;
  id: string;
  label?: string;
  route?: "straight" | "orthogonal";
  style?: "solid" | "dashed" | "dotted";
  /**
   * Id of a node or zone.
   */
  to: string;
}
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "Line".
 */
export interface Line {
  arrow?: "none" | "end" | "start" | "both";
  color?: string | null;
  id: string;
  /**
   * At least two `[x, y]` points.
   */
  points: [number, number][];
  style?: "solid" | "dashed" | "dotted";
}
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "Node".
 */
export interface Node {
  /**
   * Hex colour (`#rgb` or `#rrggbb`). Defaults to the colour of `kind`.
   */
  color?: string | null;
  id: string;
  kind: NodeKind;
  label?: string;
  /**
   * Arbitrary key/value details (IPs, ports, versions...).
   */
  meta?: {
    [k: string]: string;
  };
  /**
   * Centre x.
   */
  x: number;
  /**
   * Centre y.
   */
  y: number;
}
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "Note".
 */
export interface Note {
  color?: string | null;
  id: string;
  size?: "s" | "m" | "l" | "xl";
  text: string;
  x: number;
  y: number;
}
/**
 * This interface was referenced by `Diagram`'s JSON-Schema
 * via the `definition` "Zone".
 */
export interface Zone {
  /**
   * Hex colour (`#rgb` or `#rrggbb`). Defaults to the colour of `kind`.
   */
  color?: string | null;
  h: number;
  id: string;
  kind?:
    | "generic"
    | "region"
    | "vpc"
    | "subnet"
    | "dmz"
    | "availability-zone"
    | "account"
    | "security-group"
    | "k8s-cluster"
    | "namespace"
    | "on-prem"
    | "data-center";
  label?: string;
  /**
   * Border style. Defaults to the style of `kind`.
   */
  style?: StrokeStyle | null;
  w: number;
  /**
   * Left edge.
   */
  x: number;
  /**
   * Top edge.
   */
  y: number;
}
