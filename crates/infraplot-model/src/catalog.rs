//! Display metadata for every node and zone kind: labels, default colours, palette groups.
//!
//! Mirrors `web/src/model/catalog.ts`; keep both in sync (the `add-node-kind` skill lists
//! every place a kind lives).

use crate::{NodeKind, StrokeStyle, TextSize, ZoneKind};

/// Side of the square every node is drawn in, in world units.
pub const NODE_SIZE: f64 = 72.0;
/// Snapping grid, in world units.
pub const GRID: f64 = 20.0;

/// Palette sections, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeGroup {
    Generic,
    Compute,
    Network,
    Data,
    Security,
    Ops,
    Clients,
}

impl NodeGroup {
    pub const ALL: [Self; 7] = [
        Self::Generic,
        Self::Compute,
        Self::Network,
        Self::Data,
        Self::Security,
        Self::Ops,
        Self::Clients,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Generic => "Generic",
            Self::Compute => "Compute",
            Self::Network => "Network",
            Self::Data => "Data",
            Self::Security => "Security",
            Self::Ops => "Operations",
            Self::Clients => "Clients",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeKindInfo {
    pub label: &'static str,
    /// Default colour, `#rrggbb`.
    pub color: &'static str,
    pub group: NodeGroup,
    /// Height of the 3D model in world units.
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneKindInfo {
    pub label: &'static str,
    /// Default colour, `#rrggbb`.
    pub color: &'static str,
    pub style: StrokeStyle,
}

macro_rules! node_kinds {
    ($($kind:ident => $slug:literal, $label:literal, $color:literal, $group:ident, $height:literal;)*) => {
        impl NodeKind {
            /// Every kind, in palette order.
            pub const ALL: &'static [Self] = &[$(Self::$kind),*];

            #[must_use]
            pub fn info(self) -> NodeKindInfo {
                match self {
                    $(Self::$kind => NodeKindInfo {
                        label: $label,
                        color: $color,
                        group: NodeGroup::$group,
                        height: $height,
                    },)*
                }
            }

            /// The kebab-case name used in documents (`load-balancer`).
            #[must_use]
            pub fn slug(self) -> &'static str {
                match self {
                    $(Self::$kind => $slug,)*
                }
            }
        }
    };
}

node_kinds! {
    Component => "component", "Component", "#dee2e6", Generic, 36.0;
    Service => "service", "Service", "#a5d8ff", Compute, 40.0;
    Server => "server", "Server", "#b2f2bb", Compute, 80.0;
    Vm => "vm", "VM", "#c3fae8", Compute, 56.0;
    Container => "container", "Container", "#99e9f2", Compute, 44.0;
    Pod => "pod", "Pod", "#bac8ff", Compute, 36.0;
    K8s => "k8s", "Kubernetes", "#74c0fc", Compute, 32.0;
    Function => "function", "Function", "#ffd8a8", Compute, 40.0;
    Worker => "worker", "Worker", "#d8f5a2", Compute, 44.0;
    Scheduler => "scheduler", "Scheduler", "#ffe8cc", Compute, 40.0;
    Gpu => "gpu", "GPU", "#b197fc", Compute, 36.0;
    Proxy => "proxy", "Proxy", "#d0bfff", Network, 28.0;
    LoadBalancer => "load-balancer", "Load balancer", "#eebefa", Network, 28.0;
    ApiGateway => "api-gateway", "API gateway", "#e5dbff", Network, 52.0;
    Gateway => "gateway", "Gateway", "#dbe4ff", Network, 40.0;
    Router => "router", "Router", "#c5f6fa", Network, 24.0;
    Switch => "switch", "Switch", "#d3f9d8", Network, 16.0;
    Vpn => "vpn", "VPN", "#e3fafc", Network, 40.0;
    Cdn => "cdn", "CDN", "#ffec99", Network, 64.0;
    Dns => "dns", "DNS", "#d8f5a2", Network, 24.0;
    Database => "database", "Database", "#ffd43b", Data, 64.0;
    Cache => "cache", "Cache", "#ff8787", Data, 36.0;
    Queue => "queue", "Queue", "#ffa94d", Data, 28.0;
    Stream => "stream", "Stream", "#ffc078", Data, 24.0;
    Storage => "storage", "Storage", "#ced4da", Data, 48.0;
    Bucket => "bucket", "Bucket", "#ffe066", Data, 44.0;
    Warehouse => "warehouse", "Warehouse", "#e9c46a", Data, 52.0;
    Search => "search", "Search", "#ffdeeb", Data, 40.0;
    Firewall => "firewall", "Firewall", "#ffc9c9", Security, 48.0;
    Identity => "identity", "Identity", "#fcc2d7", Security, 44.0;
    Secrets => "secrets", "Secrets", "#ffd8a8", Security, 44.0;
    Monitoring => "monitoring", "Monitoring", "#96f2d7", Ops, 56.0;
    Logging => "logging", "Logging", "#c3fae8", Ops, 40.0;
    CiCd => "ci-cd", "CI/CD", "#b2f2bb", Ops, 32.0;
    Registry => "registry", "Registry", "#a5d8ff", Ops, 44.0;
    Notification => "notification", "Notification", "#ffec99", Ops, 40.0;
    User => "user", "User", "#fcc2d7", Clients, 64.0;
    Client => "client", "Client", "#dee2e6", Clients, 48.0;
    Mobile => "mobile", "Mobile", "#e5dbff", Clients, 56.0;
    Browser => "browser", "Browser", "#d0ebff", Clients, 48.0;
    Internet => "internet", "Internet", "#e7f5ff", Clients, 44.0;
    External => "external", "External", "#f1f3f5", Clients, 40.0;
}

macro_rules! zone_kinds {
    ($($kind:ident => $slug:literal, $label:literal, $color:literal, $style:ident;)*) => {
        impl ZoneKind {
            /// Every kind, in palette order.
            pub const ALL: &'static [Self] = &[$(Self::$kind),*];

            #[must_use]
            pub fn info(self) -> ZoneKindInfo {
                match self {
                    $(Self::$kind => ZoneKindInfo {
                        label: $label,
                        color: $color,
                        style: StrokeStyle::$style,
                    },)*
                }
            }

            /// The kebab-case name used in documents (`availability-zone`).
            #[must_use]
            pub fn slug(self) -> &'static str {
                match self {
                    $(Self::$kind => $slug,)*
                }
            }
        }
    };
}

zone_kinds! {
    Generic => "generic", "Zone", "#e9ecef", Dashed;
    Region => "region", "Region", "#fff3bf", Solid;
    Vpc => "vpc", "VPC", "#d3f9d8", Solid;
    Subnet => "subnet", "Subnet", "#e7f5ff", Dashed;
    Dmz => "dmz", "DMZ", "#ffe3e3", Dashed;
    AvailabilityZone => "availability-zone", "Availability zone", "#fff9db", Dashed;
    Account => "account", "Account", "#f8f0fc", Solid;
    SecurityGroup => "security-group", "Security group", "#fff5f5", Dotted;
    K8sCluster => "k8s-cluster", "K8s cluster", "#d0ebff", Solid;
    Namespace => "namespace", "Namespace", "#edf2ff", Dotted;
    OnPrem => "on-prem", "On-prem", "#f3f0ff", Solid;
    DataCenter => "data-center", "Data center", "#f1f3f5", Solid;
}

impl TextSize {
    pub const ALL: [Self; 4] = [Self::S, Self::M, Self::L, Self::Xl];

