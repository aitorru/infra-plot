//! A small unprivileged network scanner: TCP connect probes, the kernel's ARP cache and
//! reverse DNS.
//!
//! A host counts as up when any probe connects *or is refused* (a RST means something
//! answered), or when the kernel learnt its MAC address while probing. That finds most
//! devices on a LAN without root; for OS and version detection, import an nmap run instead
//! (see [`crate::nmap`]).

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::net::Cidr;

/// Ports probed by default: the usual suspects of an office or home network.
pub const DEFAULT_PORTS: &[u16] = &[
    21, 22, 23, 25, 53, 80, 88, 110, 135, 139, 143, 389, 443, 445, 515, 554, 631, 1883, 2049, 3000,
    3306, 3389, 5000, 5432, 5672, 5900, 6379, 8006, 8080, 8123, 8443, 9000, 9090, 9092, 9100, 9200,
    11211, 27017, 32400,
];

/// Larger scans are refused: a TCP connect scan of a /16 takes hours.
pub const MAX_HOSTS: u64 = 4096;

/// Well-known name of a TCP port.
#[must_use]
pub fn service_name(port: u16) -> Option<&'static str> {
    Some(match port {
        21 => "ftp",
        22 => "ssh",
        23 => "telnet",
        25 | 587 => "smtp",
        53 => "domain",
        80 | 8080 => "http",
        88 => "kerberos",
        110 => "pop3",
        135 => "msrpc",
        139 => "netbios-ssn",
        143 => "imap",
        389 => "ldap",
        443 | 8443 => "https",
        445 => "microsoft-ds",
        515 => "printer",
        554 => "rtsp",
        631 => "ipp",
        636 => "ldaps",
        1883 => "mqtt",
        2049 => "nfs",
        3000 => "grafana",
        3306 => "mysql",
        3389 => "rdp",
        5000 => "upnp",
        5432 => "postgresql",
        5672 => "amqp",
        5900 => "vnc",
        6379 => "redis",
        8006 => "proxmox",
        8123 => "home-assistant",
        9000 => "http-alt",
        9090 => "prometheus",
        9092 => "kafka",
        9100 => "jetdirect",
        9200 => "elasticsearch",
        11211 => "memcached",
        27017 => "mongodb",
        32400 => "plex",
        _ => return None,
    })
}

/// One open port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    pub port: u16,
    #[serde(default = "tcp", skip_serializing_if = "is_tcp")]
    pub proto: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// Software and version, when the scanner identified them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
}

fn tcp() -> String {
    "tcp".into()
}

fn is_tcp(p: &str) -> bool {
    p == "tcp"
}

impl Port {
    #[must_use]
    pub fn tcp(port: u16) -> Self {
        Self {
            port,
            proto: tcp(),
            service: service_name(port).map(str::to_owned),
            product: None,
        }
    }
}

/// A host seen by a scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observed {
    pub ip: Ipv4Addr,
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
}

impl Observed {
    #[must_use]
    pub fn new(ip: Ipv4Addr) -> Self {
        Self {
            ip,
            mac: None,
            hostnames: Vec::new(),
            vendor: None,
            os: None,
            ports: Vec::new(),
        }
    }
}

/// The outcome of a scan or an import, as stored in the vault's `scans/` folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanResult {
    /// `tcp-connect` (built-in scanner) or `nmap`.
    pub source: String,
    pub started: String,
    pub finished: String,
    /// Networks swept: hosts known in them that didn't answer are marked down. Empty for
    /// imports, which don't say what was swept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ranges: Vec<Cidr>,
    /// Whether ports were probed; otherwise the known ports of each host are kept.
    #[serde(default)]
    pub port_scan: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<Observed>,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub targets: Vec<Cidr>,
    pub ports: Vec<u16>,
    /// Per connection attempt.
    pub timeout: Duration,
    /// Parallel connection attempts.
    pub workers: usize,
    /// Look up host names (`getent hosts`, so `/etc/hosts`, DNS and mDNS when configured).
    pub resolve_names: bool,
}

impl ScanOptions {
    #[must_use]
    pub fn new(targets: Vec<Cidr>) -> Self {
        Self {
            targets,
            ports: DEFAULT_PORTS.to_vec(),
            timeout: Duration::from_millis(400),
            workers: 256,
            resolve_names: true,
        }
    }

