//! A vault: a folder that documents a network, made of plain files that diff well in git
//! and open in any Markdown editor (Obsidian included).
//!
//! ```text
//! acme-hq/
//!   vault.toml          name, networks to scan and their gateways
//!   inventory.toml      every known host, merged from scans (written by infra-plot)
//!   INVENTORY.md        the same as a Markdown table (regenerated)
//!   README.md           free-form overview of the network
//!   notes/<host>.md     free-form documentation of each host
//!   diagrams/network.toml   topology kept in step with the inventory
//!   diagrams/*.toml     any other diagram
//!   scans/<time>.toml   raw result of every scan and import
//! ```

use std::fmt::Write as _;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use infraplot_model::{Diagram, FileError};
use serde::{Deserialize, Serialize};

use crate::graph::{self, SyncReport};
use crate::inventory::{Host, Inventory, MergeReport, slug};
use crate::net::Cidr;
use crate::scan::{DEFAULT_PORTS, ScanOptions, ScanResult};

pub const CONFIG_FILE: &str = "vault.toml";
pub const INVENTORY_FILE: &str = "inventory.toml";
pub const INVENTORY_TABLE: &str = "INVENTORY.md";
pub const OVERVIEW_FILE: &str = "README.md";
pub const NOTES_DIR: &str = "notes";
pub const DIAGRAMS_DIR: &str = "diagrams";
pub const SCANS_DIR: &str = "scans";
pub const NETWORK_DIAGRAM: &str = "network.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Ports probed by the built-in scanner (defaults to [`DEFAULT_PORTS`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<u16>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<NetworkConfig>,
}

