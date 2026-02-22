-- Add PENDING state to vm_status for VMs waiting on an async job
ALTER TYPE vm_status ADD VALUE IF NOT EXISTS 'PENDING';

-- Job types (extensible for future async operations)
CREATE TYPE job_type AS ENUM ('IMAGE_PULL');

-- Job lifecycle states
CREATE TYPE job_status AS ENUM ('PENDING', 'RUNNING', 'COMPLETED', 'FAILED');

CREATE TABLE jobs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_type        job_type NOT NULL,
    status          job_status NOT NULL DEFAULT 'PENDING',
    description     TEXT,
    resource_id     UUID,
    resource_type   TEXT,
    progress        INTEGER CHECK (progress BETWEEN 0 AND 100),
    result          JSONB,
    error           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ
);
