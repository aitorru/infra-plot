//! The host inventory (`inventory.toml`): every device the vault knows about, merged from
//! scans and imports.

use std::collections::HashSet;
use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

use crate::scan::{Observed, Port, ScanResult};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<Host>,
}

/// A device on the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Host {
    /// Stable id: names the note (`notes/<id>.md`) and links diagram nodes (`meta.host`).
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<Ipv4Addr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hostnames: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<Port>,
    /// Added by hand rather than found by a scan: never marked down.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub manual: bool,
    /// Whether it answered the last scan that covered its address.
    #[serde(default = "yes")]
    pub up: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_seen: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
}

fn yes() -> bool {
    true
}

impl Host {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ip: None,
            mac: None,
            hostnames: Vec::new(),
            vendor: None,
            os: None,
            ports: Vec::new(),
            manual: false,
            up: true,
            first_seen: None,
            last_seen: None,
        }
    }

    /// Display name: the short host name, else the IP, else the id.
    #[must_use]
    pub fn name(&self) -> String {
        self.hostnames
            .first()
            .map(|h| short_name(h).to_owned())
            .or_else(|| self.ip.map(|ip| ip.to_string()))
            .unwrap_or_else(|| self.id.clone())
    }

    /// `22, 80, 443`.
    #[must_use]
    pub fn port_list(&self) -> String {
        self.ports
            .iter()
            .map(|p| {
                if p.proto == "tcp" {
                    p.port.to_string()
                } else {
                    format!("{}/{}", p.port, p.proto)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    #[must_use]
    pub fn has_port(&self, port: u16) -> bool {
        self.ports.iter().any(|p| p.port == port)
    }

    /// Whether `query` (lower case) appears in the id, names, addresses, vendor or OS.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let ip = self.ip.map(|i| i.to_string()).unwrap_or_default();
        [
            Some(self.id.as_str()),
            Some(ip.as_str()),
            self.mac.as_deref(),
            self.vendor.as_deref(),
            self.os.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(self.hostnames.iter().map(String::as_str))
        .any(|s| s.to_lowercase().contains(query))
    }
}

/// `nas.office.lan` → `nas`.
fn short_name(h: &str) -> &str {
    h.split('.').next().filter(|s| !s.is_empty()).unwrap_or(h)
}

/// Lower-case ASCII letters, digits and dashes.
#[must_use]
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// What a merge changed, by host id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    /// Known hosts that answered again after being down.
    pub back_up: Vec<String>,
    /// Known hosts in the swept ranges that didn't answer.
    pub went_down: Vec<String>,
}

impl MergeReport {
    /// `3 new · 1 back up · 2 down · 12 seen`.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("{} new", self.added.len())];
        if !self.back_up.is_empty() {
            parts.push(format!("{} back up", self.back_up.len()));
        }
        if !self.went_down.is_empty() {
            parts.push(format!("{} down", self.went_down.len()));
        }
        parts.push(format!("{} seen", self.added.len() + self.updated.len()));
        parts.join(" · ")
    }
}

impl Inventory {
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Host> {
        self.hosts.iter().find(|h| h.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Host> {
        self.hosts.iter_mut().find(|h| h.id == id)
    }

    #[must_use]
    pub fn by_ip(&self, ip: Ipv4Addr) -> Option<&Host> {
        self.hosts.iter().find(|h| h.ip == Some(ip))
    }

    /// An id for a new host: its short name, else `host-<ip>`; made unique with a suffix.
    #[must_use]
    pub fn new_id(&self, hostname: Option<&str>, ip: Option<Ipv4Addr>) -> String {
        let base = hostname
            .map(|h| slug(short_name(h)))
            .filter(|s| !s.is_empty() && s != "localhost")
            .or_else(|| ip.map(|ip| format!("host-{}", ip.to_string().replace('.', "-"))))
            .unwrap_or_else(|| "host".into());
        self.unique(&base)
    }

    /// `base`, or `base-2`, `base-3`... whichever is free.
    #[must_use]
    pub fn unique(&self, base: &str) -> String {
        let taken: HashSet<&str> = self.hosts.iter().map(|h| h.id.as_str()).collect();
        if !taken.contains(base) {
            return base.to_owned();
        }
        (2u32..=u32::MAX)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !taken.contains(id.as_str()))
            .unwrap_or_default()
    }

    /// The known host an observation refers to: same MAC, else same IP (unless that host
    /// has a different MAC, i.e. the address moved to another device).
    fn find(&self, o: &Observed) -> Option<usize> {
        if let Some(mac) = &o.mac
            && let Some(i) = self.hosts.iter().position(|h| h.mac.as_ref() == Some(mac))
        {
            return Some(i);
        }
        self.hosts.iter().position(|h| {
            h.ip == Some(o.ip)
                && match (&h.mac, &o.mac) {
                    (Some(a), Some(b)) => a == b,
                    _ => true,
                }
        })
    }

    /// Folds a scan or import into the inventory.
    pub fn merge(&mut self, scan: &ScanResult) -> MergeReport {
        let mut report = MergeReport::default();
        let mut seen = HashSet::new();
        for o in &scan.hosts {
            let i = if let Some(i) = self.find(o) {
                report.updated.push(self.hosts[i].id.clone());
                i
            } else {
                let mut h =
                    Host::new(self.new_id(o.hostnames.first().map(String::as_str), Some(o.ip)));
                h.first_seen = Some(scan.started.clone());
                report.added.push(h.id.clone());
                self.hosts.push(h);
                self.hosts.len() - 1
            };
            // Another host that still claims this address has left it.
            for (j, other) in self.hosts.iter_mut().enumerate() {
                if j != i && other.ip == Some(o.ip) && !other.manual {
                    other.ip = None;
                }
            }
            let h = &mut self.hosts[i];
            if !h.up {
                report.back_up.push(h.id.clone());
            }
            h.ip = Some(o.ip);
            h.up = true;
            h.last_seen = Some(scan.finished.clone());
            if o.mac.is_some() {
                h.mac.clone_from(&o.mac);
            }
            if !o.hostnames.is_empty() {
                h.hostnames.clone_from(&o.hostnames);
            }
            if o.vendor.is_some() {
                h.vendor.clone_from(&o.vendor);
            }
            if o.os.is_some() {
                h.os.clone_from(&o.os);
            }
            if scan.port_scan || !o.ports.is_empty() {
                h.ports.clone_from(&o.ports);
            }
            seen.insert(h.id.clone());
        }
        for h in &mut self.hosts {
            let swept =
                h.ip.is_some_and(|ip| scan.ranges.iter().any(|c| c.contains(ip)));
            if swept && h.up && !h.manual && !seen.contains(&h.id) {
                h.up = false;
                report.went_down.push(h.id.clone());
            }
        }
        self.hosts
            .sort_by_key(|h| (h.ip.is_none(), h.ip, h.id.clone()));
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed(ip: [u8; 4], mac: Option<&str>, name: Option<&str>, ports: &[u16]) -> Observed {
        let mut o = Observed::new(Ipv4Addr::from(ip));
        o.mac = mac.map(str::to_owned);
        o.hostnames = name.map(|n| vec![n.to_owned()]).unwrap_or_default();
        o.ports = ports.iter().map(|&p| Port::tcp(p)).collect();
        o
    }

    fn scan(at: &str, hosts: Vec<Observed>) -> ScanResult {
        ScanResult {
            source: "test".into(),
            started: at.into(),
            finished: at.into(),
            ranges: vec!["192.168.1.0/24".parse().unwrap()],
            port_scan: true,
            hosts,
        }
    }

    #[test]
    fn merges_scans_by_mac_then_ip() {
        let mut inv = Inventory::default();
        let r = inv.merge(&scan(
            "t1",
            vec![
                observed(
                    [192, 168, 1, 1],
                    Some("aa:aa:aa:aa:aa:01"),
                    Some("router.lan"),
                    &[53, 80],
                ),
                observed(
                    [192, 168, 1, 20],
                    Some("aa:aa:aa:aa:aa:02"),
                    Some("nas.lan"),
                    &[22, 445],
                ),
                observed([192, 168, 1, 30], None, None, &[9100]),
            ],
        ));
        assert_eq!(r.added, ["router", "nas", "host-192-168-1-30"]);
        assert_eq!(inv.get("nas").unwrap().name(), "nas");
        assert_eq!(inv.get("host-192-168-1-30").unwrap().name(), "192.168.1.30");

        // The NAS got a new address from DHCP; the printer is off.
        let r = inv.merge(&scan(
            "t2",
            vec![
                observed(
                    [192, 168, 1, 1],
                    Some("aa:aa:aa:aa:aa:01"),
                    None,
                    &[53, 80, 443],
                ),
                observed([192, 168, 1, 21], Some("aa:aa:aa:aa:aa:02"), None, &[22]),
            ],
        ));
        assert!(r.added.is_empty());
        assert_eq!(r.went_down, ["host-192-168-1-30"]);
        let nas = inv.get("nas").unwrap();
        assert_eq!(nas.ip, Some(Ipv4Addr::new(192, 168, 1, 21)));
        assert_eq!(
            nas.hostnames,
            ["nas.lan"],
            "names are kept when a scan has none"
        );
        assert_eq!(nas.port_list(), "22");
        assert_eq!(nas.first_seen.as_deref(), Some("t1"));
        assert_eq!(nas.last_seen.as_deref(), Some("t2"));
        assert!(!inv.get("host-192-168-1-30").unwrap().up);

        let r = inv.merge(&scan(
            "t3",
            vec![observed([192, 168, 1, 30], None, None, &[9100])],
        ));
        assert_eq!(r.back_up, ["host-192-168-1-30"]);
        assert_eq!(r.went_down, ["router", "nas"]);
        assert_eq!(r.summary(), "0 new · 1 back up · 2 down · 1 seen");
    }

    #[test]
    fn a_reused_address_with_another_mac_is_a_new_host() {
        let mut inv = Inventory::default();
        inv.merge(&scan(
            "t1",
            vec![observed(
                [192, 168, 1, 5],
                Some("aa:aa:aa:aa:aa:01"),
                Some("old"),
                &[],
            )],
        ));
        let r = inv.merge(&scan(
            "t2",
            vec![observed(
                [192, 168, 1, 5],
                Some("aa:aa:aa:aa:aa:02"),
                Some("old"),
                &[],
            )],
        ));
        assert_eq!(r.added, ["old-2"]);
        assert_eq!(
            inv.get("old").unwrap().ip,
            None,
            "the old device left the address"
        );
        assert_eq!(
            inv.get("old-2").unwrap().ip,
            Some(Ipv4Addr::new(192, 168, 1, 5))
        );
    }

    #[test]
    fn imports_keep_ports_of_ping_scans_and_dont_mark_hosts_down() {
        let mut inv = Inventory::default();
        inv.merge(&scan(
            "t1",
            vec![
                observed([192, 168, 1, 1], None, None, &[80]),
                observed([192, 168, 1, 2], None, None, &[22]),
            ],
        ));
        let mut ping = scan("t2", vec![observed([192, 168, 1, 1], None, None, &[])]);
        ping.port_scan = false;
        ping.ranges.clear();
        let r = inv.merge(&ping);
        assert!(r.went_down.is_empty());
        assert_eq!(
            inv.by_ip(Ipv4Addr::new(192, 168, 1, 1))
                .unwrap()
                .port_list(),
            "80"
        );
    }

    #[test]
    fn matches_search_queries() {
        let mut h = Host::new("nas");
        h.ip = Some(Ipv4Addr::new(10, 0, 0, 2));
        h.vendor = Some("Synology".into());
        assert!(h.matches("syno"));
        assert!(h.matches("10.0.0"));
        assert!(!h.matches("printer"));
        assert_eq!(slug("Office Printer (2F)"), "office-printer-2f");
    }
}