    fn addresses(&self) -> Vec<Ipv4Addr> {
        let mut v: Vec<Ipv4Addr> = self.targets.iter().flat_map(Cidr::hosts).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Shared between the scan and whoever displays it.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    /// Hosts found so far.
    pub found: AtomicUsize,
    pub cancel: AtomicBool,
}

impl Progress {
    /// 0–1.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)] // progress bar precision
        let f = self.done.load(Ordering::Relaxed) as f64 / total as f64;
        f.min(1.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("nothing to scan: add a network such as 192.168.1.0/24")]
    NoTargets,
    #[error(
        "{0} addresses is too many for a TCP connect scan (max {MAX_HOSTS}); split it or use nmap"
    )]
    TooLarge(u64),
    #[error("scan cancelled")]
    Cancelled,
}

/// Sweeps `opts.targets`. Blocking: run it on a background thread.
pub fn scan(opts: &ScanOptions, progress: &Progress) -> Result<ScanResult, ScanError> {
    let count: u64 = opts.targets.iter().map(Cidr::host_count).sum();
    if count == 0 {
        return Err(ScanError::NoTargets);
    }
    if count > MAX_HOSTS {
        return Err(ScanError::TooLarge(count));
    }
    let started = crate::time::now();
    let addrs = opts.addresses();
    let ports: Vec<u16> = if opts.ports.is_empty() {
        vec![80]
    } else {
        opts.ports.clone()
    };
    // Port-major order spreads the probes of one host over the whole run.
    let jobs: Vec<(Ipv4Addr, u16)> = ports
        .iter()
        .flat_map(|&p| addrs.iter().map(move |&a| (a, p)))
        .collect();
    progress.total.store(jobs.len(), Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    progress.found.store(0, Ordering::Relaxed);

    // ip → open ports (an empty list: answered with a reset only).
    let alive: Mutex<BTreeMap<Ipv4Addr, Vec<u16>>> = Mutex::default();
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..opts.workers.clamp(1, jobs.len()) {
            s.spawn(|| {
                loop {
                    if progress.cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(ip, port)) = jobs.get(i) else {
                        return;
                    };
                    let addr = SocketAddr::new(IpAddr::V4(ip), port);
                    let answer = match TcpStream::connect_timeout(&addr, opts.timeout) {
                        Ok(_) => Some(true),
                        Err(e) if e.kind() == ErrorKind::ConnectionRefused => Some(false),
                        Err(_) => None,
                    };
                    if let Some(open) = answer {
                        let mut map = alive
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let entry = map.entry(ip).or_insert_with(|| {
                            progress.found.fetch_add(1, Ordering::Relaxed);
                            Vec::new()
                        });
                        if open {
                            entry.push(port);
                        }
                    }
                    progress.done.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });
    if progress.cancel.load(Ordering::Relaxed) {
        return Err(ScanError::Cancelled);
    }

    let alive = alive
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut hosts: BTreeMap<Ipv4Addr, Observed> = alive
        .into_iter()
        .map(|(ip, mut open)| {
            open.sort_unstable();
            let mut o = Observed::new(ip);
            o.ports = open.into_iter().map(Port::tcp).collect();
            (ip, o)
        })
        .collect();
    for (ip, mac) in arp_cache() {
        if opts.targets.iter().any(|c| c.contains(ip)) {
            hosts.entry(ip).or_insert_with(|| Observed::new(ip)).mac = Some(mac);
        }
    }
    if opts.resolve_names {
        let names = resolve_all(&hosts.keys().copied().collect::<Vec<_>>());
        for (ip, n) in names {
            if let Some(h) = hosts.get_mut(&ip) {
                h.hostnames = n;
            }
        }
    }
    Ok(ScanResult {
        source: "tcp-connect".into(),
        started,
        finished: crate::time::now(),
        ranges: opts.targets.clone(),
        port_scan: true,
        hosts: hosts.into_values().collect(),
    })
}

/// Complete entries of the kernel's ARP cache (Linux `/proc/net/arp`).
#[must_use]
pub fn arp_cache() -> Vec<(Ipv4Addr, String)> {
    std::fs::read_to_string("/proc/net/arp")
        .map(|s| parse_arp(&s))
        .unwrap_or_default()
}

fn parse_arp(table: &str) -> Vec<(Ipv4Addr, String)> {
    table
        .lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            let ip = c.first()?.parse().ok()?;
            let flags = u32::from_str_radix(c.get(2)?.trim_start_matches("0x"), 16).ok()?;
            let mac = normalize_mac(c.get(3)?)?;
            (flags & 0x2 != 0 && mac != "00:00:00:00:00:00").then_some((ip, mac))
        })
        .collect()
}

