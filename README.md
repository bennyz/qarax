# qarax

## Description

qarax is a management platform for virtual machines running on Cloud Hypervisor.

## Architecture

qarax consists of two main components:

- **qarax** (control plane): Axum REST API server managing VM and host lifecycle, backed by PostgreSQL
- **qarax-node** (data plane): gRPC service running on hypervisor hosts, managing VM execution via Cloud Hypervisor

Communication flow: control plane → gRPC (`proto/node.proto`) → qarax-node → Cloud Hypervisor API.

## Building

```bash
# Build all packages and generate OpenAPI spec
make build

# Run tests (auto-starts Postgres via Docker)
make test

# Lint
make lint
```

The project includes auto-generated OpenAPI 3.1 documentation. Access it at `http://localhost:8000/swagger-ui` when the server is running.

## Run locally (Docker stack)

```bash
make run-local
```

Requires Docker, Docker Compose, KVM (`/dev/kvm`), and a Rust toolchain. Stop with:

```bash
make stop-local
```

## CLI

The `qarax` CLI is the primary way to interact with the API.

```bash
# Point the CLI at your server (saved to ~/.config/qarax/config.toml)
qarax configure --server http://localhost:8000

# Or use the env var / --server flag per-command
export QARAX_SERVER=http://localhost:8000
```

Output format can be changed with `-o json` or `-o yaml` on any command.

### Available commands

| Command | Description |
|---|---|
| `qarax vm` | Virtual machine operations |
| `qarax host` | Hypervisor host operations |
| `qarax storage-pool` | Storage pool operations |
| `qarax storage-object` | Storage object operations |
| `qarax boot-source` | Boot source operations |
| `qarax network` | Network operations |
| `qarax instance-type` | Instance type operations |
| `qarax vm-template` | VM template operations |
| `qarax hook` | Lifecycle webhook operations |
| `qarax transfer` | File transfer operations |
| `qarax job` | Async job status |

## Provisioning a VM (CLI walkthrough)

### Scheduling

When creating a VM, qarax picks a host in `up` state (i.e. with a reachable qarax-node). Subsequent operations route to whichever host the VM was scheduled on. Add and initialize a host first to make scheduling work.

### Step 1 — Register a storage pool

Storage pools group directories where images live on hypervisor hosts.

**Supported pool types:** `local`, `nfs`, `overlaybd`

```bash
qarax storage-pool create \
  --name local-images \
  --pool-type local \
  --config '{"path": "/var/lib/qarax/images"}'
```

Attach the pool to a host so qarax-node can use it:

```bash
qarax storage-pool attach-host local-images vmm-host-1
```

### Step 2 — Register storage objects (kernel + initramfs)

Each object points to a file on the host. Ensure the file exists at that path on the hypervisor host.

```bash
# Kernel
qarax storage-object create \
  --name vmlinux-6.1 \
  --pool local-images \
  --object-type kernel \
  --size 0

# Initramfs (optional)
qarax storage-object create \
  --name test-initramfs \
  --pool local-images \
  --object-type initrd \
  --size 0
```

`object-type` values: `disk`, `kernel`, `initrd`, `iso`, `snapshot`, `oci_image`

### Step 3 — Create a boot source

A boot source links a kernel and optional initramfs, and sets the kernel command line.

```bash
qarax boot-source create \
  --name linux-6.1 \
  --kernel vmlinux-6.1 \
  --initrd test-initramfs \
  --params "console=ttyS0 reboot=k panic=1 nomodules"
```

`--initrd` and `--params` are optional. If you omit `--boot-source` when creating a VM, the server falls back to `vm_defaults` from the YAML config.

### Step 3.5 — Optional: instance types and VM templates

**Instance types** provide reusable sizing presets:

```bash
qarax instance-type create \
  --name gpu-small \
  --vcpus 4 \
  --max-vcpus 8 \
  --memory 1073741824
```

**VM templates** provide reusable VM blueprints. They are most useful when they define an `image_ref`, a boot source plus root disk, or are created from an existing VM:

```bash
# Template with an OCI root image
qarax vm-template create \
  --name ubuntu-ai-base \
  --image-ref docker.io/library/ubuntu:22.04

# Template with a storage-pool-backed root disk
qarax vm-template create \
  --name ubuntu-disk-base \
  --vcpus 2 \
  --memory 536870912 \
  --root-disk my-disk-object
```

Create a template from an existing VM:

```bash
qarax vm template create my-vm --name golden-ubuntu
```

When creating a VM, field precedence is:
1. Fields supplied directly in `qarax vm create`
2. The selected `--instance-type` for sizing fields
3. The selected `--template` for reusable VM defaults
4. Server-side `vm_defaults` as the final fallback

### Step 4 — Create the VM

**Minimal (no networking):**

```bash
qarax vm create \
  --name my-vm \
  --vcpus 2 \
  --memory 536870912 \
  --boot-source linux-6.1
```

`--memory` is in bytes (536870912 = 512 MiB).

**With tags:**

```bash
qarax vm create \
  --name my-vm \
  --vcpus 2 \
  --memory 536870912 \
  --boot-source linux-6.1 \
  --tag dev --tag ci
```

**Using a template + instance type:**

```bash
qarax vm create \
  --name my-ai-vm \
  --template ubuntu-ai-base \
  --instance-type gpu-small \
  --vcpus 16
```

**With a network (auto-allocated IP):**

