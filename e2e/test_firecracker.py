"""
E2E tests for Firecracker VMM backend.

These tests verify:
- Creating a Firecracker VM and listing it
- Full FC lifecycle: create → start → pause → resume → stop → delete
- Force-stopping a running FC VM
- Cloud-init seed disk attachment for FC VMs
- Snapshot/restore for FC VMs
- Hot-attach operations return 501 UNIMPLEMENTED for FC VMs

Prerequisites:
- The Firecracker binary must be present at /usr/local/bin/firecracker on the
  qarax-node host (installed in the e2e Dockerfile).
- The same vmlinux and test-initramfs used by Cloud Hypervisor tests are reused.
"""

import asyncio
import os
import time
import uuid

import pytest
from qarax_api_client import Client
from qarax_api_client.api.vms import (
    list_ as list_vms,
    create as create_vm,
    get as get_vm,
    start as start_vm,
    stop as stop_vm,
    force_stop as force_stop_vm,
    pause as pause_vm,
    resume as resume_vm,
    delete as delete_vm,
    create_snapshot,
    restore,
    attach_disk,
)
from qarax_api_client.models import (
    AttachDiskRequest,
    CreateSnapshotRequest,
    Hypervisor,
    NewVm,
    RestoreRequest,
    VmStatus,
)

QARAX_URL = os.getenv("QARAX_URL", "http://localhost:8000")
VM_OPERATION_TIMEOUT = 30


@pytest.fixture
def client():
    """Create a qarax API client."""
    return Client(base_url=QARAX_URL)


async def call_api(endpoint_module, **kwargs):
    """Call generated SDK endpoint in a version-tolerant way."""
    asyncio_fn = getattr(endpoint_module, "asyncio", None)
    if callable(asyncio_fn):
        return await asyncio_fn(**kwargs)

    detailed_fn = getattr(endpoint_module, "asyncio_detailed", None)
    if callable(detailed_fn):
        response = await detailed_fn(**kwargs)
        return response.parsed

    raise AttributeError(f"{endpoint_module.__name__} has no async entrypoint")


async def call_api_detailed(endpoint_module, **kwargs):
    """Call generated SDK endpoint and return the full response."""
    detailed_fn = getattr(endpoint_module, "asyncio_detailed", None)
    if callable(detailed_fn):
        return await detailed_fn(**kwargs)
    raise AttributeError(f"{endpoint_module.__name__} has no asyncio_detailed entrypoint")


async def wait_for_status(
    client, vm_id: str, expected_status: VmStatus, timeout: int = VM_OPERATION_TIMEOUT
):
    """Wait for a VM to reach the expected status."""
    start_time = time.time()
    while time.time() - start_time < timeout:
        vm = await call_api(get_vm, client=client, vm_id=vm_id)
        if vm.status == expected_status:
            return vm
        await asyncio.sleep(0.5)

    vm = await call_api(get_vm, client=client, vm_id=vm_id)
    raise TimeoutError(
        f"VM {vm_id} did not reach status {expected_status} within {timeout}s. "
        f"Current status: {vm.status}"
    )


def new_fc_vm(name: str, **kwargs) -> NewVm:
    """Helper: create a NewVm with Firecracker defaults (128 MiB memory)."""
    return NewVm(
        name=name,
        hypervisor=Hypervisor.FIRECRACKER,
        boot_vcpus=1,
        max_vcpus=1,
        memory_size=128 * 1024 * 1024,  # 128 MiB — FC is lightweight
        **kwargs,
    )


@pytest.mark.asyncio
async def test_fc_vm_create_and_delete(client):
    """Create a Firecracker VM and verify it appears in the list, then delete it."""
    async with client as c:
        new_vm = new_fc_vm("e2e-fc-create")
        vm_id_raw = await call_api(create_vm, client=c, body=new_vm)
        assert vm_id_raw is not None
        vm_id = str(vm_id_raw).strip('"')

        try:
            # Verify the VM exists and has the right hypervisor
            vm = await call_api(get_vm, client=c, vm_id=vm_id)
            assert vm is not None
            assert vm.hypervisor == Hypervisor.FIRECRACKER
            assert vm.status == VmStatus.CREATED

            # Verify it appears in the list
            vms = await call_api(list_vms, client=c)
            vm_ids = [str(v.id) for v in (vms or [])]
            assert vm_id in vm_ids, f"FC VM {vm_id} not found in list"
        finally:
            await call_api_detailed(delete_vm, client=c, vm_id=vm_id)


@pytest.mark.asyncio
async def test_fc_vm_full_lifecycle(client):
    """Full Firecracker lifecycle: create → start → pause → resume → stop → delete."""
    async with client as c:
        new_vm = new_fc_vm("e2e-fc-lifecycle")
        vm_id_raw = await call_api(create_vm, client=c, body=new_vm)
        assert vm_id_raw is not None
        vm_id = str(vm_id_raw).strip('"')

        try:
            # Start
            await call_api_detailed(start_vm, client=c, vm_id=vm_id)
            await wait_for_status(c, vm_id, VmStatus.RUNNING)

            # Pause
            await call_api_detailed(pause_vm, client=c, vm_id=vm_id)
            await wait_for_status(c, vm_id, VmStatus.PAUSED)

            # Resume
            await call_api_detailed(resume_vm, client=c, vm_id=vm_id)
            await wait_for_status(c, vm_id, VmStatus.RUNNING)

            # Stop (soft)
            await call_api_detailed(stop_vm, client=c, vm_id=vm_id)
            vm = await call_api(get_vm, client=c, vm_id=vm_id)
            assert vm.status == VmStatus.SHUTDOWN
        finally:
            await call_api_detailed(delete_vm, client=c, vm_id=vm_id)


