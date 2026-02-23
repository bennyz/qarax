-- Track OverlayBD disk mounts for VMs booted from OCI images via overlaybd-tcmu
CREATE TABLE vm_overlaybd_disks (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vm_id           UUID NOT NULL REFERENCES vms(id) ON DELETE CASCADE,
    disk_id         VARCHAR(64) NOT NULL,       -- e.g. "vda"
    image_ref       VARCHAR(512) NOT NULL,
    image_digest    VARCHAR(128),
    registry_url    VARCHAR(512) NOT NULL,
    storage_pool_id UUID REFERENCES storage_pools(id),
    boot_order      INT NOT NULL DEFAULT 0,     -- 0 = boot disk
    UNIQUE (vm_id, disk_id)
);
