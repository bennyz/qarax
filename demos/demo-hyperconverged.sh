#!/usr/bin/env bash
#
# Hyperconverged qarax demo — "hosted engine" pattern
#
# Runs the qarax control plane AND qarax-node inside a single Cloud Hypervisor
# VM on bare metal. The host only provides a bootstrap qarax-node to launch the
# CP VM; once the VM is up, its internal qarax-node (with Cloud Hypervisor,
# overlaybd, virtiofsd, etc.) handles all workload VMs via nested KVM.
#
# Network topology:
#   Host (bare metal)
#   ├── TAP: qarax-cp-tap0 (192.168.100.1/24)
#   ├── Local OCI registry (port 5000)
#   └── Cloud Hypervisor VM: control-plane
#       ├── eth0 (192.168.100.10/24, gw 192.168.100.1)
#       ├── qarax API (port 8000)
#       ├── qarax-node (port 50051)
#       ├── overlaybd-tcmu
#       └── PostgreSQL (port 5432, local only)
#
# Prerequisites:
#   - Linux host with KVM + nested KVM (kvm_intel.nested=Y)
#   - Rust toolchain (cargo)
#   - podman
#   - cloud-hypervisor binary on PATH (auto-downloaded if missing)
#   - Root/sudo (for TAP device creation and IP configuration)
#   - qarax CLI on PATH
#
# Usage:
#   sudo ./demos/demo-hyperconverged.sh             # Full build + run
#   sudo SKIP_BUILD=1 ./demos/demo-hyperconverged.sh # Skip cargo build
#   sudo ./demos/demo-hyperconverged.sh --cleanup    # Tear down everything

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# Ensure cargo/rustup are on PATH when running under sudo
if [[ -n "${SUDO_USER:-}" ]]; then
	SUDO_HOME=$(eval echo "~${SUDO_USER}")
	export PATH="${SUDO_HOME}/.cargo/bin:${PATH}"
	export RUSTUP_HOME="${RUSTUP_HOME:-${SUDO_HOME}/.rustup}"
	export CARGO_HOME="${CARGO_HOME:-${SUDO_HOME}/.cargo}"
fi

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Configuration
CP_VM_CPUS=4
CP_VM_MEMORY=$((4 * 1024 * 1024 * 1024))  # 4 GiB in bytes
CP_VM_IP="192.168.100.10"
HOST_TAP_IP="192.168.100.1"
TAP_NAME="qarax-cp-tap0"
CP_MAC="52:54:00:aa:bb:01"
CP_API_SOCKET="/tmp/qarax-cp.sock"
CP_CONSOLE_LOG="/tmp/qarax-cp-console.log"
CP_ROOTFS="/tmp/qarax-cp-rootfs.img"
CP_ROOTFS_SIZE_MB=4096
CLOUD_HYPERVISOR_VERSION="${CLOUD_HYPERVISOR_VERSION:-v51.0}"
CH_FIRMWARE_VERSION="${CH_FIRMWARE_VERSION:-0.4.2}"
CH_BINARY="${CH_BINARY:-$(command -v cloud-hypervisor 2>/dev/null || echo /usr/local/bin/cloud-hypervisor)}"
CH_FIRMWARE="${REPO_ROOT}/.cache/hypervisor-fw-${CH_FIRMWARE_VERSION}"
QARAX_NODE_PORT=50051
REGISTRY_PORT=5000
REGISTRY_CONTAINER_NAME="qarax-demo-registry"

# ── Cleanup mode ───────────────────────────────────────────────────────────

cleanup() {
	echo -e "${YELLOW}Cleaning up hyperconverged demo...${NC}"

	# Stop control plane VM
	if [[ -S "$CP_API_SOCKET" ]]; then
		echo "Stopping control plane VM..."
		"$CH_BINARY" --api-socket "$CP_API_SOCKET" api vm.shutdown 2>/dev/null || true
		sleep 1
		"$CH_BINARY" --api-socket "$CP_API_SOCKET" api vmm.shutdown 2>/dev/null || true
		sleep 1
	fi

	# Kill any lingering cloud-hypervisor for the control plane
	if [[ -f /tmp/qarax-cp-ch.pid ]]; then
		kill "$(cat /tmp/qarax-cp-ch.pid)" 2>/dev/null || true
		rm -f /tmp/qarax-cp-ch.pid
	fi

	# Stop local registry container
	if podman container exists "$REGISTRY_CONTAINER_NAME" 2>/dev/null; then
		echo "Stopping local registry..."
		podman rm -f "$REGISTRY_CONTAINER_NAME" 2>/dev/null || true
	fi

	# Remove NAT masquerade rule (interface-agnostic)
	iptables -t nat -D POSTROUTING -s 192.168.100.0/24 -j MASQUERADE 2>/dev/null || true

	# Remove TAP device
	if ip link show "$TAP_NAME" &>/dev/null; then
		echo "Removing TAP device ${TAP_NAME}..."
		ip link delete "$TAP_NAME" 2>/dev/null || true
	fi

	# Clean up temp files
	rm -f "$CP_API_SOCKET" "$CP_CONSOLE_LOG" "$CP_ROOTFS"

	echo -e "${GREEN}Cleanup complete.${NC}"
}

