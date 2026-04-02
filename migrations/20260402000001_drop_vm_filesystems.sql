-- Remove the vm_filesystems table (virtiofs support removed).
-- OCI image VMs now use OverlayBD exclusively.
DROP TABLE IF EXISTS vm_filesystems;
