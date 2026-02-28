-- Add networking support: extend networks table, host-network bindings, IPAM

-- Add gateway, dns, and status columns to existing networks table
ALTER TABLE networks ADD COLUMN gateway INET;
ALTER TABLE networks ADD COLUMN dns INET;

CREATE TYPE network_status AS ENUM ('ACTIVE', 'INACTIVE');
ALTER TABLE networks ADD COLUMN status network_status NOT NULL DEFAULT 'ACTIVE';

-- Host-network binding (like host_storage_pools)
CREATE TABLE host_networks (
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    network_id UUID NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
    bridge_name VARCHAR(15) NOT NULL,  -- Linux limit: 15 chars
    PRIMARY KEY (host_id, network_id),
    UNIQUE (host_id, bridge_name)
);

-- IPAM: track allocated IPs
CREATE TABLE ip_allocations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    network_id UUID NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
    ip_address INET NOT NULL,
    vm_id UUID REFERENCES vms(id) ON DELETE SET NULL,
    allocated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (network_id, ip_address)
);
