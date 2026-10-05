//! Imports nmap's XML output (`nmap -oX`), which adds what an unprivileged connect scan can't
//! see: MAC vendors, OS guesses and service versions.
//!
//! ```text
//! sudo nmap -sS -sV -O -oX scan.xml 192.168.1.0/24
//! ```

use std::net::Ipv4Addr;

use roxmltree::{Document, Node, ParsingOptions};

use crate::scan::{Observed, Port, ScanResult, normalize_mac, service_name};

#[derive(Debug, thiserror::Error)]
pub enum NmapError {
    #[error("not valid XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("not an nmap XML report (expected a <nmaprun> root)")]
    NotNmap,
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(name))
}

fn children<'a, 'i: 'a>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| c.has_tag_name(name))
}

fn time_attr(n: Option<Node>, attr: &str) -> Option<String> {
    n?.attribute(attr)?
        .parse::<i64>()
        .ok()
        .map(crate::time::from_unix)
}

/// Parses an nmap XML report. Only hosts that are up and have an IPv4 address are kept, and
/// only open ports.
pub fn parse(xml: &str) -> Result<ScanResult, NmapError> {
    // nmap writes `<!DOCTYPE nmaprun>`.
    let opts = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let doc = Document::parse_with_options(xml, opts)?;
    let run = doc.root_element();
    if !run.has_tag_name("nmaprun") {
        return Err(NmapError::NotNmap);
    }
    let started = time_attr(Some(run), "start").unwrap_or_else(crate::time::now);
    let finished = time_attr(
        child(run, "runstats").and_then(|r| child(r, "finished")),
        "time",
    )
    .unwrap_or_else(|| started.clone());
    let hosts = children(run, "host").filter_map(host).collect();
    Ok(ScanResult {
        source: "nmap".into(),
        started,
        finished,
        ranges: Vec::new(),
        // `-sn` (ping scan) runs have no <scaninfo>: keep the ports already known.
        port_scan: child(run, "scaninfo").is_some(),
        hosts,
    })
}

fn host(h: Node) -> Option<Observed> {
    if child(h, "status").and_then(|s| s.attribute("state")) == Some("down") {
        return None;
    }
    let mut ip = None;
    let mut mac = None;
    let mut vendor = None;
    for a in children(h, "address") {
        match a.attribute("addrtype") {
            Some("ipv4") => ip = a.attribute("addr").and_then(|s| s.parse::<Ipv4Addr>().ok()),
            Some("mac") => {
                mac = a.attribute("addr").and_then(normalize_mac);
                vendor = a.attribute("vendor").map(str::to_owned);
            }
            _ => {}
        }
    }
    let mut o = Observed::new(ip?);
    o.mac = mac;
    o.vendor = vendor;
    if let Some(names) = child(h, "hostnames") {
        for n in children(names, "hostname") {
            if let Some(name) = n.attribute("name")
                && !o.hostnames.iter().any(|x| x == name)
            {
                o.hostnames.push(name.to_owned());
            }
        }
    }
    if let Some(ports) = child(h, "ports") {
        o.ports = children(ports, "port").filter_map(port).collect();
        o.ports.sort_by_key(|p| p.port);
    }
    o.os = child(h, "os")
        .and_then(|os| child(os, "osmatch"))
        .and_then(|m| m.attribute("name"))
        .map(str::to_owned);
    Some(o)
}

fn port(p: Node) -> Option<Port> {
    if child(p, "state").and_then(|s| s.attribute("state")) != Some("open") {
        return None;
    }
    let number: u16 = p.attribute("portid")?.parse().ok()?;
    let svc = child(p, "service");
    let attr = |name| {
        svc.and_then(|s| s.attribute(name))
            .filter(|s| !s.is_empty())
    };
    let product = match (attr("product"), attr("version")) {
        (Some(p), Some(v)) => Some(format!("{p} {v}")),
        (Some(p), None) => Some(p.to_owned()),
        _ => None,
    };
    Some(Port {
        port: number,
        proto: p.attribute("protocol").unwrap_or("tcp").to_owned(),
        service: attr("name")
            .map(str::to_owned)
            .or_else(|| service_name(number).map(str::to_owned)),
        product,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE nmaprun>
<nmaprun scanner="nmap" args="nmap -sV -O -oX scan.xml 192.168.1.0/24" start="1791193260" version="7.95">
<scaninfo type="syn" protocol="tcp" numservices="1000" services="1-1000"/>
<host starttime="1791193261" endtime="1791193270"><status state="up" reason="arp-response"/>
<address addr="192.168.1.1" addrtype="ipv4"/>
<address addr="50:88:11:C4:49:46" addrtype="mac" vendor="TP-Link Technologies"/>
<hostnames><hostname name="router.lan" type="PTR"/></hostnames>
<ports><extraports state="closed" count="997"/>
<port protocol="tcp" portid="53"><state state="open" reason="syn-ack"/><service name="domain" product="dnsmasq" version="2.89" method="probed"/></port>
<port protocol="tcp" portid="80"><state state="open" reason="syn-ack"/><service name="http" product="lighttpd" method="probed"/></port>
<port protocol="tcp" portid="23"><state state="filtered" reason="no-response"/></port>
</ports>
<os><osmatch name="Linux 4.15 - 5.19" accuracy="98"/></os>
</host>
<host><status state="down" reason="no-response"/><address addr="192.168.1.2" addrtype="ipv4"/></host>
<host><status state="up" reason="echo-reply"/><address addr="192.168.1.50" addrtype="ipv4"/>
<hostnames/>
<ports><port protocol="tcp" portid="9100"><state state="open"/></port></ports>
</host>
<runstats><finished time="1791193320" elapsed="60"/><hosts up="2" down="1" total="3"/></runstats>
</nmaprun>
"#;

    #[test]
    fn parses_hosts_ports_and_os() {
        let r = parse(REPORT).unwrap();
        assert_eq!(r.source, "nmap");
        assert_eq!(r.started, "2026-10-05T09:41:00Z");
        assert_eq!(r.finished, "2026-10-05T09:42:00Z");
        assert!(r.port_scan);
        assert_eq!(r.hosts.len(), 2);
        let gw = &r.hosts[0];
        assert_eq!(gw.ip, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(gw.mac.as_deref(), Some("50:88:11:c4:49:46"));
        assert_eq!(gw.vendor.as_deref(), Some("TP-Link Technologies"));
        assert_eq!(gw.hostnames, ["router.lan"]);
        assert_eq!(gw.os.as_deref(), Some("Linux 4.15 - 5.19"));
        assert_eq!(gw.ports.len(), 2);
        assert_eq!(gw.ports[0].product.as_deref(), Some("dnsmasq 2.89"));
        assert_eq!(gw.ports[1].product.as_deref(), Some("lighttpd"));
        let printer = &r.hosts[1];
        assert_eq!(printer.ports[0].service.as_deref(), Some("jetdirect"));
    }

    #[test]
    fn rejects_other_xml() {
        assert!(matches!(parse("<svg/>"), Err(NmapError::NotNmap)));
        assert!(matches!(parse("<nmaprun"), Err(NmapError::Xml(_))));
    }
}
