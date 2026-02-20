#!/usr/bin/env bash
#
# Run a real local host-deploy test using a libvirt VM.
#
# What this does:
#   1) Creates a cloud-init VM in libvirt (Ubuntu cloud image)
#   2) Configures SSH + a bootc stub + a qarax-node stub listener on port 50051
#   3) Registers the VM as a host in qarax
#   4) Calls POST /hosts/{host_id}/deploy with reboot=true
#   5) Waits for host status to become UP
#
# Usage:
#   bash hack/test-host-deploy-libvirt.sh
#   bash hack/test-host-deploy-libvirt.sh --start-stack
#   bash hack/test-host-deploy-libvirt.sh --keep-vm
#   bash hack/test-host-deploy-libvirt.sh --cleanup
#

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

API_URL="${API_URL:-http://localhost:8000}"
VM_NAME="${VM_NAME:-qarax-deploy-test}"
VM_MEMORY_MB="${VM_MEMORY_MB:-2048}"
VM_VCPUS="${VM_VCPUS:-2}"
VM_DISK_SIZE="${VM_DISK_SIZE:-20G}"

SSH_USER="${SSH_USER:-qarax}"
SSH_PASSWORD="${SSH_PASSWORD:-qarax}"
SSH_PORT="${SSH_PORT:-22}"
NODE_PORT="${NODE_PORT:-50051}"

DEPLOY_IMAGE="${DEPLOY_IMAGE:-ghcr.io/qarax/qarax-vmm-host:latest}"
CLOUD_IMAGE_URL="${CLOUD_IMAGE_URL:-https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img}"

STATE_DIR="${STATE_DIR:-/tmp/qarax-libvirt-deploy}"
BASE_IMAGE="${STATE_DIR}/base-noble.img"
VM_DISK="${STATE_DIR}/${VM_NAME}.qcow2"
SEED_ISO="${STATE_DIR}/${VM_NAME}-seed.iso"
USER_DATA="${STATE_DIR}/${VM_NAME}-user-data.yaml"
META_DATA="${STATE_DIR}/${VM_NAME}-meta-data.yaml"

START_STACK=0
KEEP_VM=0
CLEANUP_ONLY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --start-stack)
      START_STACK=1
      shift
      ;;
    --keep-vm)
      KEEP_VM=1
      shift
      ;;
    --cleanup)
      CLEANUP_ONLY=1
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

wait_for_http() {
  local url="$1"
  local timeout_s="${2:-60}"
  local start_ts
  start_ts="$(date +%s)"
  while true; do
    if curl -fsS "${url}" >/dev/null 2>&1; then
      return 0
    fi
    if (( "$(date +%s)" - start_ts >= timeout_s )); then
      echo "Timed out waiting for ${url}" >&2
      return 1
    fi
    sleep 2
  done
}

ensure_stack() {
  if wait_for_http "${API_URL}/" 2; then
    return 0
  fi

  if [[ "${START_STACK}" -ne 1 ]]; then
    echo "qarax API is not reachable at ${API_URL}." >&2
    echo "Start it first (e.g. ./hack/run-local.sh) or re-run with --start-stack." >&2
    exit 1
  fi

  echo "Starting local qarax stack..."
  bash "${REPO_ROOT}/hack/run-local.sh"
  wait_for_http "${API_URL}/" 120
}

ensure_libvirt_network() {
  if ! virsh net-info default >/dev/null 2>&1; then
    echo "libvirt network 'default' not found. Configure libvirt networking first." >&2
    exit 1
  fi
  virsh net-start default >/dev/null 2>&1 || true
  virsh net-autostart default >/dev/null 2>&1 || true
}

cleanup_vm() {
  if virsh dominfo "${VM_NAME}" >/dev/null 2>&1; then
    virsh destroy "${VM_NAME}" >/dev/null 2>&1 || true
    virsh undefine "${VM_NAME}" --nvram >/dev/null 2>&1 || virsh undefine "${VM_NAME}" >/dev/null 2>&1 || true
  fi
  rm -f "${VM_DISK}" "${SEED_ISO}" "${USER_DATA}" "${META_DATA}"
}