if [[ "${1:-}" == "--cleanup" ]]; then
	cleanup
	exit 0
fi

# ── Preflight checks ──────────────────────────────────────────────────────

echo "=== qarax Hyperconverged Demo ==="
echo ""

if [[ $EUID -ne 0 ]]; then
	echo -e "${RED}This script must be run as root (needs loop devices, TAP setup, etc.)${NC}"
	echo "  sudo $0 $*"
	exit 1
fi

if [[ ! -e /dev/kvm ]]; then
	echo -e "${RED}/dev/kvm not found. KVM is required.${NC}"
	exit 1
fi

if ! command -v podman &>/dev/null; then
	echo -e "${RED}podman is required. Install it and try again.${NC}"
	exit 1
fi

for tool in sgdisk mkfs.fat partprobe; do
	if ! command -v "$tool" &>/dev/null; then
		echo -e "${RED}${tool} is required. Install gdisk, dosfstools, and parted.${NC}"
		exit 1
	fi
done

if [[ ! -x "$CH_BINARY" ]]; then
	echo -e "${YELLOW}cloud-hypervisor not found, downloading ${CLOUD_HYPERVISOR_VERSION}...${NC}"
	CH_BINARY="${REPO_ROOT}/.cache/cloud-hypervisor"
	mkdir -p "$(dirname "$CH_BINARY")"
	curl -fSL \
		"https://github.com/cloud-hypervisor/cloud-hypervisor/releases/download/${CLOUD_HYPERVISOR_VERSION}/cloud-hypervisor-static" \
		-o "$CH_BINARY"
	chmod +x "$CH_BINARY"
	echo -e "${GREEN}Downloaded cloud-hypervisor ${CLOUD_HYPERVISOR_VERSION} to ${CH_BINARY}${NC}"
fi

if [[ ! -f "$CH_FIRMWARE" ]]; then
	echo -e "${YELLOW}Downloading hypervisor-fw v${CH_FIRMWARE_VERSION}...${NC}"
	mkdir -p "$(dirname "$CH_FIRMWARE")"
	curl -fSL \
		"https://github.com/cloud-hypervisor/rust-hypervisor-firmware/releases/download/${CH_FIRMWARE_VERSION}/hypervisor-fw" \
		-o "$CH_FIRMWARE"
	echo -e "${GREEN}Downloaded hypervisor-fw v${CH_FIRMWARE_VERSION} to ${CH_FIRMWARE}${NC}"
fi

# ── Phase 1: Build ─────────────────────────────────────────────────────────

echo -e "${YELLOW}Phase 1: Build${NC}"

MUSL_TARGET="x86_64-unknown-linux-musl"

if [[ -z "${SKIP_BUILD:-}" ]]; then
	if ! rustup target list --installed 2>/dev/null | grep -q "$MUSL_TARGET"; then
		echo "Adding Rust target ${MUSL_TARGET}..."
		rustup target add "$MUSL_TARGET"
	fi
	echo "Building qarax, qarax-node, and qarax-init..."
	cargo build --release -p qarax -p qarax-node -p qarax-init
else
	echo "Skipping build (SKIP_BUILD=1)"
fi

# Verify binaries exist
for bin in qarax qarax-node qarax-init; do
	if [[ ! -f "${REPO_ROOT}/target/${MUSL_TARGET}/release/${bin}" ]]; then
		echo -e "${RED}Binary not found: target/${MUSL_TARGET}/release/${bin}${NC}"
		echo "Run without SKIP_BUILD to build first."
		exit 1
	fi
done

echo "Building control plane OCI image..."
podman build -f demos/Containerfile.control-plane -t qarax-control-plane .

echo "Exporting rootfs tarball..."
CONTAINER_ID=$(podman create qarax-control-plane /bin/true | tail -1)
ROOTFS_TAR="/tmp/qarax-cp-rootfs.tar"
rm -f "$ROOTFS_TAR" "$CP_ROOTFS"
podman export "$CONTAINER_ID" -o "$ROOTFS_TAR"
podman rm "$CONTAINER_ID"

