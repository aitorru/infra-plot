# nas

- Role: file server and backups (Synology DS920+)
- Owner: IT · ana@acme.example
- Location: rack 1, U12

## Shares

| Share | Who | Backed up to |
| --- | --- | --- |
| `\\nas\projects` | everyone | Backblaze B2, nightly |
| `\\nas\finance` | finance group | B2 + USB disk, weekly |

## Notes

- Hyper Backup runs at 02:00; check the report every Monday.
- NFS export `/volume1/vm` is mounted by [[pve]] for VM disks.
