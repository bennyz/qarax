-- Allow a VM to declare an OCI image as its root filesystem.
-- When set, the control plane will call PullImage on the node and wire up virtiofs.
ALTER TABLE vms ADD COLUMN IF NOT EXISTS image_ref VARCHAR(512);