# Build a GPT disk image with ESP (FAT32) + root (ext4).
# hypervisor-fw boots via Boot Loader Specification entries on the ESP.
echo "Building GPT disk image (ESP + root)..."
ESP_SIZE_MB=256
dd if=/dev/zero of="$CP_ROOTFS" bs=1M count="$CP_ROOTFS_SIZE_MB"

# Create GPT: partition 1 = ESP (256 MB), partition 2 = root (rest)
sgdisk -Z "$CP_ROOTFS"
sgdisk -n 1:2048:+${ESP_SIZE_MB}M -t 1:ef00 -c 1:"EFI System" "$CP_ROOTFS"
sgdisk -n 2:0:0 -t 2:8300 -c 2:"Linux root" "$CP_ROOTFS"
sgdisk -p "$CP_ROOTFS"

LOOP_DEV=$(losetup --find --show -P "$CP_ROOTFS")
# -P creates partition devices: ${LOOP_DEV}p1, ${LOOP_DEV}p2
ESP_DEV="${LOOP_DEV}p1"
ROOT_DEV="${LOOP_DEV}p2"

# Wait for partition devices to appear
timeout=5; elapsed=0
while [[ ! -b "$ROOT_DEV" && $elapsed -lt $timeout ]]; do
	sleep 0.5; elapsed=$((elapsed + 1))
done
partprobe "$LOOP_DEV" 2>/dev/null || true

if [[ ! -b "$ESP_DEV" || ! -b "$ROOT_DEV" ]]; then
	echo -e "${RED}Partition devices not found: ${ESP_DEV} / ${ROOT_DEV}${NC}"
	losetup -d "$LOOP_DEV"
	exit 1
fi

mkfs.fat -F 32 "$ESP_DEV"
mkfs.ext4 -F "$ROOT_DEV"

# Mount root and populate from tarball
ROOTFS_MOUNT=$(mktemp -d)
mount "$ROOT_DEV" "$ROOTFS_MOUNT"
tar xf "$ROOTFS_TAR" -C "$ROOTFS_MOUNT"

# Find the kernel version and save boot files before mounting ESP over /boot
KVER=$(ls "${ROOTFS_MOUNT}/lib/modules/" | sort -V | tail -1)
echo "Kernel version in rootfs: ${KVER}"

# Find vmlinuz - in Fedora containers, kernel-core installs it to
# /lib/modules/<version>/vmlinuz (kernel-install doesn't run without systemd).
# Check both /boot/ and /lib/modules/ locations.
VMLINUZ=""
if [[ -f "${ROOTFS_MOUNT}/boot/vmlinuz-${KVER}" ]]; then
	VMLINUZ="${ROOTFS_MOUNT}/boot/vmlinuz-${KVER}"
elif [[ -f "${ROOTFS_MOUNT}/lib/modules/${KVER}/vmlinuz" ]]; then
	VMLINUZ="${ROOTFS_MOUNT}/lib/modules/${KVER}/vmlinuz"
fi

if [[ -z "$VMLINUZ" ]]; then
	echo -e "${RED}Could not find vmlinuz for kernel ${KVER}${NC}"
	echo "Contents of /boot/:" && ls -la "${ROOTFS_MOUNT}/boot/" 2>/dev/null || true
	echo "Contents of /lib/modules/${KVER}/:" && ls -la "${ROOTFS_MOUNT}/lib/modules/${KVER}/" 2>/dev/null || true
	umount "$ROOTFS_MOUNT"
	losetup -d "$LOOP_DEV"
	exit 1
fi
echo "Found vmlinuz at: ${VMLINUZ}"

# Mount ESP at /boot and copy kernel + BLS entry
mount "$ESP_DEV" "${ROOTFS_MOUNT}/boot"
cp "$VMLINUZ" "${ROOTFS_MOUNT}/boot/vmlinuz-${KVER}"
echo "Copied vmlinuz-${KVER} to ESP"

# Create BLS loader config and entry for hypervisor-fw
mkdir -p "${ROOTFS_MOUNT}/boot/loader/entries"

cat > "${ROOTFS_MOUNT}/boot/loader/loader.conf" << EOF
default qarax
EOF

cat > "${ROOTFS_MOUNT}/boot/loader/entries/qarax.conf" << EOF
title   qarax Control Plane
linux   /vmlinuz-${KVER}
options root=/dev/vda2 rw console=ttyS0 systemd.unified_cgroup_hierarchy=1
EOF

