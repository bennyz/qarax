DROP TABLE vm_snapshots;

CREATE TABLE vm_snapshots (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vm_id             UUID NOT NULL REFERENCES vms(id) ON DELETE CASCADE,
    storage_object_id UUID NOT NULL REFERENCES storage_objects(id) ON DELETE CASCADE,
    name              TEXT NOT NULL DEFAULT '',
    status            snapshot_status NOT NULL DEFAULT 'CREATING',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX vm_snapshots_vm_id_idx        ON vm_snapshots(vm_id);
CREATE INDEX vm_snapshots_storage_obj_idx  ON vm_snapshots(storage_object_id);
