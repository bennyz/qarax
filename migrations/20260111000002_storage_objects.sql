CREATE TYPE storage_object_type AS ENUM (
    'DISK',
    'KERNEL',
    'INITRD',
    'ISO',
    'SNAPSHOT'
);

CREATE TABLE storage_objects (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    storage_pool_id UUID REFERENCES storage_pools(id) NOT NULL,
    object_type storage_object_type NOT NULL,
    size_bytes BIGINT NOT NULL,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    parent_id UUID REFERENCES storage_objects(id),
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(storage_pool_id, name)
);

CREATE INDEX idx_storage_objects_pool ON storage_objects(storage_pool_id);
CREATE INDEX idx_storage_objects_type ON storage_objects(object_type);
CREATE INDEX idx_storage_objects_parent ON storage_objects(parent_id);

CREATE TRIGGER update_storage_objects_modtime
BEFORE UPDATE ON storage_objects
FOR EACH ROW
EXECUTE FUNCTION update_modified_column();