echo "loader.conf:"
cat "${ROOTFS_MOUNT}/boot/loader/loader.conf"
echo "BLS entry:"
cat "${ROOTFS_MOUNT}/boot/loader/entries/qarax.conf"
echo "ESP contents:"
find "${ROOTFS_MOUNT}/boot" -type f

umount "${ROOTFS_MOUNT}/boot"
umount "$ROOTFS_MOUNT"
losetup -d "$LOOP_DEV"
rmdir "$ROOTFS_MOUNT"
rm -f "$ROOTFS_TAR"

# Build the CLI last so the `qarax` binary in target/ is the CLI, not the server.
echo "Building qarax CLI..."
cargo build --release -p cli
QARAX_CLI="${REPO_ROOT}/target/${MUSL_TARGET}/release/qarax"

echo -e "${GREEN}Phase 1 complete. Rootfs: ${CP_ROOTFS} ($(du -h "$CP_ROOTFS" | cut -f1))${NC}"
echo ""

# ── Phase 2: Start local OCI registry ────────────────────────────────────

echo -e "${YELLOW}Phase 2: Start local OCI registry${NC}"

if podman container exists "$REGISTRY_CONTAINER_NAME" 2>/dev/null; then
	echo "Registry container already exists, reusing"
else
	echo "Starting local registry on port ${REGISTRY_PORT}..."
	podman run -d --name "$REGISTRY_CONTAINER_NAME" \
		-p "${REGISTRY_PORT}:5000" \
		registry:2
fi

# Wait for registry to be ready
echo -n "Waiting for registry"
timeout=15
elapsed=0
while [[ $elapsed -lt $timeout ]]; do
	if curl -sf "http://localhost:${REGISTRY_PORT}/v2/" -o /dev/null 2>/dev/null; then
		echo ""
		echo -e "${GREEN}Local registry is ready on port ${REGISTRY_PORT}${NC}"
		break
	fi
	echo -n "."
	sleep 1
	elapsed=$((elapsed + 1))
done

if [[ $elapsed -ge $timeout ]]; then
	echo ""
	echo -e "${RED}Timeout waiting for local registry${NC}"
	exit 1
fi

echo ""

# ── Phase 3: Launch control plane VM ──────────────────────────────────────

echo -e "${YELLOW}Phase 3: Launch control plane VM${NC}"

# Create TAP device
if ip link show "$TAP_NAME" &>/dev/null; then
	echo "TAP device ${TAP_NAME} already exists, reusing"
else
	echo "Creating TAP device ${TAP_NAME}..."
	ip tuntap add dev "$TAP_NAME" mode tap
fi
ip link set "$TAP_NAME" up
ip addr add "${HOST_TAP_IP}/24" dev "$TAP_NAME" 2>/dev/null || true

# Enable IP forwarding and NAT masquerade (needed for VM internet access)
# Use interface-agnostic rule so it survives switching between wifi/ethernet.
sysctl -q net.ipv4.ip_forward=1
if ! iptables -t nat -C POSTROUTING -s 192.168.100.0/24 -j MASQUERADE 2>/dev/null; then
	echo "Setting up NAT masquerade for 192.168.100.0/24..."
	iptables -t nat -A POSTROUTING -s 192.168.100.0/24 -j MASQUERADE
fi

echo "Launching Cloud Hypervisor VM..."
"$CH_BINARY" \
	--api-socket "$CP_API_SOCKET" \
	--cpus boot=${CP_VM_CPUS} \
	--memory size=${CP_VM_MEMORY} \
	--firmware "$CH_FIRMWARE" \
	--disk path="$CP_ROOTFS" \
	--net tap=${TAP_NAME},mac=${CP_MAC} \
	--serial file="$CP_CONSOLE_LOG" \
	--console off &
CH_PID=$!
echo "$CH_PID" > /tmp/qarax-cp-ch.pid

echo "Cloud Hypervisor PID: ${CH_PID}"
echo "Console log: ${CP_CONSOLE_LOG}"
echo ""

