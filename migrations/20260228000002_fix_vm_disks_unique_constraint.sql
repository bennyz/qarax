-- Fix disk_id uniqueness: should be per-VM, not global.
-- Two different VMs should both be able to have a disk called "vda".
ALTER TABLE vm_disks DROP CONSTRAINT vm_disks_disk_id_unique;
ALTER TABLE vm_disks ADD CONSTRAINT vm_disks_vm_disk_id_unique UNIQUE (vm_id, disk_id);
