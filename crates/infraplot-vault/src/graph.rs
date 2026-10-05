//! Keeps a topology diagram in step with the inventory.
//!
//! Each network becomes a `subnet` zone with its hosts on a grid inside; the gateway sits
//! above it, linked to the zone and to an `internet` node. Nodes are tied to hosts through
//! `meta.host`, so a sync only adds what is missing and refreshes the scan facts in `meta`:
//! anything the user moved, renamed, recoloured or drew by hand stays as it is.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::Ipv4Addr;

use infraplot_model::catalog::NODE_SIZE;
use infraplot_model::geometry::doc_bounds;
use infraplot_model::{Arrow, Diagram, Edge, Node, NodeKind, Route, StrokeStyle, Zone, ZoneKind};

use crate::inventory::{Host, Inventory};
use crate::net::Cidr;
use crate::vault::NetworkConfig;

/// Node meta key holding the id of the host a node documents.
pub const HOST_KEY: &str = "host";
/// Colour of nodes whose host didn't answer the last scan (unless coloured by hand).
pub const DOWN_COLOR: &str = "#adb5bd";

/// Meta keys written by the sync; other keys belong to the user.
pub const SCAN_KEYS: &[&str] = &[
    "ip",
    "mac",
    "hostname",
    "vendor",
    "os",
    "ports",
    "status",
    "last-seen",
];

/// Grid columns of a new zone grow with its hosts (about twice as wide as tall), within
/// these bounds.
const MIN_COLS: usize = 6;
const MAX_COLS: usize = 16;
const CELL_W: f64 = 140.0;
const CELL_H: f64 = 140.0;
// With 72-unit nodes these paddings put node centres and zone sides on the 20-unit grid.
const PAD_X: f64 = 44.0;
const PAD_TOP: f64 = 64.0;
const PAD_BOTTOM: f64 = 44.0;
/// Top of every new zone.
const ZONE_Y: f64 = 0.0;
/// Between zones; wide enough for a gateway and the internet node on the left of a zone,
/// where the edges run level and miss the node labels.
const ZONE_GAP: f64 = 440.0;
const GATEWAY_DX: f64 = 160.0;
const INTERNET_DX: f64 = 180.0;

/// Best guess of what a host is, from its open ports and vendor.
#[must_use]
pub fn guess_kind(h: &Host, gateway: bool) -> NodeKind {
    if gateway {
        return NodeKind::Router;
    }
    let vendor = h.vendor.as_deref().unwrap_or_default().to_lowercase();
    let any = |ports: &[u16]| ports.iter().any(|&p| h.has_port(p));
    if any(&[9100, 515, 631]) {
        NodeKind::Client // printers
    } else if any(&[53]) {
        NodeKind::Dns
    } else if any(&[3306, 5432, 1433, 1521, 27017, 9200]) {
        NodeKind::Database
    } else if any(&[6379, 11211]) {
        NodeKind::Cache
    } else if any(&[5672, 1883]) {
        NodeKind::Queue
    } else if any(&[9092]) {
        NodeKind::Stream
    } else if any(&[389, 636, 88]) {
        NodeKind::Identity
    } else if any(&[9090, 3000]) {
        NodeKind::Monitoring
    } else if any(&[445, 139, 2049]) && !any(&[3389, 135]) {
        NodeKind::Storage
    } else if any(&[8006]) {
        NodeKind::Server
    } else if any(&[3389, 5900, 135]) {
        NodeKind::Client
    } else if [
        "ubiquiti", "cisco", "mikrotik", "netgear", "tp-link", "aruba", "juniper",
    ]
    .iter()
    .any(|v| vendor.contains(v))
    {
        NodeKind::Switch
    } else if any(&[22]) {
        NodeKind::Server
    } else if any(&[80, 443, 8080, 8443]) {
        NodeKind::Service
    } else if ["apple", "samsung", "xiaomi", "google", "oneplus", "huawei"]
        .iter()
        .any(|v| vendor.contains(v))
    {
        NodeKind::Mobile
    } else {
        NodeKind::Component
    }
}

/// What a sync changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub added_nodes: usize,
    pub added_zones: usize,
    pub updated_nodes: usize,
}