    /// Font size in world units.
    #[must_use]
    pub fn px(self) -> f64 {
        match self {
            Self::S => 16.0,
            Self::M => 22.0,
            Self::L => 32.0,
            Self::Xl => 48.0,
        }
    }
}

/// Parses `#rgb` / `#rrggbb` into `0xrrggbb`.
#[must_use]
pub fn parse_hex(color: &str) -> Option<u32> {
    let hex = color.strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok(),
        3 => {
            let v = u32::from_str_radix(hex, 16).ok()?;
            let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
            Some((r * 0x11) << 16 | (g * 0x11) << 8 | (b * 0x11))
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)] // exact grid values
mod tests {
    use super::*;

    #[test]
    fn slugs_match_serde() {
        for &k in NodeKind::ALL {
            assert_eq!(serde_json::to_value(k).unwrap(), k.slug());
            assert!(parse_hex(k.info().color).is_some());
        }
        for &k in ZoneKind::ALL {
            assert_eq!(serde_json::to_value(k).unwrap(), k.slug());
            assert!(parse_hex(k.info().color).is_some());
        }
        assert_eq!(NodeKind::ALL.len(), 42);
    }

    #[test]
    fn hex_colours() {
        assert_eq!(parse_hex("#abc"), Some(0xaabbcc));
        assert_eq!(parse_hex("#1e1e1e"), Some(0x1e1e1e));
        assert_eq!(parse_hex("1e1e1e"), None);
        assert_eq!(parse_hex("#12345"), None);
    }
}
