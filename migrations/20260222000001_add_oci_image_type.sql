-- Add OCI_IMAGE variant to storage_object_type enum
ALTER TYPE storage_object_type ADD VALUE IF NOT EXISTS 'OCI_IMAGE';