/// `AA-BB-CC-DD-EE-FF` / `aabb.ccdd.eeff` → `aa:bb:cc:dd:ee:ff`.
#[must_use]
pub fn normalize_mac(s: &str) -> Option<String> {
    let hex: String = s.chars().filter(char::is_ascii_hexdigit).collect();
    if hex.len() != 12
        || s.chars()
            .any(|c| !c.is_ascii_hexdigit() && !":-.".contains(c))
    {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    Some(
        hex.as_bytes()
            .chunks(2)
            .map(|p| std::str::from_utf8(p).unwrap_or("00"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

/// Reverse lookups in parallel, each capped at two seconds.
fn resolve_all(ips: &[Ipv4Addr]) -> Vec<(Ipv4Addr, Vec<String>)> {
    let out = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for chunk in ips.chunks(ips.len().div_ceil(32).max(1)) {
            let out = &out;
            s.spawn(move || {
                for &ip in chunk {
                    let names = reverse_lookup(ip);
                    if !names.is_empty() {
                        out.lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .push((ip, names));
                    }
                }
            });
        }
    });
    out.into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Names for `ip` through the system resolver (`getent hosts`).
#[must_use]
pub fn reverse_lookup(ip: Ipv4Addr) -> Vec<String> {
    let Ok(mut child) = Command::new("getent")
        .args(["hosts", &ip.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Vec::new();
            }
        }
    }
    let Ok(out) = child.wait_with_output() else {
        return Vec::new();
    };
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .skip(1)
        .map(|n| n.trim_end_matches('.').to_owned())
        .collect();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn parses_the_arp_cache() {
        let table = "IP address       HW type     Flags       HW address            Mask     Device\n\
            192.168.1.1      0x1         0x2         50:88:11:C4:49:46     *        wlan0\n\
            192.168.1.9      0x1         0x0         00:00:00:00:00:00     *        wlan0\n";
        assert_eq!(
            parse_arp(table),
            [(
                Ipv4Addr::new(192, 168, 1, 1),
                "50:88:11:c4:49:46".to_owned()
            )]
        );
        assert_eq!(
            normalize_mac("AA-BB-CC-DD-EE-0F").as_deref(),
            Some("aa:bb:cc:dd:ee:0f")
        );
        assert_eq!(normalize_mac("not a mac"), None);
    }

    #[test]
    fn finds_open_ports_on_localhost() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let open = listener.local_addr().unwrap().port();
        let mut opts = ScanOptions::new(vec!["127.0.0.1/32".parse().unwrap()]);
        opts.ports = vec![open, 1];
        opts.resolve_names = false;
        let progress = Progress::default();
        let res = scan(&opts, &progress).unwrap();
        assert!((progress.fraction() - 1.0).abs() < 1e-9);
        assert_eq!(res.hosts.len(), 1);
        assert_eq!(res.hosts[0].ip, Ipv4Addr::LOCALHOST);
        assert_eq!(
            res.hosts[0]
                .ports
                .iter()
                .map(|p| p.port)
                .collect::<Vec<_>>(),
            [open]
        );
    }

    #[test]
    fn refuses_huge_or_empty_scans() {
        let progress = Progress::default();
        let big = ScanOptions::new(vec!["10.0.0.0/16".parse().unwrap()]);
        assert!(matches!(
            scan(&big, &progress),
            Err(ScanError::TooLarge(65534))
        ));
        let none = ScanOptions::new(Vec::new());
        assert!(matches!(scan(&none, &progress), Err(ScanError::NoTargets)));
    }
}
