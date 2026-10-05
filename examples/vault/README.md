# Acme HQ

Documentation of the Acme HQ network, kept with infra-plot.

- [Inventory](INVENTORY.md): every host found by the scans.
- `diagrams/network.toml`: the topology, updated after each scan.
- `notes/`: one page per host.

## Overview

One office, one /24. Servers run as VMs on [[pve]]; files live on [[nas]]; Windows logins go
through [[dc01]]. The edge router [[gw]] does DHCP and DNS.

## Access and credentials

Where they are kept (never the secrets themselves).

## Contacts

