//! Network vaults for infra-plot: document a network as a folder of plain files.
//!
//! A [`Vault`] holds an inventory of hosts found by scanning the network ([`scan`], or an
//! nmap report through [`nmap`]), a Markdown note per host, and diagrams; the topology
//! diagram is kept in step with the inventory by [`graph::sync`].

pub mod graph;
pub mod inventory;
pub mod net;
pub mod nmap;
pub mod scan;
pub mod time;
pub mod vault;

pub use inventory::{Host, Inventory, MergeReport};
pub use net::Cidr;
pub use scan::{Observed, Port, Progress, ScanError, ScanOptions, ScanResult};
pub use vault::{NetworkConfig, ScanSummary, Vault, VaultConfig, VaultError};
