-- Rename disk_id to logical_name: disk_id was confusing since id (UUID) is the
-- real disk identifier. logical_name holds the CH device name ("vda", "rootfs", etc).
ALTER TABLE vm_disks RENAME COLUMN disk_id TO logical_name;

-- Update the unique constraint to use the new column name
ALTER TABLE vm_disks DROP CONSTRAINT vm_disks_vm_disk_id_unique;
ALTER TABLE vm_disks ADD CONSTRAINT vm_disks_vm_logical_name_unique UNIQUE (vm_id, logical_name);

-- Update index
DROP INDEX IF EXISTS idx_vm_disks_id;
CREATE INDEX IF NOT EXISTS idx_vm_disks_logical_name ON vm_disks(logical_name);
