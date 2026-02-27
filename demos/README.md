# qarax Demos

Demo scripts showcasing different ways to deploy VMs with qarax.

## Prerequisites

1. Start the local stack:
   ```bash
   ./hack/run-local.sh           # OCI workflow (no VM networking)
   ./hack/run-local.sh --with-vm # boot source workflow (builds kernel + rootfs)
   ```

2. Ensure the `qarax` CLI is installed and on your PATH.

## Demos

### OCI Image (demo-oci.sh)

Run a VM from an OCI container image via OverlayBD. The image is imported into a storage pool, attached as a disk, and booted.

```bash
# Default: Alpine Linux
./demos/demo-oci.sh

# Custom image
./demos/demo-oci.sh --image docker.io/library/ubuntu:latest --name ubuntu-vm

# More resources
./demos/demo-oci.sh --vcpus 2 --memory 512
```

Requires: `./hack/run-local.sh` (creates the overlaybd storage pool).

### Boot Source (demo-boot-source.sh)

Run a VM from a kernel + initramfs (traditional direct-boot). A local storage pool is created, kernel/initramfs are transferred in, and a boot source is assembled.

```bash
# Default: uses kernel/initramfs from run-local.sh --with-vm
./demos/demo-boot-source.sh

# Custom kernel
./demos/demo-boot-source.sh --kernel /path/to/vmlinux --no-initramfs

# Custom kernel cmdline
./demos/demo-boot-source.sh --cmdline "console=ttyS0 root=/dev/vda rw"
```

Requires: `./hack/run-local.sh --with-vm` (builds kernel and initramfs).

### Hyperconverged (demo-hyperconverged.sh)

Run the qarax control plane (API + PostgreSQL) inside a Cloud Hypervisor VM on bare metal, with qarax-node managing VMs on the same host. This is the "hosted engine" pattern where the management server itself runs as a VM managed by the host agent.

```
Host (bare metal)
├── qarax-node (port 50051)
├── TAP: qarax-cp-tap0 (192.168.100.1/24)
└── Cloud Hypervisor VM: control-plane
    ├── eth0 (192.168.100.10/24)
    ├── qarax API (port 8000)
    └── PostgreSQL (local)
```

```bash
# Full build + run (requires root for TAP device)
sudo ./demos/demo-hyperconverged.sh

# Skip cargo build (use existing binaries)
sudo SKIP_BUILD=1 ./demos/demo-hyperconverged.sh

# Custom kernel path
sudo KERNEL_PATH=/path/to/vmlinux ./demos/demo-hyperconverged.sh

# Cleanup
sudo ./demos/demo-hyperconverged.sh --cleanup
```

Requires:
- Linux host with KVM (`/dev/kvm`)
- `podman` (for building the control plane image)
- `cloud-hypervisor` binary on PATH or at `/usr/local/bin/cloud-hypervisor`
- Kernel image at `/var/lib/qarax/images/vmlinux` (or set `KERNEL_PATH`)
- Root/sudo access (for TAP device creation)
- `qarax` CLI on PATH

## Cleanup

```bash
# Stop individual VMs
qarax vm stop <name>
qarax vm delete <name>

# Tear down the entire stack
./hack/run-local.sh --cleanup
```
