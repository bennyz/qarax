-- Replace the host_id column on storage_pools with a proper many-to-many join table.
-- Storage pools are shared cluster-wide resources; multiple hosts can attach to the same pool.

ALTER TABLE storage_pools DROP COLUMN IF EXISTS host_id;
DROP INDEX IF EXISTS idx_storage_pools_host;

CREATE TABLE host_storage_pools (
    host_id          UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    storage_pool_id  UUID NOT NULL REFERENCES storage_pools(id) ON DELETE CASCADE,
    PRIMARY KEY (host_id, storage_pool_id)
);

CREATE INDEX idx_host_storage_pools_host ON host_storage_pools(host_id);
CREATE INDEX idx_host_storage_pools_pool ON host_storage_pools(storage_pool_id);
