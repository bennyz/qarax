"""
E2E test configuration and shared fixtures.

Registers the qarax-node as a host and sets it to UP before running any tests.
This is required for VM scheduling (the control plane picks a host in UP state).
"""

import os

import httpx
import pytest

QARAX_URL = os.getenv("QARAX_URL", "http://localhost:8000")
# Address of qarax-node as seen from the qarax control plane (inside docker network)
QARAX_NODE_HOST = os.getenv("QARAX_NODE_HOST", "qarax-node")
QARAX_NODE_PORT = int(os.getenv("QARAX_NODE_PORT", "50051"))


def _find_host_by_address(client: httpx.Client, address: str) -> str | None:
    """Return the host ID for the first host with the given address, or None."""
    resp = client.get("/hosts")
    resp.raise_for_status()
    for host in resp.json():
        if host.get("address") == address:
            return host["id"]
    return None


@pytest.fixture(scope="session", autouse=True)
def ensure_host_registered():
    """Register the qarax-node host and ensure it is in UP state before tests run."""
    with httpx.Client(base_url=QARAX_URL) as client:
        # Check if there is already a host pointing at our qarax-node address
        host_id = _find_host_by_address(client, QARAX_NODE_HOST)

        if host_id is None:
            # Not registered yet — register it now
            resp = client.post(
                "/hosts",
                json={
                    "name": "e2e-node",
                    "address": QARAX_NODE_HOST,
                    "port": QARAX_NODE_PORT,
                    "host_user": "root",
                    "password": "",
                },
            )
            if resp.status_code == 201:
                host_id = resp.text.strip().strip('"')
            else:
                # Could be a 409/422/500 due to stale DB state; re-fetch to find it
                host_id = _find_host_by_address(client, QARAX_NODE_HOST)

        if host_id is None:
            raise RuntimeError(
                f"Could not register or find a host at {QARAX_NODE_HOST}:{QARAX_NODE_PORT}"
            )

        # Ensure the host is in UP state so the scheduler can assign VMs to it
        resp = client.patch(f"/hosts/{host_id}", json={"status": "up"})
        if resp.status_code not in (200, 204):
            raise RuntimeError(
                f"Failed to set host status to UP: HTTP {resp.status_code} — {resp.text}"
            )
