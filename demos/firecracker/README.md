# Firecracker Demo

Run a VM on Qarax using the **Firecracker** backend and exercise the full lifecycle:
create → start → pause → resume → stop → delete.

## Prerequisites

- qarax stack running (`./hack/run-local.sh`) or let the script auto-start it
- `qarax` CLI on PATH (or Rust toolchain so the script can auto-build CLI)
- Firecracker available on qarax-node (`/usr/local/bin/firecracker`)

## Usage

```bash
./demos/firecracker/run.sh

# Custom API endpoint
./demos/firecracker/run.sh --server http://localhost:8000

# Keep VM for inspection
./demos/firecracker/run.sh --no-cleanup
```

## Options

| Flag | Default | Description |
|------|---------|-------------|
| `--server URL` | `$QARAX_SERVER` or `http://localhost:8000` | qarax API URL |
| `--name NAME` | `fc-demo-<timestamp>` | VM name |
| `--vcpus N` | `1` | vCPU count |
| `--memory MiB` | `128` | Memory in MiB |
| `--no-cleanup` | off | Leave VM after demo |

## Notes

- This demo explicitly uses `--hypervisor firecracker`.
- It waits for each lifecycle state transition before continuing.
