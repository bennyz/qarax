-- Add Cloud Hypervisor and kernel version tracking to hosts
ALTER TABLE hosts
    ADD COLUMN cloud_hypervisor_version TEXT,
    ADD COLUMN kernel_version TEXT;