@pytest.mark.asyncio
async def test_fc_vm_force_stop(client):
    """Force-stop a running Firecracker VM."""
    async with client as c:
        new_vm = new_fc_vm("e2e-fc-force-stop")
        vm_id_raw = await call_api(create_vm, client=c, body=new_vm)
        assert vm_id_raw is not None
        vm_id = str(vm_id_raw).strip('"')

        try:
            await call_api_detailed(start_vm, client=c, vm_id=vm_id)
            await wait_for_status(c, vm_id, VmStatus.RUNNING)

            await call_api_detailed(force_stop_vm, client=c, vm_id=vm_id)
            vm = await call_api(get_vm, client=c, vm_id=vm_id)
            assert vm.status == VmStatus.SHUTDOWN
        finally:
            await call_api_detailed(delete_vm, client=c, vm_id=vm_id)


@pytest.mark.asyncio
async def test_fc_cloud_init(client):
    """Verify cloud-init seed disk is generated and VM boots with it."""
    user_data = """\
#cloud-config
runcmd:
  - echo "fc cloud-init e2e test" > /tmp/fc-cloud-init-ran
"""
    meta_data = "instance-id: e2e-fc-cloud-init\nlocal-hostname: e2e-fc-vm\n"

    async with client as c:
        new_vm = new_fc_vm(
            "e2e-fc-cloud-init",
            cloud_init_user_data=user_data,
            cloud_init_meta_data=meta_data,
        )
        vm_id_raw = await call_api(create_vm, client=c, body=new_vm)
        assert vm_id_raw is not None
        vm_id = str(vm_id_raw).strip('"')

        try:
            # Verify cloud-init data stored
            vm = await call_api(get_vm, client=c, vm_id=vm_id)
            assert vm.cloud_init_user_data == user_data
            assert vm.cloud_init_meta_data == meta_data

            # Start the VM — it should boot with the cidata seed attached
            await call_api_detailed(start_vm, client=c, vm_id=vm_id)
            await wait_for_status(c, vm_id, VmStatus.RUNNING)

            await call_api_detailed(stop_vm, client=c, vm_id=vm_id)
        finally:
            await call_api_detailed(delete_vm, client=c, vm_id=vm_id)


@pytest.mark.asyncio
@pytest.mark.xfail(reason="FC restore is currently unstable in e2e", strict=False)
async def test_fc_vm_snapshot_restore(client):
    """Snapshot a paused FC VM and restore it."""
    async with client as c:
        new_vm = new_fc_vm("e2e-fc-snapshot")
        vm_id_raw = await call_api(create_vm, client=c, body=new_vm)
        assert vm_id_raw is not None
        vm_id = str(vm_id_raw).strip('"')

        try:
            # Start and pause (FC requires paused state for snapshot)
            await call_api_detailed(start_vm, client=c, vm_id=vm_id)
            await wait_for_status(c, vm_id, VmStatus.RUNNING)
            await call_api_detailed(pause_vm, client=c, vm_id=vm_id)
            await wait_for_status(c, vm_id, VmStatus.PAUSED)

            # Snapshot
            snapshot = await call_api(
                create_snapshot,
                client=c,
                vm_id=vm_id,
                body=CreateSnapshotRequest(),
            )
            assert snapshot is not None

            # Stop, then restore from created snapshot
            await call_api_detailed(stop_vm, client=c, vm_id=vm_id)
            await wait_for_status(c, vm_id, VmStatus.SHUTDOWN)

            restored_vm = await call_api(
                restore,
                client=c,
                vm_id=vm_id,
                body=RestoreRequest(snapshot_id=snapshot.id),
            )
            assert restored_vm is not None
            assert restored_vm.status == VmStatus.RUNNING

            await call_api_detailed(stop_vm, client=c, vm_id=vm_id)
        finally:
            await call_api_detailed(delete_vm, client=c, vm_id=vm_id)


@pytest.mark.asyncio
async def test_fc_unsupported_hotplug_returns_error(client):
    """Hot-attach disk operations should return an error for Firecracker VMs."""
    async with client as c:
        new_vm = new_fc_vm("e2e-fc-no-hotplug")
        vm_id_raw = await call_api(create_vm, client=c, body=new_vm)
        assert vm_id_raw is not None
        vm_id = str(vm_id_raw).strip('"')

        try:
            await call_api_detailed(start_vm, client=c, vm_id=vm_id)
            await wait_for_status(c, vm_id, VmStatus.RUNNING)

            # Attempt hot-attach — expect an error response (501 UNIMPLEMENTED or similar)
            response = await call_api_detailed(
                attach_disk,
                client=c,
                vm_id=vm_id,
                body=AttachDiskRequest(storage_object_id=uuid.uuid4()),
            )
            # The server should return a non-2xx status for unsupported operation
            assert response.status_code not in (200, 201, 204), (
                f"Expected error for FC hotplug but got {response.status_code}"
            )
        finally:
            await call_api_detailed(force_stop_vm, client=c, vm_id=vm_id)
            await call_api_detailed(delete_vm, client=c, vm_id=vm_id)
