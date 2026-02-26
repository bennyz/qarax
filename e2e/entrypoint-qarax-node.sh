#!/bin/sh
# Entrypoint for qarax-node container.
# Loads the TCMU and loopback kernel modules, creates UIO device nodes for
# overlaybd-tcmu inside the container's /dev/, then starts the daemon.
set -e

# Load TCMU kernel module (required for overlaybd-tcmu backstore)
modprobe target_core_user 2>/dev/null || true

# Load the loopback SCSI transport (required to expose TCMU backstores as
# block devices via /sys/kernel/config/target/loopback/)
modprobe tcm_loop 2>/dev/null || true

# Create UIO character device nodes inside the container's /dev/.
#
# When target_core_user enables a TCMU device the kernel calls
# uio_register_device(), which allocates a UIO minor and adds an entry to
# /sys/class/uio/ on the HOST. The container's /dev/ is isolated, so udev
# never creates /dev/uio0 here. We pre-create nodes for minors 0-7 using the
# UIO major number read from /proc/devices after the modules are loaded.
UIO_MAJOR=$(awk '/[[:space:]]uio$/{print $1}' /proc/devices 2>/dev/null | head -1)
if [ -n "$UIO_MAJOR" ]; then
    echo "Creating UIO device nodes (major=${UIO_MAJOR}) in container /dev/"
    for i in 0 1 2 3 4 5 6 7; do
        [ -e "/dev/uio${i}" ] || mknod "/dev/uio${i}" c "${UIO_MAJOR}" "${i}"
    done
else
    echo "WARNING: could not determine UIO major from /proc/devices; overlaybd-tcmu may fail to open /dev/uio0"
fi

# Start overlaybd-tcmu daemon if available (installed to /opt/overlaybd/bin/)
if [ -x /opt/overlaybd/bin/overlaybd-tcmu ]; then
    echo "Starting overlaybd-tcmu..."
    /opt/overlaybd/bin/overlaybd-tcmu &
    sleep 1
    echo "overlaybd-tcmu started"
else
    echo "overlaybd-tcmu not found, OverlayBD disabled"
fi

exec /usr/local/bin/qarax-node --port 50051 "$@"