/// A network the vault documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkConfig {
    pub cidr: Cidr,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<Ipv4Addr>,
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{}: {source}", path.display())]
    Diagram { path: PathBuf, source: FileError },
    #[error("{} is not a vault (no {CONFIG_FILE})", .0.display())]
    NotAVault(PathBuf),
    #[error("{} already is a vault", .0.display())]
    AlreadyAVault(PathBuf),
    #[error("cannot encode TOML: {0}")]
    Encode(#[from] toml::ser::Error),
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> VaultError + '_ {
    move |source| VaultError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Writes through a temporary file, so a crash never leaves half a file behind.
fn write_atomic(path: &Path, contents: &str) -> Result<(), VaultError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
    }
    let name = path
        .file_name()
        .map_or_else(Default::default, |n| n.to_string_lossy());
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    std::fs::write(&tmp, contents).map_err(io(&tmp))?;
    std::fs::rename(&tmp, path).map_err(io(path))
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, VaultError> {
    let src = std::fs::read_to_string(path).map_err(io(path))?;
    toml::from_str(&src).map_err(|source| VaultError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// What recording a scan changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanSummary {
    pub merge: MergeReport,
    pub sync: SyncReport,
    /// Where the raw result was stored.
    pub file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Vault {
    pub root: PathBuf,
    pub config: VaultConfig,
    pub inventory: Inventory,
}

impl Vault {
    #[must_use]
    pub fn is_vault(dir: &Path) -> bool {
        dir.join(CONFIG_FILE).is_file()
    }

    /// The vault containing `path` (a file or folder inside it), if any.
    #[must_use]
    pub fn find(path: &Path) -> Option<PathBuf> {
        let path = std::fs::canonicalize(path).ok()?;
        path.ancestors()
            .find(|d| Self::is_vault(d))
            .map(Path::to_path_buf)
    }

    /// Creates a vault in `dir` (made if missing). `networks` defaults to the networks of
    /// this machine's default route.
    pub fn init(
        dir: &Path,
        name: Option<&str>,
        networks: Option<Vec<NetworkConfig>>,
    ) -> Result<Self, VaultError> {
        if Self::is_vault(dir) {
            return Err(VaultError::AlreadyAVault(dir.to_path_buf()));
        }
        std::fs::create_dir_all(dir).map_err(io(dir))?;
        let name = name.map_or_else(
            || {
                std::fs::canonicalize(dir)
                    .ok()
                    .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "Network".into())
            },
            str::to_owned,
        );
        let networks = networks.unwrap_or_else(|| {
            crate::net::local_networks()
                .into_iter()
                .map(|n| NetworkConfig {
                    cidr: n.cidr,
                    name: n.interface,
                    gateway: n.gateway,
                })
                .collect()
        });
        let vault = Self {
            root: dir.to_path_buf(),
            config: VaultConfig {
                name: name.clone(),
                description: String::new(),
                ports: None,
                networks,
            },
            inventory: Inventory::default(),
        };
        vault.save_config()?;
        vault.save_inventory()?;
        write_atomic(&vault.root.join(OVERVIEW_FILE), &overview_template(&name))?;
        for d in [NOTES_DIR, SCANS_DIR] {
            let p = vault.root.join(d);
            std::fs::create_dir_all(&p).map_err(io(&p))?;
        }
        let path = vault.network_diagram();
        let mut doc = Diagram::new(format!("{name} network"));
        doc.description = "Kept in step with the vault inventory by infra-plot.".into();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io(dir))?;
        }
        doc.save(&path)
            .map_err(|source| VaultError::Diagram { path, source })?;
        Ok(vault)
    }

    pub fn open(dir: &Path) -> Result<Self, VaultError> {
        let config_path = dir.join(CONFIG_FILE);
        if !config_path.is_file() {
            return Err(VaultError::NotAVault(dir.to_path_buf()));
        }
        let inv_path = dir.join(INVENTORY_FILE);
        Ok(Self {
            root: dir.to_path_buf(),
            config: read_toml(&config_path)?,
            inventory: if inv_path.exists() {
                read_toml(&inv_path)?
            } else {
                Inventory::default()
            },
        })
    }

    /// Re-reads the inventory, e.g. after a scan from the command line.
    pub fn reload(&mut self) -> Result<(), VaultError> {
        *self = Self::open(&self.root)?;
        Ok(())
    }

    pub fn save_config(&self) -> Result<(), VaultError> {
        write_atomic(
            &self.root.join(CONFIG_FILE),
            &toml::to_string_pretty(&self.config)?,
        )
    }

    /// Writes `inventory.toml` and its Markdown table.
    pub fn save_inventory(&self) -> Result<(), VaultError> {
        write_atomic(
            &self.root.join(INVENTORY_FILE),
            &toml::to_string_pretty(&self.inventory)?,
        )?;
        write_atomic(&self.root.join(INVENTORY_TABLE), &self.inventory_table())
    }

    #[must_use]
    pub fn network_diagram(&self) -> PathBuf {
        self.root.join(DIAGRAMS_DIR).join(NETWORK_DIAGRAM)
    }

    #[must_use]
    pub fn overview(&self) -> PathBuf {
        self.root.join(OVERVIEW_FILE)
    }

    /// Diagram files, the network diagram first and the rest by name.
    #[must_use]
    pub fn diagrams(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(self.root.join(DIAGRAMS_DIR))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().is_some_and(|e| {
                        e.eq_ignore_ascii_case("toml") || e.eq_ignore_ascii_case("json")
                    })
            })
            .collect();
        v.sort_by_key(|p| (p.file_name() != Some(NETWORK_DIAGRAM.as_ref()), p.clone()));
        v
    }

    /// Creates `diagrams/<slug>.toml` (or `-2`, `-3`...) with an empty diagram.
    pub fn new_diagram(&self, title: &str) -> Result<PathBuf, VaultError> {
        let base = Some(slug(title))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "diagram".into());
        let dir = self.root.join(DIAGRAMS_DIR);
        let path = (1u32..=u32::MAX)
            .map(|n| {
                dir.join(if n == 1 {
                    format!("{base}.toml")
                } else {
                    format!("{base}-{n}.toml")
                })
            })
            .find(|p| !p.exists())
            .unwrap_or_default();
        std::fs::create_dir_all(&dir).map_err(io(&dir))?;
        Diagram::new(title)
            .save(&path)
            .map_err(|source| VaultError::Diagram {
                path: path.clone(),
                source,
            })?;
        Ok(path)
    }

    #[must_use]
    pub fn note_path(&self, host_id: &str) -> PathBuf {
        self.root.join(NOTES_DIR).join(format!("{host_id}.md"))
    }

    /// The host's note, or an empty string when it has none yet.
    #[must_use]
    pub fn read_note(&self, host_id: &str) -> String {
        std::fs::read_to_string(self.note_path(host_id)).unwrap_or_default()
    }

    /// Saves the host's note; an empty text removes the file.
    pub fn write_note(&self, host_id: &str, text: &str) -> Result<(), VaultError> {
        let path = self.note_path(host_id);
        if text.trim().is_empty() {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(&path)(e)),
                _ => Ok(()),
            };
        }
        write_atomic(&path, text)
    }

    /// A starting point for a host's note.
    #[must_use]
    pub fn note_template(&self, host: &Host) -> String {
        format!(
            "# {}\n\n- Role: \n- Owner: \n- Location: \n\n## Notes\n\n",
            host.name()
        )
    }

    /// Options for the built-in scanner: the configured networks and ports.
    #[must_use]
    pub fn scan_options(&self) -> ScanOptions {
        let mut o = ScanOptions::new(self.config.networks.iter().map(|n| n.cidr).collect());
        o.ports = self
            .config
            .ports
            .clone()
            .unwrap_or_else(|| DEFAULT_PORTS.to_vec());
        o
    }

    /// Adds a host by hand (for devices a scan can't see) and returns its id.
    pub fn add_manual_host(
        &mut self,
        base_id: &str,
        ip: Option<Ipv4Addr>,
    ) -> Result<String, VaultError> {
        let base = Some(slug(base_id))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "host".into());
        let mut h = Host::new(self.inventory.unique(&base));
        h.ip = ip;
        h.manual = true;
        h.first_seen = Some(crate::time::now());
        let id = h.id.clone();
        self.inventory.hosts.push(h);
        self.save_inventory()?;
        Ok(id)
    }

    /// Stores a scan under `scans/`, merges it into the inventory and updates the network
    /// diagram on disk.
    pub fn record_scan(&mut self, scan: &ScanResult) -> Result<ScanSummary, VaultError> {
        let file = self.save_scan(scan)?;
        let merge = self.inventory.merge(scan);
        self.save_inventory()?;
        let sync = self.sync_network_diagram()?;
        Ok(ScanSummary { merge, sync, file })
    }

    /// Like [`Self::record_scan`], but leaves the diagram to the caller (the editor applies
    /// the sync to the open document so it can be undone).
    pub fn record_scan_only(
        &mut self,
        scan: &ScanResult,
    ) -> Result<(MergeReport, PathBuf), VaultError> {
        let file = self.save_scan(scan)?;
        let merge = self.inventory.merge(scan);
        self.save_inventory()?;
        Ok((merge, file))
    }

    fn save_scan(&self, scan: &ScanResult) -> Result<PathBuf, VaultError> {
        let dir = self.root.join(SCANS_DIR);
        let stamp = crate::time::compact(&scan.started);
        let file = (1u32..=u32::MAX)
            .map(|n| {
                dir.join(if n == 1 {
                    format!("{stamp}-{}.toml", scan.source)
                } else {
                    format!("{stamp}-{}-{n}.toml", scan.source)
                })
            })
            .find(|p| !p.exists())
            .unwrap_or_default();
        write_atomic(&file, &toml::to_string_pretty(scan)?)?;
        Ok(file)
    }

    /// Applies [`graph::sync`] to a document.
    pub fn sync_diagram(&self, doc: &mut Diagram) -> SyncReport {
        graph::sync(doc, &self.inventory, &self.config.networks)
    }

    /// Loads (or creates) `diagrams/network.toml`, syncs it and saves it if it changed.
    pub fn sync_network_diagram(&self) -> Result<SyncReport, VaultError> {
        let path = self.network_diagram();
        let err = |source| VaultError::Diagram {
            path: path.clone(),
            source,
        };
        let mut doc = if path.exists() {
            Diagram::load(&path).map_err(err)?
        } else {
            Diagram::new(format!("{} network", self.config.name))
        };
        let before = doc.clone();
        let report = self.sync_diagram(&mut doc);
        if doc != before || !path.exists() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(io(dir))?;
            }
            doc.save(&path).map_err(err)?;
        }
        Ok(report)
    }

    /// `INVENTORY.md`.
    #[must_use]
    pub fn inventory_table(&self) -> String {
        let mut s = format!(
            "# {} — inventory\n\n<!-- Generated by infra-plot from inventory.toml; edits here are overwritten. -->\n\n",
            self.config.name
        );
        if self.inventory.hosts.is_empty() {
            s.push_str("No hosts yet: run a scan.\n");
            return s;
        }
        s.push_str("| Host | IP | MAC | Vendor | OS | Ports | Status | Last seen |\n");
        s.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
        let cell = |v: Option<&str>| v.unwrap_or("").replace('|', "\\|");
        for h in &self.inventory.hosts {
            let name = if self.note_path(&h.id).exists() {
                format!("[{}]({NOTES_DIR}/{}.md)", h.name(), h.id)
            } else {
                h.name()
            };
            let ip = h.ip.map(|i| i.to_string());
            let status = if h.manual {
                "manual"
            } else if h.up {
                "up"
            } else {
                "down"
            };
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} | {status} | {} |",
                cell(Some(&name)),
                cell(ip.as_deref()),
                cell(h.mac.as_deref()),
                cell(h.vendor.as_deref()),
                cell(h.os.as_deref()),
                h.port_list(),
                h.last_seen
                    .as_deref()
                    .map(crate::time::date)
                    .unwrap_or_default(),
            );
        }
        s
    }
}

