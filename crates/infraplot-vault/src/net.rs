//! IPv4 networks (`192.168.1.0/24`) and the local machine's networks.

use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An IPv4 network in CIDR notation. A bare address is a `/32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cidr {
    /// The network address (host bits cleared).
    pub addr: Ipv4Addr,
    pub prefix: u8,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("`{0}` is not an IPv4 network like 192.168.1.0/24")]
pub struct CidrError(String);

fn mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - u32::from(prefix))
    }
}

impl Cidr {
    #[must_use]
    pub fn new(addr: Ipv4Addr, prefix: u8) -> Self {
        let prefix = prefix.min(32);
        Self {
            addr: Ipv4Addr::from(u32::from(addr) & mask(prefix)),
            prefix,
        }
    }

    #[must_use]
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        u32::from(ip) & mask(self.prefix) == u32::from(self.addr)
    }

    /// Number of addresses [`Self::hosts`] yields.
    #[must_use]
    pub fn host_count(&self) -> u64 {
        let all = 1u64 << (32 - u32::from(self.prefix));
        if self.prefix >= 31 { all } else { all - 2 }
    }

    /// Usable addresses: everything but the network and broadcast addresses (all of them for
    /// `/31` and `/32`).
    pub fn hosts(&self) -> impl Iterator<Item = Ipv4Addr> + use<> {
        let base = u64::from(u32::from(self.addr));
        let all = 1u64 << (32 - u32::from(self.prefix));
        let (first, last) = if self.prefix >= 31 {
            (base, base + all - 1)
        } else {
            (base + 1, base + all - 2)
        };
        #[allow(clippy::cast_possible_truncation)] // within the u32 range by construction
        (first..=last).map(|n| Ipv4Addr::from(n as u32))
    }

    /// A string usable inside ids and file names: `192-168-1-0-24`.
    #[must_use]
    pub fn slug(&self) -> String {
        format!(
            "{}-{}",
            self.addr.to_string().replace('.', "-"),
            self.prefix
        )
    }
}

impl fmt::Display for Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.addr, self.prefix)
    }
}

impl FromStr for Cidr {
    type Err = CidrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let err = || CidrError(s.to_owned());
        let (ip, prefix) = match s.split_once('/') {
            Some((ip, p)) => (ip, p.parse::<u8>().map_err(|_| err())?),
            None => (s, 32),
        };
        if prefix > 32 {
            return Err(err());
        }
        Ok(Self::new(ip.parse().map_err(|_| err())?, prefix))
    }
}

impl Serialize for Cidr {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Cidr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Parses a comma or whitespace separated list of networks.
pub fn parse_list(s: &str) -> Result<Vec<Cidr>, CidrError> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .collect()
}

/// A network the machine is directly attached to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalNetwork {
    pub cidr: Cidr,
    pub interface: String,
    /// The default gateway, when it lives in this network.
    pub gateway: Option<Ipv4Addr>,
}

/// Networks on the interface that holds the default route (Linux: `/proc/net/route`).
/// Empty elsewhere or when there is no default route.
#[must_use]
pub fn local_networks() -> Vec<LocalNetwork> {
    std::fs::read_to_string("/proc/net/route")
        .map(|s| parse_routes(&s))
        .unwrap_or_default()
}

fn parse_routes(table: &str) -> Vec<LocalNetwork> {
    // Columns: Iface Destination Gateway Flags RefCnt Use Metric Mask ...; addresses are
    // little-endian hex.
    let hex = |s: &str| u32::from_str_radix(s, 16).ok().map(u32::from_be);
    let rows: Vec<(&str, u32, u32, u32)> = table
        .lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            Some((
                *c.first()?,
                hex(c.get(1)?)?,
                hex(c.get(2)?)?,
                hex(c.get(7)?)?,
            ))
        })
        .collect();
    let Some(&(iface, _, gw, _)) = rows.iter().find(|r| r.1 == 0 && r.3 == 0) else {
        return Vec::new();
    };
    let gw = Ipv4Addr::from(gw);
    rows.iter()
        .filter(|r| r.0 == iface && r.1 != 0 && r.2 == 0)
        .map(|&(_, dest, _, m)| {
            let cidr = Cidr::new(
                Ipv4Addr::from(dest),
                u8::try_from(m.count_ones()).unwrap_or(32),
            );
            LocalNetwork {
                cidr,
                interface: iface.to_owned(),
                gateway: cidr.contains(gw).then_some(gw),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_iterates_networks() {
        let c: Cidr = "192.168.1.77/24".parse().unwrap();
        assert_eq!(c.to_string(), "192.168.1.0/24");
        assert_eq!(c.host_count(), 254);
        assert_eq!(c.hosts().count(), 254);
        assert_eq!(c.hosts().next(), Some(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(c.hosts().last(), Some(Ipv4Addr::new(192, 168, 1, 254)));
        assert!(c.contains(Ipv4Addr::new(192, 168, 1, 200)));
        assert!(!c.contains(Ipv4Addr::new(192, 168, 2, 1)));
        assert_eq!(c.slug(), "192-168-1-0-24");

        let one: Cidr = "10.0.0.5".parse().unwrap();
        assert_eq!(
            one.hosts().collect::<Vec<_>>(),
            [Ipv4Addr::new(10, 0, 0, 5)]
        );
        assert!("10.0.0.0/33".parse::<Cidr>().is_err());
        assert!("nope".parse::<Cidr>().is_err());
        assert_eq!(parse_list("10.0.0.0/30, 10.0.1.1").unwrap().len(), 2);
    }

    #[test]
    fn finds_the_default_route_network() {
        let table = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
            end0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
            end0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n\
            docker0\t000011AC\t00000000\t0001\t0\t0\t0\t0000FFFF\t0\t0\t0\n";
        let nets = parse_routes(table);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].cidr.to_string(), "192.168.1.0/24");
        assert_eq!(nets[0].interface, "end0");
        assert_eq!(nets[0].gateway, Some(Ipv4Addr::new(192, 168, 1, 1)));
    }
}
