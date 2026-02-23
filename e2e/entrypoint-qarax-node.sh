#!/bin/sh
# Entrypoint for qarax-node container.
# Loads the TCMU kernel module and starts overlaybd-tcmu before qarax-node.
set -e

# Load the TCMU kernel module if available (required for overlaybd-tcmu)
modprobe target_core_user 2>/dev/null || true

# Start overlaybd-tcmu daemon if available
if command -v overlaybd-tcmu >/dev/null 2>&1; then
    echo "Starting overlaybd-tcmu..."
    overlaybd-tcmu &
    sleep 1
    echo "overlaybd-tcmu started"
else
    echo "overlaybd-tcmu not found, OverlayBD disabled"
fi

exec /usr/local/bin/qarax-node --port 50051 "$@"