/// The network a host is drawn in: the first configured one containing its address, else
/// its /24.
fn network_of(ip: Ipv4Addr, networks: &[NetworkConfig]) -> NetworkConfig {
    networks
        .iter()
        .find(|n| n.cidr.contains(ip))
        .cloned()
        .unwrap_or_else(|| NetworkConfig {
            cidr: Cidr::new(ip, 24),
            name: String::new(),
            gateway: None,
        })
}

fn zone_id(c: Cidr) -> String {
    format!("net-{}", c.slug())
}

/// The node documenting `host`, if any.
#[must_use]
pub fn node_for<'a>(doc: &'a Diagram, host: &str) -> Option<&'a Node> {
    doc.nodes
        .iter()
        .find(|n| n.meta.get(HOST_KEY).is_some_and(|h| h == host))
}

fn scan_meta(h: &Host) -> BTreeMap<&'static str, String> {
    let mut m = BTreeMap::new();
    let mut put = |k: &'static str, v: Option<String>| {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            m.insert(k, v);
        }
    };
    put("ip", h.ip.map(|i| i.to_string()));
    put("mac", h.mac.clone());
    put("hostname", h.hostnames.first().cloned());
    put("vendor", h.vendor.clone());
    put("os", h.os.clone());
    put("ports", Some(h.port_list()));
    put(
        "status",
        (!h.manual).then(|| if h.up { "up" } else { "down" }.to_owned()),
    );
    put(
        "last-seen",
        h.last_seen
            .as_deref()
            .map(|t| crate::time::date(t).to_owned()),
    );
    m
}

/// Refreshes the scan facts of a linked node. Returns whether anything changed.
fn refresh(n: &mut Node, h: &Host) -> bool {
    let before = (n.meta.clone(), n.color.clone());
    let fresh = scan_meta(h);
    for k in SCAN_KEYS {
        match fresh.get(k) {
            Some(v) => {
                n.meta.insert((*k).to_owned(), v.clone());
            }
            None => {
                n.meta.remove(*k);
            }
        }
    }
    if !h.up && !h.manual && n.color.is_none() {
        n.color = Some(DOWN_COLOR.into());
    } else if h.up && n.color.as_deref() == Some(DOWN_COLOR) {
        n.color = None;
    }
    (n.meta.clone(), n.color.clone()) != before
}

fn new_node(doc: &Diagram, h: &Host, kind: NodeKind, x: f64, y: f64) -> Node {
    let id = if doc.element_type(&h.id).is_none() {
        h.id.clone()
    } else {
        doc.unique_id(&h.id)
    };
    let mut n = Node {
        id,
        kind,
        label: h.name(),
        x,
        y,
        color: None,
        meta: BTreeMap::from([(HOST_KEY.to_owned(), h.id.clone())]),
    };
    refresh(&mut n, h);
    n
}

fn has_edge(doc: &Diagram, a: &str, b: &str) -> bool {
    doc.edges
        .iter()
        .any(|e| (e.from == a && e.to == b) || (e.from == b && e.to == a))
}

fn add_edge(doc: &mut Diagram, from: &str, to: &str) {
    if has_edge(doc, from, to) {
        return;
    }
    let id = doc.unique_id("link");
    doc.edges.push(Edge {
        id,
        from: from.to_owned(),
        to: to.to_owned(),
        label: String::new(),
        style: StrokeStyle::Solid,
        arrow: Arrow::None,
        route: Route::Straight,
        bend: None,
        color: None,
    });
}

/// Columns for a new zone holding `hosts` nodes.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // small counts
fn cols_for(hosts: usize) -> usize {
    ((hosts as f64 * 2.0).sqrt().ceil() as usize).clamp(MIN_COLS, MAX_COLS)
}

/// Columns that fit in an existing zone `w` wide.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small counts
fn cols_in(w: f64) -> usize {
    (((w - 2.0 * PAD_X - NODE_SIZE) / CELL_W).floor().max(0.0) as usize + 1).min(64)
}

/// Centre of grid cell `i` in a zone whose top-left corner is `(x, y)`.
#[allow(clippy::cast_precision_loss)] // small grid indices
fn cell(x: f64, y: f64, i: usize, cols: usize) -> [f64; 2] {
    [
        x + PAD_X + NODE_SIZE / 2.0 + (i % cols) as f64 * CELL_W,
        y + PAD_TOP + NODE_SIZE / 2.0 + (i / cols) as f64 * CELL_H,
    ]
}

