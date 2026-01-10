CREATE TABLE vm_disks (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    vm_id UUID NOT NULL REFERENCES vms(id) ON DELETE CASCADE,
    storage_object_id UUID NOT NULL REFERENCES storage_objects(id),
    device_name VARCHAR(20) NOT NULL,
    boot_order INTEGER,
    read_only BOOLEAN DEFAULT false,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(vm_id, device_name),
    UNIQUE(vm_id, boot_order)
);

CREATE INDEX idx_vm_disks_vm ON vm_disks(vm_id);
CREATE INDEX idx_vm_disks_storage ON vm_disks(storage_object_id);
CREATE INDEX idx_vm_disks_boot ON vm_disks(boot_order) WHERE boot_order IS NOT NULL;
