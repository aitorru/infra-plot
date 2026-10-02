//! The `clean` line icons, generated from `web/src/render2d/icons-clean.ts` by `gen-icons`.
//!
//! gpui paints SVGs as one-colour masks, so each kind has a body layer (filled shapes, tinted
//! with the body colour) and a stroke layer drawn on top.

use infraplot_model::NodeKind;

pub struct Icon {
    /// Cache keys for gpui's sprite atlas.
    pub body_key: &'static str,
    pub strokes_key: &'static str,
    pub body: &'static [u8],
    pub strokes: &'static [u8],
}

macro_rules! icons {
    ($($slug:literal),* $(,)?) => {
        pub fn icon(kind: NodeKind) -> Option<Icon> {
            match kind.slug() {
                $($slug => Some(Icon {
                    body_key: concat!("icons/", $slug, ".body.svg"),
                    strokes_key: concat!("icons/", $slug, ".svg"),
                    body: include_bytes!(concat!("../assets/icons/", $slug, ".body.svg")),
                    strokes: include_bytes!(concat!("../assets/icons/", $slug, ".svg")),
                }),)*
                _ => None,
            }
        }
    };
}

icons![
    "api-gateway",
    "browser",
    "bucket",
    "cache",
    "cdn",
    "ci-cd",
    "client",
    "component",
    "container",
    "database",
    "dns",
    "external",
    "firewall",
    "function",
    "gateway",
    "gpu",
    "identity",
    "internet",
    "k8s",
    "load-balancer",
    "logging",
    "mobile",
    "monitoring",
    "notification",
    "pod",
    "proxy",
    "queue",
    "registry",
    "router",
    "scheduler",
    "search",
    "secrets",
    "server",
    "service",
    "storage",
    "stream",
    "switch",
    "user",
    "vm",
    "vpn",
    "warehouse",
    "worker",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_an_icon() {
        for &k in NodeKind::ALL {
            assert!(icon(k).is_some(), "no icon for {}", k.slug());
        }
    }
}
