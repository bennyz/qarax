# Qarax Local Testing Scripts

Quick reference for running qarax locally with VMs.

## Quick Start

```bash
# Start the stack only (registers host, no VM)
./hack/run-local.sh

# Start the stack and create an example VM with SSH access
./hack/run-local.sh --with-vm

# Cleanup everything
./hack/run-local.sh --cleanup
```

## Host Deploy Test (libvirt VM)

To test `POST /hosts/{host_id}/deploy` against a real VM, use:

```bash
# Start stack automatically if needed
bash ./hack/test-host-deploy-libvirt.sh --start-stack

# Keep the VM running for debugging
bash ./hack/test-host-deploy-libvirt.sh --start-stack --keep-vm

# Cleanup VM created by the script
bash ./hack/test-host-deploy-libvirt.sh --cleanup
```

This test provisions an Ubuntu cloud VM via libvirt + cloud-init, configures SSH,
adds a `bootc` stub command, exposes a qarax-node stub on port `50051`, triggers
`/hosts/{host_id}/deploy` with `reboot=true`, and waits until host status becomes `up`.

Prerequisites for this script:
- libvirt + QEMU (`virsh`, `virt-install`, `qemu-img`)
- cloud-init tooling (`cloud-localds`, usually package `cloud-image-utils`)
- local Docker stack access (the script validates connectivity from qarax container)

## Flags

- `--with-vm`: Create and start an example Alpine Linux VM with SSH. Builds a rootfs on first run (takes a few minutes).
- `--cleanup`: Stop and remove the Docker stack, volumes, and cached boot images.

## Environment Variables

- `REBUILD=1`: Force rebuild of Docker images and qarax-node binary.
- `SKIP_BUILD=1`: Skip building the qarax-node binary (use existing).

## Networking

TAP devices are created and managed automatically by qarax-node. When a VM with a network interface is created, qarax-node creates a TAP device named `qt<vm-id-prefix>n<nic-index>` (e.g. `qt24b6061en0`). The device is deleted when the VM is deleted.

You can verify with:
```bash
docker compose -f e2e/docker-compose.yml exec qarax-node ip link show type tun

VM_ID=<your-vm-id>
docker compose -f e2e/docker-compose.yml exec qarax-node \
  curl -s --unix-socket /var/lib/qarax/vms/${VM_ID}.sock \
  http://localhost/api/v1/vm.info | jq '.config.net'
```

## Common Issues

### `/dev/net/tun` not found
```bash
sudo modprobe tun
sudo mkdir -p /dev/net
sudo mknod /dev/net/tun c 10 200
sudo chmod 0666 /dev/net/tun
```
Then restart the stack: `./hack/run-local.sh --cleanup && ./hack/run-local.sh`

### VM won't start
```bash
docker compose -f e2e/docker-compose.yml logs qarax-node
curl -s http://localhost:8000/hosts | jq
```

### Services won't start
```bash
./hack/run-local.sh --cleanup
REBUILD=1 ./hack/run-local.sh
```

## Useful Commands

```bash
# View VM console output
docker compose -f e2e/docker-compose.yml exec qarax-node \
  tail -f /var/lib/qarax/vms/<vm-id>.console.log

# List VMs
curl -s http://localhost:8000/vms | jq

# Stop / delete a VM
curl -X POST http://localhost:8000/vms/<vm-id>/stop
curl -X DELETE http://localhost:8000/vms/<vm-id>

# Follow logs
docker compose -f e2e/docker-compose.yml logs -f qarax-node
docker compose -f e2e/docker-compose.yml logs -f qarax
```

## API Documentation

Once the stack is running:
- **Swagger UI**: http://localhost:8000/swagger-ui
- **OpenAPI JSON**: http://localhost:8000/api-docs/openapi.json