#[allow(clippy::cast_precision_loss)]
fn zone_size(cells: usize, cols: usize) -> (f64, f64) {
    let rows = cells.div_ceil(cols).max(1);
    let cols = cells.clamp(1, cols);
    (
        2.0 * PAD_X + NODE_SIZE + (cols - 1) as f64 * CELL_W,
        PAD_TOP + NODE_SIZE + (rows - 1) as f64 * CELL_H + PAD_BOTTOM,
    )
}

/// Puts the gateway left of its zone (unless it is already drawn) and links it to the zone
/// and to the internet.
fn link_gateway(doc: &mut Diagram, gw: &Host, zid: &str, report: &mut SyncReport) {
    let gw_node = if let Some(n) = node_for(doc, &gw.id) {
        n.id.clone()
    } else {
        // Level with the middle of the zone, so the edge to it runs straight across.
        let (zx, mid_y) = doc
            .zones
            .iter()
            .find(|z| z.id == zid)
            .map_or((0.0, 0.0), |z| (z.x, z.y + z.h / 2.0));
        let y = (mid_y / 20.0).round() * 20.0;
        let n = new_node(doc, gw, guess_kind(gw, true), zx - GATEWAY_DX, y);
        let id = n.id.clone();
        doc.nodes.push(n);
        report.added_nodes += 1;
        id
    };
    add_edge(doc, &gw_node, zid);
    if doc.element_type("internet").is_none() {
        let (x, y) = doc
            .nodes
            .iter()
            .find(|n| n.id == gw_node)
            .map_or((0.0, 0.0), |n| (n.x - INTERNET_DX, n.y));
        doc.nodes.push(Node {
            id: "internet".into(),
            kind: NodeKind::Internet,
            label: "Internet".into(),
            x,
            y,
            color: None,
            meta: BTreeMap::new(),
        });
        report.added_nodes += 1;
    }
    if doc.element_type("internet") == Some(infraplot_model::ops::ElementType::Node) {
        add_edge(doc, "internet", &gw_node);
    }
}

