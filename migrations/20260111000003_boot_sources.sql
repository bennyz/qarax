CREATE TABLE boot_sources (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    name VARCHAR(100) UNIQUE NOT NULL,
    description TEXT,
    kernel_image_id UUID REFERENCES storage_objects(id) NOT NULL,
    kernel_params TEXT,
    initrd_image_id UUID REFERENCES storage_objects(id),
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITHOUT TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_boot_sources_kernel ON boot_sources(kernel_image_id);
CREATE INDEX idx_boot_sources_initrd ON boot_sources(initrd_image_id);

CREATE TRIGGER update_boot_sources_modtime
BEFORE UPDATE ON boot_sources
FOR EACH ROW
EXECUTE FUNCTION update_modified_column();
