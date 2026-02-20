#!/usr/bin/env bash
# Register the qarax-node as a host and set it to UP.
# Required for VM scheduling (the control plane picks a host in UP state).
#
# Looks up hosts by address so stale data from previous runs doesn't cause failures.
# Usage: ./setup_host.sh [QARAX_URL] [QARAX_NODE_HOST] [QARAX_NODE_PORT]

set -e

QARAX_URL="${1:-http://localhost:8000}"
QARAX_NODE_HOST="${2:-qarax-node}"
QARAX_NODE_PORT="${3:-50051}"

echo "Registering host (${QARAX_NODE_HOST}:${QARAX_NODE_PORT})..."

# Attempt registration; ignore errors since the host may already exist with a
# different name (the address column has a UNIQUE constraint).
curl -s -X POST "${QARAX_URL}/hosts" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"e2e-node\",\"address\":\"${QARAX_NODE_HOST}\",\"port\":${QARAX_NODE_PORT},\"host_user\":\"root\",\"password\":\"\"}" \
  -o /dev/null || true

# Find the host by address (name may differ from previous runs)
host_id=$(curl -s "${QARAX_URL}/hosts" | python3 -c "
import sys, json
for h in json.load(sys.stdin):
    if h.get('address') == '${QARAX_NODE_HOST}':
        print(h['id'])
        break
")

if [ -z "$host_id" ]; then
  echo "ERROR: Could not find a host with address '${QARAX_NODE_HOST}'" >&2
  curl -s "${QARAX_URL}/hosts" >&2
  exit 1
fi

curl -s -X PATCH "${QARAX_URL}/hosts/${host_id}" \
  -H "Content-Type: application/json" -d '{"status":"up"}' -o /dev/null

echo "Host set to UP (id: ${host_id})"
