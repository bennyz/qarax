-- Remove ceph, lvm, zfs from storage_pool_type; keep only local and nfs.
-- Pools with CEPH, LVM, or ZFS are migrated to LOCAL.

CREATE TYPE storage_pool_type_new AS ENUM ('LOCAL', 'NFS');

ALTER TABLE storage_pools
  ALTER COLUMN pool_type TYPE storage_pool_type_new
  USING (
    CASE pool_type::text
      WHEN 'LOCAL' THEN 'LOCAL'::storage_pool_type_new
      WHEN 'NFS' THEN 'NFS'::storage_pool_type_new
      ELSE 'LOCAL'::storage_pool_type_new
    END
  );

DROP TYPE storage_pool_type;

ALTER TYPE storage_pool_type_new RENAME TO storage_pool_type;
