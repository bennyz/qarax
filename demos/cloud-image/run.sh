#!/usr/bin/env bash
#
# Demo: Boot a VM from a cloud image with cloud-init credential injection
#
# This script demonstrates the cloud image workflow:
#   1. Create a local storage pool
#   2. Download a cloud image (e.g. Ubuntu 22.04) directly into the pool
#   3. Create a VM with the image as its root disk + cloud-init to inject SSH keys
#   4. Start the VM
#
# The result is a fully functional VM with a standalone raw disk — no OCI registry,
# no OverlayBD, no external dependency after the download.
#
# Prerequisites:
#   - qarax stack running (make run-local or hack/run-local.sh)
#   - qarax CLI on PATH
#   - A host registered and in "up" state
#   - Internet access from the qarax-node for the image download
#   - An SSH public key at ~/.ssh/id_rsa.pub (or set SSH_PUB_KEY)
#
# Usage:
#   ./demos/cloud-image/run.sh
#   ./demos/cloud-image/run.sh --image-url https://example.com/custom.img --name my-vm
#   ./demos/cloud-image/run.sh --preallocate   # reserve blocks upfront
#

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "${REPO_ROOT}/demos/lib.sh"

# Defaults
VM_NAME="demo-cloud-image-vm"
HOST_NAME="${QARAX_HOST:-local-node}"
POOL_NAME="demo-cloud-image-pool"
POOL_PATH="/var/lib/qarax/cloud-image-pool"
NETWORK_NAME="demo-cloud-image-net"
NETWORK_SUBNET="10.97.0.0/24"
NETWORK_GATEWAY="10.97.0.1"
BRIDGE_NAME="qci0"
PARENT_INTERFACE=""
DISK_NAME="ubuntu-22.04-cloud"
IMAGE_URL="https://cloud-images.ubuntu.com/minimal/releases/jammy/release/ubuntu-22.04-minimal-cloudimg-amd64.img"
VCPUS=2
MEMORY_GIB=1
SERVER="${QARAX_SERVER:-http://localhost:8000}"
SSH_PUB_KEY="${SSH_PUB_KEY:-$(cat ~/.ssh/id_rsa.pub 2>/dev/null || echo '')}"
PREALLOCATE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
	case $1 in
	--name)
		VM_NAME="$2"
		shift 2
		;;
	--pool-name)
		POOL_NAME="$2"
		shift 2
		;;
	--pool-path)
		POOL_PATH="$2"
		shift 2
		;;
	--network-name)
		NETWORK_NAME="$2"
		shift 2
		;;
	--subnet)
		NETWORK_SUBNET="$2"
		shift 2
		;;
	--gateway)
		NETWORK_GATEWAY="$2"
		shift 2
		;;
	--bridge-name)
		BRIDGE_NAME="$2"
		shift 2
		;;
	--parent-interface)
		PARENT_INTERFACE="$2"
		shift 2
		;;
	--disk-name)
		DISK_NAME="$2"
		shift 2
		;;
	--image-url)
		IMAGE_URL="$2"
		shift 2
		;;
	--host)
		HOST_NAME="$2"
		shift 2
		;;
	--vcpus)
		VCPUS="$2"
		shift 2
		;;
	--memory)
		MEMORY_GIB="$2"
		shift 2
		;;
	--ssh-key)
		SSH_PUB_KEY="$2"
		shift 2
		;;
	--preallocate)
		PREALLOCATE=true
		shift
		;;
	--server)
		SERVER="$2"
		shift 2
		;;
	*)
		echo "Unknown argument: $1" >&2
		exit 1
		;;
	esac
done

export QARAX_SERVER="$SERVER"

QARAX_BIN="$(find_qarax_bin)"
[[ -z "$QARAX_BIN" ]] && die "qarax CLI not found. Run 'make build' or add it to PATH."
QARAX="$QARAX_BIN --server $SERVER"

ensure_stack "$SERVER"

wait_for_job() {
	local job_id="$1"
	local label="$2"

	while true; do
		local job_json status progress
		job_json=$($QARAX job get "$job_id" --output json)
		status=$(jq -r '.status' <<<"$job_json")
		case "$status" in
		completed)
			echo "  ${label}: completed"
			return 0
			;;
		failed)
			echo "  ${label}: failed" >&2
			jq -r '.error // "unknown error"' <<<"$job_json" >&2
			return 1
			;;
		*)
			progress=$(jq -r '.progress // 0' <<<"$job_json")
			echo -ne "\r  ${label}: [${status}] ${progress}%   "
			sleep 2
			;;
		esac
	done
}

if [[ -z "$SSH_PUB_KEY" ]]; then
	echo "Error: no SSH public key found. Set SSH_PUB_KEY or ensure ~/.ssh/id_rsa.pub exists." >&2
	exit 1
fi

echo "=== Cloud Image VM Demo ==="
echo "  Image URL:  $IMAGE_URL"
echo "  VM name:    $VM_NAME"
echo "  Pool:       $POOL_NAME ($POOL_PATH)"
echo "  Network:    $NETWORK_NAME ($NETWORK_SUBNET)"
echo "  Memory:     ${MEMORY_GIB} GiB"
echo "  Preallocate: $PREALLOCATE"
echo ""

# ── Step 1: Storage pool ──────────────────────────────────────────────────────

