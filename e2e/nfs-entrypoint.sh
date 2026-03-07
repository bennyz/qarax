#!/bin/sh
set -e

SHARED_DIR="${SHARED_DIRECTORY:-/nfs-export}"
mkdir -p "$SHARED_DIR"

echo "${SHARED_DIR} *(rw,sync,no_subtree_check,no_root_squash)" > /etc/exports

# Mount the nfsd kernel filesystem — requires --privileged
mount -t nfsd nfsd /proc/fs/nfsd 2>/dev/null || true

rpcbind -w

# Wait for rpcbind to be ready before starting other RPC services
until rpcinfo -p localhost > /dev/null 2>&1; do
    sleep 0.5
done

rpc.nfsd 8
rpc.mountd --no-udp
exportfs -ra

echo "NFS server ready: $(cat /etc/exports)"

# Stay alive and keep exports current
while true; do
    sleep 30
    exportfs -ra 2>/dev/null || true
done
