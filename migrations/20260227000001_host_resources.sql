ALTER TABLE hosts
    ADD COLUMN total_cpus INT,
    ADD COLUMN total_memory_bytes BIGINT,
    ADD COLUMN available_memory_bytes BIGINT,
    ADD COLUMN load_average DOUBLE PRECISION,
    ADD COLUMN disk_total_bytes BIGINT,
    ADD COLUMN disk_available_bytes BIGINT,
    ADD COLUMN resources_updated_at TIMESTAMPTZ;