create_cloud_init() {
  mkdir -p "${STATE_DIR}"
  cat > "${USER_DATA}" <<EOF
#cloud-config
hostname: ${VM_NAME}
manage_etc_hosts: true
users:
  - name: ${SSH_USER}
    shell: /bin/bash
    groups: [sudo]
    sudo: ALL=(ALL) NOPASSWD:ALL
    lock_passwd: false
chpasswd:
  expire: false
  users:
    - name: ${SSH_USER}
      password: ${SSH_PASSWORD}
ssh_pwauth: true
package_update: true
packages:
  - socat
  - qemu-guest-agent
write_files:
  - path: /usr/local/bin/bootc
    permissions: "0755"
    owner: root:root
    content: |
      #!/usr/bin/env bash
      set -euo pipefail
      if [[ "\${1:-}" == "switch" ]]; then
        echo "bootc switch requested: \$*" > /var/log/bootc-switch.log
        exit 0
      fi
      echo "bootc stub unsupported args: \$*" >&2
      exit 1
  - path: /etc/systemd/system/qarax-node-stub.service
    permissions: "0644"
    owner: root:root
    content: |
      [Unit]
      Description=qarax-node stub listener for deploy tests
      After=network-online.target
      Wants=network-online.target

      [Service]
      ExecStart=/usr/bin/socat TCP-LISTEN:${NODE_PORT},reuseaddr,fork EXEC:/bin/cat
      Restart=always
      RestartSec=1

      [Install]
      WantedBy=multi-user.target
runcmd:
  - systemctl daemon-reload
  - systemctl enable --now qemu-guest-agent
  - systemctl enable --now qarax-node-stub.service
  - systemctl restart ssh || true
EOF

  cat > "${META_DATA}" <<EOF
instance-id: ${VM_NAME}
local-hostname: ${VM_NAME}
EOF

  cloud-localds "${SEED_ISO}" "${USER_DATA}" "${META_DATA}"
}

create_vm() {
  mkdir -p "${STATE_DIR}"
  if [[ ! -f "${BASE_IMAGE}" ]]; then
    echo "Downloading Ubuntu cloud image..."
    curl -fL "${CLOUD_IMAGE_URL}" -o "${BASE_IMAGE}"
  fi

  qemu-img create -f qcow2 -F qcow2 -b "${BASE_IMAGE}" "${VM_DISK}" "${VM_DISK_SIZE}" >/dev/null
  create_cloud_init

  virt-install \
    --name "${VM_NAME}" \
    --memory "${VM_MEMORY_MB}" \
    --vcpus "${VM_VCPUS}" \
    --cpu host-passthrough \
    --import \
    --disk "path=${VM_DISK},format=qcow2,bus=virtio" \
    --disk "path=${SEED_ISO},device=cdrom" \
    --network "network=default,model=virtio" \
    --os-variant ubuntu24.04 \
    --graphics none \
    --noautoconsole \
    --rng /dev/urandom
}

wait_for_vm_ip() {
  local timeout_s="${1:-240}"
  local start_ts ip
  start_ts="$(date +%s)"

  while true; do
    ip="$(
      virsh domifaddr "${VM_NAME}" --source lease 2>/dev/null \
        | awk '/ipv4/ {print $4}' \
        | cut -d/ -f1 \
        | head -n1
    )"
    if [[ -n "${ip}" ]]; then
      echo "${ip}"
      return 0
    fi

    if (( "$(date +%s)" - start_ts >= timeout_s )); then
      echo "Timed out waiting for VM IP address" >&2
      return 1
    fi
    sleep 2
  done
}

wait_for_tcp() {
  local host="$1"
  local port="$2"
  local timeout_s="$3"
  local start_ts
  start_ts="$(date +%s)"
  while true; do
    if nc -z "${host}" "${port}" >/dev/null 2>&1; then
      return 0
    fi
    if (( "$(date +%s)" - start_ts >= timeout_s )); then
      echo "Timed out waiting for ${host}:${port}" >&2
      return 1
    fi
    sleep 2
  done
}

lookup_host_id_by_address() {
  local address="$1"
  curl -sS "${API_URL}/hosts" | python3 - "$address" <<'PY'
import json
import sys

address = sys.argv[1]
for host in json.load(sys.stdin):
    if host.get("address") == address:
        print(host["id"])
        break
PY
}

lookup_host_status() {
  local host_id="$1"
  curl -sS "${API_URL}/hosts" | python3 - "$host_id" <<'PY'
import json
import sys

host_id = sys.argv[1]
for host in json.load(sys.stdin):
    if host.get("id") == host_id:
        print(host.get("status", ""))
        break
PY
}