/// Adds the hosts of `inv` missing from `doc` and refreshes the ones already there.
pub fn sync(doc: &mut Diagram, inv: &Inventory, networks: &[NetworkConfig]) -> SyncReport {
    let mut report = SyncReport::default();

    // Refresh every linked node, wherever it is.
    let hosts: HashMap<&str, &Host> = inv.hosts.iter().map(|h| (h.id.as_str(), h)).collect();
    for n in &mut doc.nodes {
        if let Some(h) = n.meta.get(HOST_KEY).and_then(|id| hosts.get(id.as_str()))
            && refresh(n, h)
        {
            report.updated_nodes += 1;
        }
    }
    let linked: HashSet<String> = doc
        .nodes
        .iter()
        .filter_map(|n| n.meta.get(HOST_KEY).cloned())
        .collect();

    // Hosts with an address, by network.
    let mut by_net: BTreeMap<Cidr, (NetworkConfig, Vec<&Host>)> = BTreeMap::new();
    for h in &inv.hosts {
        let Some(ip) = h.ip else { continue };
        let net = network_of(ip, networks);
        by_net
            .entry(net.cidr)
            .or_insert_with(|| (net, Vec::new()))
            .1
            .push(h);
    }

    for (cidr, (net, members)) in by_net {
        let gateway = net.gateway.and_then(|gw| inv.by_ip(gw));
        let missing: Vec<&Host> = members
            .iter()
            .copied()
            .filter(|h| {
                !linked.contains(&h.id) && Some(h.id.as_str()) != gateway.map(|g| g.id.as_str())
            })
            .collect();
        let zid = zone_id(cidr);
        let zone_exists = doc.zones.iter().any(|z| z.id == zid);
        if missing.is_empty() && zone_exists {
            continue;
        }
        if !zone_exists {
            let (w, h) = zone_size(missing.len(), cols_for(missing.len()));
            let x = doc_bounds(doc).map_or(0.0, |b| b.x + b.w + ZONE_GAP);
            let x = (x / 20.0).ceil() * 20.0;
            doc.zones.push(Zone {
                id: zid.clone(),
                kind: ZoneKind::Subnet,
                label: if net.name.is_empty() {
                    cidr.to_string()
                } else {
                    format!("{} · {cidr}", net.name)
                },
                x,
                y: ZONE_Y,
                w,
                h,
                color: None,
                style: None,
            });
            report.added_zones += 1;
        }
        let (zx, zy, cols) = doc
            .zones
            .iter()
            .find(|z| z.id == zid)
            .map_or((0.0, 0.0, MIN_COLS), |z| (z.x, z.y, cols_in(z.w)));

        // Grid cells already taken by nodes inside the zone.
        let taken: HashSet<usize> = doc
            .nodes
            .iter()
            .filter_map(|n| {
                let col = ((n.x - zx - PAD_X - NODE_SIZE / 2.0) / CELL_W).round();
                let row = ((n.y - zy - PAD_TOP - NODE_SIZE / 2.0) / CELL_H).round();
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss
                )]
                ((0.0..cols as f64).contains(&col) && row >= 0.0)
                    .then(|| row as usize * cols + col as usize)
            })
            .collect();
        let mut free = (0..).filter(|i| !taken.contains(i));
        let mut last = 0;
        for h in &missing {
            let i = free.next().unwrap_or_default();
            last = last.max(i);
            let [x, y] = cell(zx, zy, i, cols);
            let n = new_node(doc, h, guess_kind(h, false), x, y);
            doc.nodes.push(n);
            report.added_nodes += 1;
        }
        // Grow the zone to fit its grid.
        let (min_w, min_h) = zone_size(last + 1, cols);
        if let Some(z) = doc.zones.iter_mut().find(|z| z.id == zid) {
            z.w = z.w.max(min_w);
            z.h = z.h.max(min_h);
        }

        if let Some(gw) = gateway {
            link_gateway(doc, gw, &zid, &mut report);
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{Observed, Port, ScanResult};

    fn inventory() -> Inventory {
        let mut inv = Inventory::default();
        let host = |ip: [u8; 4], name: Option<&str>, ports: &[u16]| {
            let mut o = Observed::new(Ipv4Addr::from(ip));
            o.hostnames = name.map(|n| vec![n.to_owned()]).unwrap_or_default();
            o.ports = ports.iter().map(|&p| Port::tcp(p)).collect();
            o
        };
        inv.merge(&ScanResult {
            source: "test".into(),
            started: "2026-10-05T09:41:00Z".into(),
            finished: "2026-10-05T09:42:00Z".into(),
            ranges: vec!["192.168.1.0/24".parse().unwrap()],
            port_scan: true,
            hosts: vec![
                host([192, 168, 1, 1], Some("router.lan"), &[53, 80]),
                host([192, 168, 1, 10], Some("nas.lan"), &[22, 445]),
                host([192, 168, 1, 11], Some("db"), &[5432]),
                host([192, 168, 1, 50], None, &[9100]),
                host([10, 0, 0, 5], Some("vpn-box"), &[22]),
            ],
        });
        inv
    }

    fn lan() -> Vec<NetworkConfig> {
        vec![NetworkConfig {
            cidr: "192.168.1.0/24".parse().unwrap(),
            name: "Office".into(),
            gateway: Some(Ipv4Addr::new(192, 168, 1, 1)),
        }]
    }

    #[test]
    fn builds_a_topology_from_the_inventory() {
        let inv = inventory();
        let mut doc = Diagram::new("Network");
        let r = sync(&mut doc, &inv, &lan());
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
        assert_eq!(r.added_zones, 2);
        // 4 hosts in the LAN (router above the zone) + 1 in 10.0.0.0/24 + internet.
        assert_eq!(r.added_nodes, 6);
        let zone = doc
            .zones
            .iter()
            .find(|z| z.id == "net-192-168-1-0-24")
            .unwrap();
        assert_eq!(zone.label, "Office · 192.168.1.0/24");
        let router = node_for(&doc, "router").unwrap();
        assert_eq!(router.kind, NodeKind::Router);
        assert!(router.x < zone.x, "the gateway sits left of its zone");
        let internet = doc.nodes.iter().find(|n| n.id == "internet").unwrap();
        assert!((router.y - internet.y).abs() < 1e-9 && internet.x < router.x);
        let nas = node_for(&doc, "nas").unwrap();
        assert_eq!(nas.kind, NodeKind::Storage);
        assert_eq!(nas.meta["ports"], "22, 445");
        assert_eq!(nas.meta["status"], "up");
        assert!(
            infraplot_model::geometry::zone_box(zone)
                .contains(&infraplot_model::geometry::node_box(nas))
        );
        assert_eq!(node_for(&doc, "db").unwrap().kind, NodeKind::Database);
        assert_eq!(
            node_for(&doc, "host-192-168-1-50").unwrap().kind,
            NodeKind::Client
        );
        assert!(has_edge(&doc, "router", "net-192-168-1-0-24"));
        assert!(has_edge(&doc, "internet", "router"));
    }

    #[test]
    fn resync_keeps_user_edits_and_places_new_hosts_in_free_cells() {
        let mut inv = inventory();
        let mut doc = Diagram::new("Network");
        sync(&mut doc, &inv, &lan());
        let nas = doc.node_mut("nas").unwrap();
        nas.x += 1000.0; // moved out of the zone by hand
        nas.label = "Main NAS".into();
        nas.meta.insert("owner".into(), "IT".into());
        let edges = doc.edges.len();

        // Nothing new: nothing added.
        let r = sync(&mut doc, &inv, &lan());
        assert_eq!((r.added_nodes, r.added_zones, r.updated_nodes), (0, 0, 0));
        assert_eq!(doc.edges.len(), edges);

        // A new host takes the first free cell (the one the NAS left); the NAS goes down.
        let mut h = Host::new("cam");
        h.ip = Some(Ipv4Addr::new(192, 168, 1, 60));
        inv.hosts.push(h);
        inv.get_mut("nas").unwrap().up = false;
        let r = sync(&mut doc, &inv, &lan());
        assert_eq!(r.added_nodes, 1);
        assert_eq!(r.updated_nodes, 1);
        let zone = doc
            .zones
            .iter()
            .find(|z| z.id == "net-192-168-1-0-24")
            .unwrap();
        let cam = node_for(&doc, "cam").unwrap();
        let [x, y] = cell(zone.x, zone.y, 0, cols_in(zone.w));
        assert!((cam.x - x).abs() < 1e-9 && (cam.y - y).abs() < 1e-9);
        let nas = node_for(&doc, "nas").unwrap();
        assert_eq!(nas.label, "Main NAS");
        assert_eq!(nas.meta["owner"], "IT");
        assert_eq!(nas.meta["status"], "down");
        assert_eq!(nas.color.as_deref(), Some(DOWN_COLOR));
        assert!(doc.validate().is_empty());
    }

    #[test]
    fn big_networks_get_wide_zones() {
        let mut inv = Inventory::default();
        for i in 1..=140u8 {
            let mut h = Host::new(format!("h{i}"));
            h.ip = Some(Ipv4Addr::new(10, 20, 0, i));
            inv.hosts.push(h);
        }
        let mut doc = Diagram::new("n");
        sync(&mut doc, &inv, &[]);
        let z = &doc.zones[0];
        assert_eq!(cols_in(z.w), MAX_COLS);
        assert!(z.w > z.h, "{} × {}", z.w, z.h);
        for n in &doc.nodes {
            assert!(
                infraplot_model::geometry::zone_box(z)
                    .contains(&infraplot_model::geometry::node_box(n))
            );
        }
    }

    #[test]
    fn zones_grow_with_their_hosts() {
        let mut inv = Inventory::default();
        for i in 1..=14u8 {
            let mut h = Host::new(format!("h{i}"));
            h.ip = Some(Ipv4Addr::new(10, 1, 1, i));
            inv.hosts.push(h);
        }
        let mut doc = Diagram::new("n");
        sync(&mut doc, &inv, &[]);
        let z = &doc.zones[0];
        assert_eq!(z.label, "10.1.1.0/24");
        for n in &doc.nodes {
            assert!(
                infraplot_model::geometry::zone_box(z)
                    .contains(&infraplot_model::geometry::node_box(n))
            );
        }
    }
}
