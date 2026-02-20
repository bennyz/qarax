-- Async file transfers scoped to storage pools

CREATE TYPE transfer_type AS ENUM ('DOWNLOAD', 'LOCAL_COPY');
CREATE TYPE transfer_status AS ENUM ('PENDING', 'RUNNING', 'COMPLETED', 'FAILED');

CREATE TABLE transfers (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    transfer_type transfer_type NOT NULL,
    status transfer_status NOT NULL DEFAULT 'PENDING',
    source TEXT NOT NULL,
    storage_pool_id UUID REFERENCES storage_pools(id) NOT NULL,
    object_type storage_object_type NOT NULL,
    storage_object_id UUID REFERENCES storage_objects(id),
    total_bytes BIGINT,
    transferred_bytes BIGINT DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    started_at TIMESTAMP WITHOUT TIME ZONE,
    completed_at TIMESTAMP WITHOUT TIME ZONE
);

CREATE INDEX idx_transfers_storage_pool ON transfers(storage_pool_id);
CREATE INDEX idx_transfers_status ON transfers(status);