register_host() {
  local vm_ip="$1"
  local host_name="libvirt-${VM_NAME}-$(date +%s)"
  local payload
  payload="$(jq -n \
    --arg name "${host_name}" \
    --arg address "${vm_ip}" \
    --arg host_user "${SSH_USER}" \
    --arg password "" \
    --argjson port "${NODE_PORT}" \
    '{name:$name,address:$address,port:$port,host_user:$host_user,password:$password}')"

  local body_file code host_id
  body_file="$(mktemp)"
  code="$(
    curl -sS -o "${body_file}" -w "%{http_code}" \
      -X POST "${API_URL}/hosts" \
      -H "Content-Type: application/json" \
      -d "${payload}"
  )"

  if [[ "${code}" != "201" ]]; then
    echo "Failed to register host (HTTP ${code}):" >&2
    cat "${body_file}" >&2
    rm -f "${body_file}"
    exit 1
  fi

  host_id="$(tr -d '"' < "${body_file}")"
  rm -f "${body_file}"
  echo "${host_id}"
}

trigger_deploy() {
  local host_id="$1"
  local payload body_file code
  payload="$(jq -n \
    --arg image "${DEPLOY_IMAGE}" \
    --arg ssh_user "${SSH_USER}" \
    --arg ssh_password "${SSH_PASSWORD}" \
    --argjson ssh_port "${SSH_PORT}" \
    '{
      image:$image,
      ssh_port:$ssh_port,
      ssh_user:$ssh_user,
      ssh_password:$ssh_password,
      install_bootc:false,
      reboot:true
    }')"

  body_file="$(mktemp)"
  code="$(
    curl -sS -o "${body_file}" -w "%{http_code}" \
      -X POST "${API_URL}/hosts/${host_id}/deploy" \
      -H "Content-Type: application/json" \
      -d "${payload}"
  )"

  if [[ "${code}" != "202" ]]; then
    echo "Deploy request failed (HTTP ${code}):" >&2
    cat "${body_file}" >&2
    rm -f "${body_file}"
    exit 1
  fi
  rm -f "${body_file}"
}

wait_for_deploy_success() {
  local host_id="$1"
  local timeout_s="${2:-420}"
  local start_ts status saw_installing
  start_ts="$(date +%s)"
  saw_installing=0

  while true; do
    status="$(lookup_host_status "${host_id}")"
    case "${status}" in
      installing)
        saw_installing=1
        ;;
      up)
        if [[ "${saw_installing}" -eq 1 ]]; then
          return 0
        fi
        ;;
      installation_failed)
        echo "Host deployment failed (status=installation_failed)." >&2
        return 1
        ;;
      "")
        echo "Host ${host_id} not found while polling status." >&2
        return 1
        ;;
    esac

    if (( "$(date +%s)" - start_ts >= timeout_s )); then
      echo "Timed out waiting for host deployment to finish. Last status=${status}" >&2
      return 1
    fi
    sleep 5
  done
}

run_connectivity_probe_from_qarax_container() {
  local vm_ip="$1"
  if ! docker compose -f "${REPO_ROOT}/e2e/docker-compose.yml" exec -T qarax \
      sh -lc "curl -sS --max-time 3 telnet://${vm_ip}:${SSH_PORT} >/dev/null" >/dev/null 2>&1; then
    echo "qarax container cannot reach ${vm_ip}:${SSH_PORT}." >&2
    echo "Ensure libvirt network is reachable from Docker, then retry." >&2
    exit 1
  fi
}

main() {
  require_cmd virsh
  require_cmd virt-install
  require_cmd qemu-img
  require_cmd cloud-localds
  require_cmd curl
  require_cmd jq
  require_cmd nc
  require_cmd python3

  ensure_libvirt_network

  if [[ "${CLEANUP_ONLY}" -eq 1 ]]; then
    cleanup_vm
    echo "Cleanup complete for VM ${VM_NAME}."
    exit 0
  fi

  ensure_stack

  cleanup_vm
  create_vm

  if [[ "${KEEP_VM}" -ne 1 ]]; then
    trap cleanup_vm EXIT
  fi

  local vm_ip host_id
  vm_ip="$(wait_for_vm_ip 300)"
  echo "VM IP: ${vm_ip}"

  wait_for_tcp "${vm_ip}" "${SSH_PORT}" 240
  wait_for_tcp "${vm_ip}" "${NODE_PORT}" 240
  run_connectivity_probe_from_qarax_container "${vm_ip}"

  host_id="$(register_host "${vm_ip}")"
  echo "Registered host id: ${host_id}"

  trigger_deploy "${host_id}"
  echo "Deploy triggered. Waiting for host to become UP..."

  wait_for_deploy_success "${host_id}" 420

  echo ""
  echo "SUCCESS: deploy flow completed against real libvirt VM."
  echo "Host ${host_id} is UP."
  echo "VM ${VM_NAME} (${vm_ip})"
  if [[ "${KEEP_VM}" -eq 1 ]]; then
    echo "VM was kept because --keep-vm was set."
  fi
}

main "$@"