echo "→ Creating storage pool '$POOL_NAME'..."
POOL_ID=$($QARAX storage-pool create \
	--name "$POOL_NAME" \
	--pool-type local \
	--path "$POOL_PATH" \
	--host "$HOST_NAME" \
	--output json | jq -r '.pool_id' 2>/dev/null ||
	$QARAX storage-pool list --output json | jq -r ".[] | select(.name==\"$POOL_NAME\") | .id")

echo "  Pool: $POOL_ID"

# ── Step 2: Network ────────────────────────────────────────────────────────────

echo "→ Creating network '$NETWORK_NAME'..."
NETWORK_ID=$($QARAX network create \
	--name "$NETWORK_NAME" \
	--subnet "$NETWORK_SUBNET" \
	--gateway "$NETWORK_GATEWAY" \
	--output json | jq -r '.network_id' 2>/dev/null ||
	$QARAX network list --output json | jq -r ".[] | select(.name==\"$NETWORK_NAME\") | .id")

echo "  Network: $NETWORK_ID"

echo "→ Attaching network to host '$HOST_NAME'..."
if [[ -n "$PARENT_INTERFACE" ]]; then
	$QARAX network attach-host \
		--network "$NETWORK_NAME" \
		--host "$HOST_NAME" \
		--bridge-name "$BRIDGE_NAME" \
		--parent-interface "$PARENT_INTERFACE"
else
	$QARAX network attach-host \
		--network "$NETWORK_NAME" \
		--host "$HOST_NAME" \
		--bridge-name "$BRIDGE_NAME"
fi

# ── Step 3: Download cloud image into the pool ────────────────────────────────

echo "→ Downloading cloud image into pool (this may take a few minutes)..."

PREALLOCATE_FLAG=""
if [[ "$PREALLOCATE" == "true" ]]; then
	PREALLOCATE_FLAG="--preallocate"
fi

DISK_RESULT=$($QARAX storage-pool create-disk \
	--pool "$POOL_NAME" \
	--name "$DISK_NAME" \
	--source "$IMAGE_URL" \
	$PREALLOCATE_FLAG \
	--output json)

DISK_ID=$(echo "$DISK_RESULT" | jq -r '.storage_object_id')
DISK_JOB_ID=$(echo "$DISK_RESULT" | jq -r '.job_id // empty')
echo "  Disk: $DISK_ID"
if [[ -n "$DISK_JOB_ID" ]]; then
	wait_for_job "$DISK_JOB_ID" "Disk download"
fi

# ── Step 4: Create VM with the disk as root + cloud-init ──────────────────────

echo "→ Creating VM '$VM_NAME'..."

USER_DATA="#cloud-config
users:
  - name: qarax
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
     ssh_authorized_keys:
       - $SSH_PUB_KEY
growpart:
  mode: auto
  devices: ['/']
resize_rootfs: true
runcmd:
  - [sh, -c, 'echo demo-cloud-image > /var/tmp/index.html']
  - [sh, -c, 'nohup python3 -m http.server 8080 --directory /var/tmp >/var/log/qarax-demo-http.log 2>&1 &']"

USER_DATA_FILE="$(mktemp /tmp/qarax-cloud-init-XXXXXX.yaml)"
trap 'rm -f "$USER_DATA_FILE"' EXIT
printf '%s\n' "$USER_DATA" >"$USER_DATA_FILE"

VM_ID=$($QARAX vm create \
	--name "$VM_NAME" \
	--vcpus "$VCPUS" \
	--memory "${MEMORY_GIB}GiB" \
	--boot-mode firmware \
	--root-disk "$DISK_ID" \
	--network "$NETWORK_NAME" \
	--cloud-init-user-data "$USER_DATA_FILE" \
	--output json | jq -r '.vm_id')

echo "  VM: $VM_ID"

# ── Step 5: Start VM ──────────────────────────────────────────────────────────

echo "→ Starting VM..."
$QARAX vm start "$VM_NAME"
VM_IP=$($QARAX network list-ips "$NETWORK_NAME" --output json |
	jq -r ".[] | select(.vm_id==\"$VM_ID\") | .ip_address" |
	head -n1)
VM_IP="${VM_IP%/32}"

echo ""
echo "=== Done ==="
echo "VM '$VM_NAME' is booting."
echo "cloud-init will set up the 'qarax' user with your SSH key on first boot and"
echo "start a tiny HTTP server on port 8080."
echo ""
if [[ -n "$VM_IP" ]]; then
	echo "Allocated IP: $VM_IP"
fi
echo ""
if [[ -n "$PARENT_INTERFACE" ]]; then
	echo "The network is bridged to '$PARENT_INTERFACE', so the VM should be reachable"
	echo "from your LAN once cloud-init finishes:"
	echo "  curl http://$VM_IP:8080"
	echo "  ssh qarax@$VM_IP"
else
	echo "Compose mode note: Qarax bridge networks live inside the qarax-node namespace."
	echo "Verify reachability from the node container:"
	echo "  docker exec e2e-qarax-node-1 bash -lc 'timeout 3 bash -lc \"</dev/tcp/$VM_IP/22\" && echo ssh-open'"
	echo "  docker exec e2e-qarax-node-1 curl http://$VM_IP:8080"
fi
echo ""
echo "To inspect the VM:"
echo "  $QARAX vm get $VM_NAME"
