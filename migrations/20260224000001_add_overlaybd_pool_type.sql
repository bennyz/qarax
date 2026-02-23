-- Add OVERLAYBD variant to storage_pool_type enum
-- PostgreSQL requires creating a new enum type and migrating the column

CREATE TYPE storage_pool_type_new AS ENUM ('LOCAL', 'NFS', 'OVERLAYBD');

ALTER TABLE storage_pools
    ALTER COLUMN pool_type TYPE storage_pool_type_new
    USING pool_type::text::storage_pool_type_new;

DROP TYPE storage_pool_type;

ALTER TYPE storage_pool_type_new RENAME TO storage_pool_type;