```bash
qarax vm create \
  --name my-vm \
  --vcpus 2 \
  --memory 536870912 \
  --boot-source linux-6.1 \
  --network my-network
```

**With a static IP:**

```bash
qarax vm create \
  --name my-vm \
  --vcpus 2 \
  --memory 536870912 \
  --boot-source linux-6.1 \
  --network my-network \
  --ip 192.168.100.10
```

**With a storage-backed root disk:**

```bash
qarax vm create \
  --name ubuntu-disk-vm \
  --vcpus 2 \
  --memory 536870912 \
  --boot-source linux-6.1 \
  --root-disk my-disk-object
```

**OCI image (OverlayBD, async creation):**

```bash
qarax vm create \
  --name my-oci-vm \
  --vcpus 2 \
  --memory 536870912 \
  --image-ref public.ecr.aws/docker/library/ubuntu:22.04
```

When using `--image-ref`, creation is asynchronous. The CLI polls the job until it completes.

**With cloud-init:**

```bash
qarax vm create \
  --name my-vm \
  --vcpus 2 \
  --memory 536870912 \
  --boot-source linux-6.1 \
  --network my-network \
  --cloud-init-user-data ./user-data.yaml
```

**Root disk sources:**
- `--image-ref` uses the OCI image workflow (OverlayBD or virtiofs depending on host support)
- `--root-disk` uses an existing storage object from a storage pool

### Step 5 — Start the VM

```bash
qarax vm start my-vm
```

Start is asynchronous. The CLI polls the job and prints progress until the VM is running.

### Step 6 — Check status

```bash
qarax vm list
qarax vm get my-vm
```

VM `status` values: `unknown`, `created`, `running`, `paused`, `shutdown`

### Other lifecycle operations

```bash
qarax vm pause my-vm
qarax vm resume my-vm
qarax vm stop my-vm
qarax vm force-stop my-vm   # hard power-off
qarax vm delete my-vm
```

**Console output:**

```bash
qarax vm console my-vm      # print the stored serial console log
qarax vm attach my-vm       # attach an interactive WebSocket console
```

Serial console output is written to `/var/lib/qarax/vms/<vm-uuid>.console.log` on the qarax-node host.

### Disk and NIC management

```bash
# Attach an OverlayBD disk (hotplugs if VM is running)
qarax vm attach-disk my-vm --object my-disk-object

# Remove a disk
qarax vm remove-disk my-vm --device-id disk0

# Add a NIC (hotplugs if VM is running)
qarax vm add-nic my-vm --network my-network

# Remove a NIC
qarax vm remove-nic my-vm --device-id net1
```

### Live resize

```bash
# Resize vCPUs and/or memory on a running VM
qarax vm resize my-vm --vcpus 4
qarax vm resize my-vm --ram 1073741824
```

### Snapshots

```bash
qarax vm snapshot create my-vm --name snap-1
qarax vm snapshot list my-vm
qarax vm snapshot restore my-vm --snapshot snap-1
```

### Live migration

```bash
qarax vm migrate my-vm --host vmm-host-2
```

Live migration requires NFS-backed storage shared between both hosts.

## Host Provisioning

qarax uses bootc (bootable containers) to deploy VMM hosts. The bootc image includes qarax-node, Cloud Hypervisor, and all necessary dependencies.

### Host requirements

Before deployment, provide a pre-installed Linux host that:
- is reachable via SSH from the qarax control plane
- has virtualization support enabled (`/dev/kvm`)
- can expose qarax-node on a reachable gRPC port (default `50051`)

### Register and deploy a host

```bash
# Register the host
qarax host add \
  --name vmm-host-1 \
  --address 10.0.0.42 \
  --port 22 \
  --user root \
  --password ""

# Build and push the bootc image
make appliance-build
make appliance-push

# Deploy the bootc image over SSH
qarax host deploy vmm-host-1 \
  --image quay.io/yourorg/qarax-vmm-host:latest \
  --ssh-key /home/qarax/.ssh/id_ed25519

# After the host reboots, initialize it (connects via gRPC, marks as UP)
qarax host init vmm-host-1
```

### Inspect hosts

```bash
qarax host list
qarax host get vmm-host-1
qarax host gpus vmm-host-1   # list GPUs available for passthrough
```

## Lifecycle Hooks

Register webhooks that fire on VM status transitions:

```bash
# Global hook — fires on all VM events
qarax hook create \
  --name notify-all \
  --url https://hooks.example.com/qarax \
  --secret my-hmac-secret

# VM-scoped hook
qarax hook create \
  --name notify-my-vm \
  --url https://hooks.example.com/qarax \
  --scope vm \
  --scope-value <vm-uuid> \
  --events vm.started,vm.stopped

# Tag-scoped hook
qarax hook create \
  --name notify-prod \
  --url https://hooks.example.com/qarax \
  --scope tag \
  --scope-value prod

# Inspect hook executions
qarax hook executions notify-all
```

## VM Boot Configuration

Configure default boot artifacts in your environment's YAML file:

```yaml
vm_defaults:
  kernel: "/var/lib/qarax/images/vmlinux"
  initramfs: "/var/lib/qarax/images/initramfs.gz"
  cmdline: "console=ttyS0 console=hvc0 root=/dev/vda1"
```

YAML files live in `configuration/` (`base.yaml`, `local.yaml`, `production.yaml`), selected by the `APP_ENVIRONMENT` env var (default: `local`).
