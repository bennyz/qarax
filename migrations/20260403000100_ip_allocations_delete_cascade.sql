DELETE FROM ip_allocations WHERE vm_id IS NULL;

ALTER TABLE ip_allocations
DROP CONSTRAINT IF EXISTS ip_allocations_vm_id_fkey;

ALTER TABLE ip_allocations
ADD CONSTRAINT ip_allocations_vm_id_fkey
FOREIGN KEY (vm_id) REFERENCES vms(id) ON DELETE CASCADE;
