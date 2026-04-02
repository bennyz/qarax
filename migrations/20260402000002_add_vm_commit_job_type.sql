-- Add VM_COMMIT job type for converting OCI image-based VMs to raw disk images.
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'VM_COMMIT';
