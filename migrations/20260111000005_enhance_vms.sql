-- Extend hypervisor enum to include QEMU
ALTER TYPE hypervisor ADD VALUE IF NOT EXISTS 'QEMU';

-- Add new columns to vms table
ALTER TABLE vms
  ADD COLUMN boot_source_id UUID REFERENCES boot_sources(id),
  ADD COLUMN description TEXT;

-- Add indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_vms_hypervisor ON vms(hypervisor);
CREATE INDEX IF NOT EXISTS idx_vms_host_id ON vms(host_id);
CREATE INDEX IF NOT EXISTS idx_vms_status ON vms(status);
CREATE INDEX IF NOT EXISTS idx_vms_boot_source ON vms(boot_source_id);
