-- VM filesystem (virtiofs) devices — one row per virtiofs mount per VM.
-- Each filesystem maps to a nydusd instance serving an OCI image via vhost-user-fs.
CREATE TABLE vm_filesystems (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vm_id           UUID NOT NULL REFERENCES vms(id) ON DELETE CASCADE,

    -- Virtiofs mount tag (used as the mount source in the guest kernel cmdline)
    tag             VARCHAR(64) NOT NULL,

    -- Performance tuning
    num_queues      INTEGER NOT NULL DEFAULT 1,
    queue_size      INTEGER NOT NULL DEFAULT 1024,

    -- PCI configuration
    pci_segment     INTEGER,

    -- OCI image metadata (populated when booting from an OCI image)
    image_ref       VARCHAR(512),   -- e.g. "docker.io/library/ubuntu:22.04"
    image_digest    VARCHAR(128),   -- sha256:...

    -- Ensure each VM has at most one filesystem per tag
    UNIQUE (vm_id, tag)
);
