# VM Commit Demo

Demonstrates the `vm commit` workflow: converting an OCI image-backed VM
(OverlayBD) into a standalone raw-disk VM.

## What it does

1. Ensures the qarax stack is running (`make run-local`)
2. Registers and initialises a qarax-node host
3. Creates an OverlayBD storage pool backed by the local test registry
4. Creates a Local storage pool for the committed raw disk
5. Attaches both pools to the host
6. Pushes `busybox:latest` to the local registry
7. Creates a VM with `--image-ref` pointing at the pushed image (async job)
8. Runs `vm commit` to byte-copy the OverlayBD block device to a raw disk
9. Verifies that `image_ref` is cleared and the committed disk object exists

## Prerequisites

- Linux host with `/dev/kvm` (KVM virtualisation)
- Docker
- Rust toolchain (`cargo`)
- `jq`, `curl`

## Usage

```bash
# Start the stack if not running, then run the demo
./demos/vm-commit/run.sh

# If the stack is already running
SKIP_STACK_START=1 ./demos/vm-commit/run.sh

# Custom server URL
QARAX_SERVER=http://localhost:8000 ./demos/vm-commit/run.sh
```

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `QARAX_SERVER` | `http://localhost:8000` | qarax API URL |
| `REGISTRY_PUSH_URL` | `localhost:5001` | Registry URL for docker push (host-side) |
| `REGISTRY_INTERNAL_URL` | `registry:5000` | Registry URL as seen inside Docker network |
| `QARAX_NODE_ADDRESS` | `qarax-node` | Node hostname (inside Docker network) |
| `QARAX_NODE_PORT` | `50051` | Node gRPC port |
