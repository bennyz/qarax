CREATE TYPE storage_pool_type AS ENUM (
    'LOCAL',
    'NFS',
    'CEPH',
    'LVM',
    'ZFS'
);

CREATE TYPE storage_pool_status AS ENUM (
    'ACTIVE',
    'INACTIVE',
    'ERROR'
);

CREATE TABLE storage_pools (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    name VARCHAR(100) UNIQUE NOT NULL,
    pool_type storage_pool_type NOT NULL,
    status storage_pool_status NOT NULL DEFAULT 'INACTIVE',
    host_id UUID REFERENCES hosts(id),
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    capacity_bytes BIGINT,
    allocated_bytes BIGINT DEFAULT 0,
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT check_capacity CHECK (allocated_bytes <= capacity_bytes OR capacity_bytes IS NULL)
);

CREATE INDEX idx_storage_pools_host ON storage_pools(host_id);
CREATE INDEX idx_storage_pools_type ON storage_pools(pool_type);
CREATE INDEX idx_storage_pools_status ON storage_pools(status);

CREATE TRIGGER update_storage_pools_modtime
BEFORE UPDATE ON storage_pools
FOR EACH ROW
EXECUTE FUNCTION update_modified_column();
