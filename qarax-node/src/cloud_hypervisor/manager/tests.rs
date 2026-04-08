use super::*;

use crate::rpc::node::{CpuPinning as ProtoCpuPinning, NumaPlacement as ProtoNumaPlacement};
use tempfile::TempDir;

fn base_vm_config(memory_size: i64, boot_vcpus: i32) -> VmConfig {
    VmConfig {
        cpus: Some(Box::new(CpusConfig {
            boot_vcpus,
            max_vcpus: boot_vcpus,
            topology: None,
            kvm_hyperv: None,
            max_phys_bits: None,
            affinity: None,
            features: None,
            nested: None,
        })),
        memory: Some(Box::new(MemoryConfig {
            size: memory_size,
            hotplug_size: None,
            hotplugged_size: None,
            mergeable: None,
            hotplug_method: None,
            shared: None,
            hugepages: None,
            hugepage_size: None,
            prefault: None,
            thp: None,
            zones: None,
        })),
        ..Default::default()
    }
}

#[test]
fn apply_numa_placement_sets_cpu_affinity() {
    let mut config = base_vm_config(512 * 1024 * 1024, 2);
    let placement = ProtoNumaPlacement {
        host_numa_node_ids: vec![],
        cpu_pinning: vec![
            ProtoCpuPinning {
                vcpu: 0,
                host_cpus: vec![0, 1],
            },
            ProtoCpuPinning {
                vcpu: 1,
                host_cpus: vec![2, 3],
            },
        ],
        memory_zone_ids: vec![],
    };

    VmManager::apply_numa_placement(&mut config, &placement);

    let affinity = config.cpus.as_ref().unwrap().affinity.as_ref().unwrap();
    assert_eq!(affinity.len(), 2);
    assert_eq!(affinity[0].vcpu, 0);
    assert_eq!(affinity[0].host_cpus, vec![0, 1]);
    assert_eq!(affinity[1].vcpu, 1);
    assert_eq!(affinity[1].host_cpus, vec![2, 3]);
}

#[test]
fn apply_numa_placement_creates_memory_zones() {
    let mut config = base_vm_config(1024 * 1024 * 1024, 4);
    let placement = ProtoNumaPlacement {
        host_numa_node_ids: vec![0],
        cpu_pinning: vec![],
        memory_zone_ids: vec!["zone0".to_string()],
    };

    VmManager::apply_numa_placement(&mut config, &placement);

    let memory = config.memory.as_ref().unwrap();
    // When zones are set, size must be 0
    assert_eq!(memory.size, 0);
    let zones = memory.zones.as_ref().unwrap();
    assert_eq!(zones.len(), 1);
    assert_eq!(zones[0].id, "zone0");
    assert_eq!(zones[0].host_numa_node, Some(0));
}

#[test]
fn apply_numa_placement_preserves_memory_remainder() {
    let mut config = base_vm_config(1025, 4);
    let placement = ProtoNumaPlacement {
        host_numa_node_ids: vec![0, 1],
        cpu_pinning: vec![],
        memory_zone_ids: vec!["zone0".to_string(), "zone1".to_string()],
    };

    VmManager::apply_numa_placement(&mut config, &placement);

    let memory = config.memory.as_ref().unwrap();
    assert_eq!(memory.size, 0);
    let zones = memory.zones.as_ref().unwrap();
    assert_eq!(zones.len(), 2);
    assert_eq!(zones[0].size, 513);
    assert_eq!(zones[1].size, 512);
}

#[test]
fn apply_numa_placement_sets_guest_numa_topology() {
    let mut config = base_vm_config(512 * 1024 * 1024, 2);
    let placement = ProtoNumaPlacement {
        host_numa_node_ids: vec![0],
        cpu_pinning: vec![
            ProtoCpuPinning {
                vcpu: 0,
                host_cpus: vec![0],
            },
            ProtoCpuPinning {
                vcpu: 1,
                host_cpus: vec![1],
            },
        ],
        memory_zone_ids: vec!["zone0".to_string()],
    };

    VmManager::apply_numa_placement(&mut config, &placement);

    let numa = config.numa.as_ref().unwrap();
    assert_eq!(numa.len(), 1);
    assert_eq!(numa[0].guest_numa_id, 0);
    // All vCPUs (0..boot_vcpus) assigned to guest node 0
    assert_eq!(numa[0].cpus, Some(vec![0, 1]));
    assert_eq!(numa[0].memory_zones, Some(vec!["zone0".to_string()]));
}

#[test]
fn apply_numa_placement_empty_is_noop() {
    let mut config = base_vm_config(512 * 1024 * 1024, 1);
    let original_size = config.memory.as_ref().unwrap().size;
    let placement = ProtoNumaPlacement {
        host_numa_node_ids: vec![],
        cpu_pinning: vec![],
        memory_zone_ids: vec![],
    };

    VmManager::apply_numa_placement(&mut config, &placement);

    // Nothing should have changed
    assert!(config.cpus.as_ref().unwrap().affinity.is_none());
    assert!(config.memory.as_ref().unwrap().zones.is_none());
    assert_eq!(config.memory.as_ref().unwrap().size, original_size);
    assert!(config.numa.is_none());
}

#[test]
fn resolve_vsock_config_sets_defaults() {
    let runtime_dir = TempDir::new().unwrap();
    let manager = VmManager::new(runtime_dir.path(), "/bin/true");
    let mut vsock = ProtoVsockConfig::default();

    manager.resolve_vsock_config("test-vm", &mut vsock);

    assert!(vsock.cid.is_some());
    assert_eq!(
        vsock.socket.as_deref(),
        Some(
            runtime_dir
                .path()
                .join("test-vm.vsock")
                .to_string_lossy()
                .as_ref()
        )
    );
}

#[tokio::test]
async fn exec_vm_rejects_empty_command() {
    let runtime_dir = TempDir::new().unwrap();
    let manager = VmManager::new(runtime_dir.path(), "/bin/true");

    let err = manager
        .exec_vm("test-vm", Vec::new(), None)
        .await
        .unwrap_err();

    assert!(matches!(err, VmManagerError::ExecInvalid(_)));
}

#[test]
fn build_guest_exec_request_is_newline_framed_json() {
    let payload =
        VmManager::build_guest_exec_request(vec!["/bin/echo".into(), "hello".into()], Some(5))
            .unwrap();

    assert_eq!(payload.last(), Some(&b'\n'));

    let body = std::str::from_utf8(&payload[..payload.len() - 1]).unwrap();
    let json: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(json["command"], serde_json::json!(["/bin/echo", "hello"]));
    assert_eq!(json["timeout_secs"], 5);
}