# Wait for API to become healthy
echo -n "Waiting for qarax API at http://${CP_VM_IP}:8000/"
timeout=120
elapsed=0
while [[ $elapsed -lt $timeout ]]; do
	if curl -sf "http://${CP_VM_IP}:8000/" -o /dev/null 2>/dev/null; then
		echo ""
		echo -e "${GREEN}qarax API is ready!${NC}"
		break
	fi
	echo -n "."
	sleep 2
	elapsed=$((elapsed + 2))

	# Check if CH process died
	if ! kill -0 "$CH_PID" 2>/dev/null; then
		echo ""
		echo -e "${RED}Cloud Hypervisor process died. Check console log:${NC}"
		tail -20 "$CP_CONSOLE_LOG" 2>/dev/null || true
		exit 1
	fi

	if [[ $((elapsed % 20)) -eq 0 ]]; then
		echo -e " (${elapsed}s / ${timeout}s)"
		echo -n "  "
	fi
done

if [[ $elapsed -ge $timeout ]]; then
	echo ""
	echo -e "${RED}Timeout waiting for qarax API. Console log tail:${NC}"
	tail -30 "$CP_CONSOLE_LOG" 2>/dev/null || true
	exit 1
fi

echo ""

echo -e "${YELLOW}Phase 4: Register host${NC}"

QARAX_API="http://${CP_VM_IP}:8000"

echo "Adding CP VM as compute host (self-registration)..."
HOST_ID=$(curl -sf -X POST "${QARAX_API}/hosts" \
	-H "Content-Type: application/json" \
	-d '{
		"name": "local-node",
		"address": "'"${CP_VM_IP}"'",
		"port": '"${QARAX_NODE_PORT}"',
		"host_user": "root",
		"password": ""
	}' | tr -d '"')

if [[ -z "$HOST_ID" ]]; then
	echo -e "${RED}Failed to add host${NC}"
	exit 1
fi
echo -e "Host added: ${HOST_ID}"

echo "Initializing host (gRPC handshake)..."
curl -sf -X POST "${QARAX_API}/hosts/${HOST_ID}/init" | head -c 200
echo ""

echo ""

# ── Phase 5: Create workload VM ───────────────────────────────────────────

echo -e "${YELLOW}Phase 5: Create workload VM${NC}"

DEMO_IMAGE="${DEMO_IMAGE:-public.ecr.aws/docker/library/alpine:latest}"
DEMO_VM_NAME="alpine-vm"
DEMO_VM_MEMORY=268435456  # 256 MiB

export QARAX_SERVER="${QARAX_API}"

echo "Creating overlaybd storage pool..."
"$QARAX_CLI" storage-pool create --name overlaybd-pool --pool-type overlaybd \
	--config '{"url":"http://'"${HOST_TAP_IP}"':'"${REGISTRY_PORT}"'"}'

echo "Attaching pool to host..."
"$QARAX_CLI" storage-pool attach-host overlaybd-pool local-node

echo "Importing OCI image: ${DEMO_IMAGE}..."
"$QARAX_CLI" storage-pool import --pool overlaybd-pool \
	--image-ref "${DEMO_IMAGE}" \
	--name alpine-obd

echo "Creating VM: ${DEMO_VM_NAME}..."
"$QARAX_CLI" vm create --name "${DEMO_VM_NAME}" --vcpus 1 --memory "${DEMO_VM_MEMORY}"

echo "Attaching disk..."
"$QARAX_CLI" vm attach-disk "${DEMO_VM_NAME}" --object alpine-obd

echo "Starting VM..."
"$QARAX_CLI" vm start "${DEMO_VM_NAME}"

echo ""

# ── Phase 6: Summary ──────────────────────────────────────────────────────

echo -e "${GREEN}=== Hyperconverged qarax Demo Ready ===${NC}"
echo ""
echo "Control Plane VM (hyperconverged — API + compute):"
echo "  API:         http://${CP_VM_IP}:8000/"
echo "  Swagger UI:  http://${CP_VM_IP}:8000/swagger-ui"
echo "  qarax-node:  ${CP_VM_IP}:${QARAX_NODE_PORT}"
echo "  Console log: ${CP_CONSOLE_LOG}"
echo ""
echo "Workload VM:"
echo "  Name:        ${DEMO_VM_NAME}"
echo "  Image:       ${DEMO_IMAGE}"
echo ""
echo "Local OCI Registry:"
echo "  Host URL:    http://localhost:${REGISTRY_PORT}"
echo "  VM URL:      http://${HOST_TAP_IP}:${REGISTRY_PORT}"
echo ""
echo "Set server for CLI commands:"
echo "  export QARAX_SERVER=http://${CP_VM_IP}:8000"
echo ""
echo "Interact with the workload VM:"
echo "  qarax vm attach ${DEMO_VM_NAME}"
echo "  qarax vm list"
echo ""
echo "Cleanup:"
echo "  sudo ./demos/demo-hyperconverged.sh --cleanup"
echo ""
