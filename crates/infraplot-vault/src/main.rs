//! `infra-plot-vault`: manage a network vault from the command line (no display needed, so
//! it also runs from cron or a server).

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use infraplot_vault::{Cidr, NetworkConfig, Progress, ScanSummary, Vault, net, nmap, scan};

#[derive(Debug, Parser)]
#[command(name = "infra-plot-vault", version, about)]
struct Args {
    /// The vault folder (defaults to the vault containing the current directory).
    #[arg(long, short = 'C', global = true, env = "INFRAPLOT_VAULT")]
    vault: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Create a vault. Without --network, documents the networks of this machine.
    Init {
        /// Folder to create it in (default: current directory or --vault).
        dir: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
        /// `CIDR[@gateway]`, e.g. 192.168.1.0/24@192.168.1.1. Repeatable.
        #[arg(long = "network", value_parser = parse_network)]
        networks: Vec<NetworkConfig>,
    },
    /// Scan the vault's networks (or the given ones) and update inventory and diagram.
    Scan {
        /// Networks to sweep instead of the configured ones.
        targets: Vec<Cidr>,
        /// Comma-separated ports to probe.
        #[arg(long, value_delimiter = ',')]
        ports: Vec<u16>,
        /// Connection timeout in milliseconds.
        #[arg(long, default_value_t = 400)]
        timeout_ms: u64,
        /// Skip reverse DNS lookups.
        #[arg(long)]
        no_names: bool,
    },
    /// Import an nmap XML report (`nmap -oX file.xml ...`).
    Import { file: PathBuf },
    /// List the hosts in the inventory.
    Hosts {
        /// Only hosts matching this text (name, IP, MAC, vendor...).
        filter: Option<String>,
    },
    /// Add missing hosts to diagrams/network.toml and refresh their details.
    Sync,
}

fn parse_network(s: &str) -> Result<NetworkConfig, String> {
    let (cidr, gw) = match s.split_once('@') {
        Some((c, g)) => (c, Some(g.parse::<Ipv4Addr>().map_err(|e| e.to_string())?)),
        None => (s, None),
    };
    Ok(NetworkConfig {
        cidr: cidr.parse().map_err(|e: net::CidrError| e.to_string())?,
        name: String::new(),
        gateway: gw,
    })
}

fn open(vault: Option<&Path>) -> anyhow::Result<Vault> {
    let dir = match vault {
        Some(d) => d.to_path_buf(),
        None => Vault::find(Path::new(".")).context(
            "not inside a vault: pass --vault <dir> or create one with `infra-plot-vault init`",
        )?,
    };
    Ok(Vault::open(&dir)?)
}

fn report(s: &ScanSummary) {
    println!("{}", s.merge.summary());
    for id in &s.merge.added {
        println!("  + {id}");
    }
    for id in &s.merge.went_down {
        println!("  - {id} (down)");
    }
    println!(
        "network diagram: {} nodes added, {} updated · raw result in {}",
        s.sync.added_nodes,
        s.sync.updated_nodes,
        s.file.display()
    );
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Cmd::Init {
            dir,
            name,
            networks,
        } => {
            let dir = dir.or(args.vault).unwrap_or_else(|| PathBuf::from("."));
            let v = Vault::init(
                &dir,
                name.as_deref(),
                (!networks.is_empty()).then_some(networks),
            )?;
            println!("Created vault “{}” in {}", v.config.name, dir.display());
            if v.config.networks.is_empty() {
                println!("No network detected: add one to vault.toml or pass it to `scan`.");
            }
            for n in &v.config.networks {
                let gw = n.gateway.map(|g| format!(" via {g}")).unwrap_or_default();
                println!("  network {}{gw}", n.cidr);
            }
        }
        Cmd::Scan {
            targets,
            ports,
            timeout_ms,
            no_names,
        } => {
            let mut v = open(args.vault.as_deref())?;
            let mut opts = v.scan_options();
            if !targets.is_empty() {
                opts.targets = targets;
            }
            if !ports.is_empty() {
                opts.ports = ports;
            }
            opts.timeout = Duration::from_millis(timeout_ms);
            opts.resolve_names = !no_names;
            if opts.targets.is_empty() {
                bail!("no networks in vault.toml: pass one, e.g. `scan 192.168.1.0/24`");
            }
            let list: Vec<String> = opts.targets.iter().map(ToString::to_string).collect();
            eprintln!("Scanning {} ({} ports)…", list.join(", "), opts.ports.len());
            let res = scan::scan(&opts, &Progress::default())?;
            report(&v.record_scan(&res)?);
        }
        Cmd::Import { file } => {
            let mut v = open(args.vault.as_deref())?;
            let xml = std::fs::read_to_string(&file)
                .with_context(|| format!("reading {}", file.display()))?;
            let res = nmap::parse(&xml).with_context(|| file.display().to_string())?;
            report(&v.record_scan(&res)?);
        }
        Cmd::Hosts { filter } => {
            let v = open(args.vault.as_deref())?;
            let q = filter.unwrap_or_default().to_lowercase();
            for h in v.inventory.hosts.iter().filter(|h| h.matches(&q)) {
                println!(
                    "{:<4} {:<24} {:<15} {:<17} {}",
                    if h.manual {
                        "man"
                    } else if h.up {
                        "up"
                    } else {
                        "down"
                    },
                    h.id,
                    h.ip.map(|i| i.to_string()).unwrap_or_default(),
                    h.mac.as_deref().unwrap_or_default(),
                    h.port_list()
                );
            }
        }
        Cmd::Sync => {
            let v = open(args.vault.as_deref())?;
            let r = v.sync_network_diagram()?;
            println!(
                "{} nodes and {} zones added, {} nodes updated",
                r.added_nodes, r.added_zones, r.updated_nodes
            );
        }
    }
    Ok(())
}