fn overview_template(name: &str) -> String {
    format!(
        "# {name}\n\nDocumentation of the {name} network, kept with infra-plot.\n\n\
         - [Inventory](INVENTORY.md): every host found by the scans.\n\
         - `diagrams/network.toml`: the topology, updated after each scan.\n\
         - `notes/`: one page per host.\n\n\
         ## Overview\n\n\n## Access and credentials\n\nWhere they are kept (never the secrets themselves).\n\n\
         ## Contacts\n\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{Observed, Port};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("infraplot-vault-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn init_scan_and_reopen() {
        let dir = tmp("init");
        let lan = NetworkConfig {
            cidr: "192.168.7.0/24".parse().unwrap(),
            name: "LAN".into(),
            gateway: Some(Ipv4Addr::new(192, 168, 7, 1)),
        };
        let mut v = Vault::init(&dir, Some("Acme HQ"), Some(vec![lan])).unwrap();
        assert!(Vault::is_vault(&dir));
        assert!(matches!(
            Vault::init(&dir, None, None),
            Err(VaultError::AlreadyAVault(_))
        ));
        assert_eq!(v.diagrams(), [v.network_diagram()]);
        assert_eq!(
            Vault::find(&v.network_diagram()),
            std::fs::canonicalize(&dir).ok()
        );

        let mut gw = Observed::new(Ipv4Addr::new(192, 168, 7, 1));
        gw.hostnames = vec!["gw.acme".into()];
        gw.ports = vec![Port::tcp(53)];
        let pc = Observed::new(Ipv4Addr::new(192, 168, 7, 30));
        let scan = ScanResult {
            source: "tcp-connect".into(),
            started: "2026-10-05T09:41:00Z".into(),
            finished: "2026-10-05T09:41:30Z".into(),
            ranges: v.scan_options().targets,
            port_scan: true,
            hosts: vec![gw, pc],
        };
        let s = v.record_scan(&scan).unwrap();
        assert_eq!(s.merge.added.len(), 2);
        assert_eq!(s.sync.added_nodes, 3, "gateway, pc and internet");
        assert!(s.file.ends_with("scans/20261005T094100Z-tcp-connect.toml"));
        assert_eq!(v.record_scan(&scan).unwrap().sync.added_nodes, 0);

        v.write_note("gw", "# Gateway\n\nISP: Example\n").unwrap();
        v.save_inventory().unwrap();
        let table = std::fs::read_to_string(dir.join(INVENTORY_TABLE)).unwrap();
        assert!(
            table.contains("| [gw](notes/gw.md) | 192.168.7.1 |"),
            "{table}"
        );

        let reopened = Vault::open(&dir).unwrap();
        assert_eq!(reopened.inventory, v.inventory);
        assert_eq!(reopened.config.name, "Acme HQ");
        assert_eq!(reopened.read_note("gw"), "# Gateway\n\nISP: Example\n");
        v.write_note("gw", "  \n").unwrap();
        assert!(!v.note_path("gw").exists());

        let doc = Diagram::load(&v.network_diagram()).unwrap();
        assert!(graph::node_for(&doc, "gw").is_some());

        let p = v.new_diagram("Rack layout").unwrap();
        assert!(p.ends_with("diagrams/rack-layout.toml"));
        assert!(
            v.new_diagram("Rack layout")
                .unwrap()
                .ends_with("rack-layout-2.toml")
        );
        assert_eq!(v.diagrams().len(), 3);
        assert_eq!(
            v.add_manual_host("Core Switch", None).unwrap(),
            "core-switch"
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn opening_a_plain_folder_fails() {
        let dir = tmp("plain");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(matches!(Vault::open(&dir), Err(VaultError::NotAVault(_))));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
