-- Make network_id, mac_address, and ip_address nullable to support TAP/vhost-user
-- interfaces where MAC is assigned by Cloud Hypervisor and IP is optional.

ALTER TABLE network_interfaces ALTER COLUMN network_id DROP NOT NULL;
ALTER TABLE network_interfaces ALTER COLUMN mac_address DROP NOT NULL;
ALTER TABLE network_interfaces ALTER COLUMN ip_address DROP NOT NULL;
