# Qarax VMM Host Deployments

This directory contains everything needed to build and deploy bootc images for qarax VMM (Virtual Machine Manager) hosts.

## Overview

qarax uses **bootc** (bootable containers) to deploy VMM hosts. This provides:

- **Immutable infrastructure**: Hosts boot from container images
- **Atomic updates**: Switch between image versions with rollback capability
- **Consistency**: All hosts run identical, versioned images
- **Fast deployment**: Pull image and reboot instead of configuration management

## Files in This Directory

- `Containerfile.qarax-vmm`: Defines the bootc image for VMM hosts
- `qarax-node.service`: systemd unit for running qarax-node
- `vm-network.conf`: sysctl configuration for VM networking
- `vm-modules.conf`: Kernel modules to load for VM support
- `README.md`: This file

## Building the Image

### Prerequisites

1. **Build qarax-node binary**:
   ```bash
   cargo build --release -p qarax-node
   ```

2. **Install podman or docker**:
   ```bash
   # Fedora/RHEL/CentOS
   sudo dnf install podman

   # Ubuntu/Debian
   sudo apt install podman
   ```

### Build the Image

```bash
# From the qarax repository root:
podman build \
  -f deployments/Containerfile.qarax-vmm \
  -t quay.io/yourorg/qarax-vmm-host:v1.0.0 \
  .
```

### Push to Registry

```bash
# Login to your container registry
podman login quay.io

# Push the image
podman push quay.io/yourorg/qarax-vmm-host:v1.0.0

# Tag as latest
podman tag quay.io/yourorg/qarax-vmm-host:v1.0.0 quay.io/yourorg/qarax-vmm-host:latest
podman push quay.io/yourorg/qarax-vmm-host:latest
```

## Deploying to Hosts

### Initial Host Setup

Your target host needs to support bootc. Recommended base systems:

- Fedora CoreOS
- CentOS Stream with bootc installed

If starting from a standard system:

```bash
# On the target host:
sudo dnf install bootc

# Enable bootc
sudo bootc install to-existing-root
```

### Deploy the Image

```bash
# SSH to the target host
ssh root@vmm-host

# Switch to the qarax VMM image
bootc switch quay.io/yourorg/qarax-vmm-host:v1.0.0

# Reboot into the new image
systemctl reboot
```

After reboot, the host will:
- Boot from the container image
- Load all kernel modules for VM support
- Apply networking configuration
- Start qarax-node automatically

### Verify Deployment

```bash
# Check qarax-node is running
ssh root@vmm-host "systemctl status qarax-node"

# Check which image is active
ssh root@vmm-host "bootc status"

# Test connectivity
ssh root@vmm-host "/usr/local/bin/qarax-node --version"
```

## Development Workflow

During development, you don't need bootc images. Use **direct deployment**:

### Quick Deploy for Development

```bash
# Build debug binary
cargo build -p qarax-node

# Copy to test host
scp target/debug/qarax-node root@test-host:/usr/local/bin/qarax-node

# Restart service
ssh root@test-host "systemctl restart qarax-node"
```

### Watch Mode for Active Development

```bash
# Terminal 1: Auto-rebuild and deploy on changes
cargo watch -x 'build -p qarax-node' -s 'scp target/debug/qarax-node root@test-host:/usr/local/bin/ && ssh root@test-host systemctl restart qarax-node'

# Terminal 2: Watch logs
ssh root@test-host "journalctl -u qarax-node -f"
```

## Updating Hosts

### Update to New Version

```bash
# On the host or via qarax API:
ssh root@vmm-host "bootc switch quay.io/yourorg/qarax-vmm-host:v1.1.0 && systemctl reboot"
```

The host will:
1. Download the new image
2. Prepare the new bootloader entry
3. Reboot into the new version
4. Keep the old version available for rollback

### Rollback to Previous Version

```bash
# If something goes wrong with the new version:
ssh root@vmm-host "bootc rollback && systemctl reboot"
```

### Check Available Images

```bash
ssh root@vmm-host "bootc status"
```

Output shows:
- Current booted image
- Available images
- Rollback targets

## Customizing the Image

### Add Additional Software

Edit `Containerfile.qarax-vmm`:

```dockerfile
# Add your packages
RUN dnf install -y \
    your-package \
    another-tool
```

### Change Cloud Hypervisor Version

```bash
podman build \
  --build-arg CLOUD_HYPERVISOR_VERSION=v39.0 \
  -f deployments/Containerfile.qarax-vmm \
  -t quay.io/yourorg/qarax-vmm-host:v1.1.0 \
  .
```

### Modify qarax-node Configuration

Add a configuration file:

```dockerfile
# In Containerfile
COPY deployments/qarax-node-config.yaml /etc/qarax-node/config.yaml
```

Update the service file to use it:

```ini
# In qarax-node.service
ExecStart=/usr/local/bin/qarax-node --port 50051 --config /etc/qarax-node/config.yaml
```
