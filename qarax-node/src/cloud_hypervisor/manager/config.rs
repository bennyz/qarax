use super::*;

impl VmManager {
    /// Convert proto VmConfig to SDK VmConfig
    pub(super) fn proto_to_sdk_config(
        &self,
        config: &ProtoVmConfig,
    ) -> Result<VmConfig, VmManagerError> {
        // Build payload config (required)
        let payload = config
            .payload
            .as_ref()
            .map(Self::proto_payload_to_sdk)
            .unwrap_or_else(|| PayloadConfig {
                firmware: None,
                kernel: None,
                cmdline: None,
                initramfs: None,
                igvm: None,
                host_data: None,
            });

        let mut sdk_config = VmConfig::new(payload);

        // CPU config
        if let Some(cpus) = &config.cpus {
            sdk_config.cpus = Some(Box::new(Self::proto_cpus_to_sdk(cpus)));
        }

        // Memory config
        if let Some(memory) = &config.memory {
            sdk_config.memory = Some(Box::new(Self::proto_memory_to_sdk(memory)));
        }

        // Disks
        if !config.disks.is_empty() {
            sdk_config.disks = Some(config.disks.iter().map(Self::proto_disk_to_sdk).collect());
        }

        // Networks
        if !config.networks.is_empty() {
            sdk_config.net = Some(config.networks.iter().map(Self::proto_net_to_sdk).collect());
        }

        // RNG
        if let Some(rng) = &config.rng {
            sdk_config.rng = Some(Box::new(Self::proto_rng_to_sdk(rng)));
        }

        // Serial console
        if let Some(serial) = &config.serial {
            sdk_config.serial = Some(Box::new(Self::proto_console_to_sdk(serial)));
        }

        // Console
        if let Some(console) = &config.console {
            sdk_config.console = Some(Box::new(Self::proto_console_to_sdk(console)));
        }

        // VFIO devices (GPU passthrough)
        if !config.devices.is_empty() {
            sdk_config.devices = Some(
                config
                    .devices
                    .iter()
                    .map(Self::proto_vfio_device_to_sdk)
                    .collect(),
            );
        }

        // Virtio-vsock guest-agent channel
        if let Some(vsock) = &config.vsock {
            sdk_config.vsock = Some(Box::new(Self::proto_vsock_to_sdk(vsock)?));
        }

        // NUMA placement (optional)
        if let Some(placement) = &config.numa_placement {
            Self::apply_numa_placement(&mut sdk_config, placement);
        }

        Ok(sdk_config)
    }

    /// Apply NUMA placement constraints to the SDK VmConfig.
    ///
    /// Sets up:
    /// - `cpus.affinity`: per-vCPU host CPU pinning
    /// - `memory.zones`: one zone per NUMA node pinned to the correct host node
    /// - `numa`: single guest NUMA node 0 owning all vCPUs and memory zones
    pub(super) fn apply_numa_placement(sdk_config: &mut VmConfig, placement: &ProtoNumaPlacement) {
        // CPU affinity
        if !placement.cpu_pinning.is_empty() {
            let cpus = sdk_config.cpus.get_or_insert_with(|| {
                Box::new(CpusConfig {
                    boot_vcpus: 1,
                    max_vcpus: 1,
                    topology: None,
                    kvm_hyperv: None,
                    max_phys_bits: None,
                    affinity: None,
                    features: None,
                    nested: None,
                })
            });
            cpus.affinity = Some(
                placement
                    .cpu_pinning
                    .iter()
                    .map(|p| CpuAffinity {
                        vcpu: p.vcpu,
                        host_cpus: p.host_cpus.clone(),
                    })
                    .collect(),
            );
        }

        // Memory zones pinned to the NUMA node(s)
        if !placement.host_numa_node_ids.is_empty() {
            let memory = sdk_config.memory.get_or_insert_with(|| {
                Box::new(MemoryConfig {
                    size: 0,
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
                })
            });

            let total_size = memory.size;
            let n = placement.host_numa_node_ids.len() as i64;
            let (zone_size, remainder) = if n > 0 {
                (total_size / n, total_size % n)
            } else {
                (total_size, 0)
            };

            // Preserve memory flags from base config
            let shared = memory.shared;
            let hugepages = memory.hugepages;
            let hugepage_size = memory.hugepage_size;

            let zones: Vec<MemoryZoneConfig> = placement
                .host_numa_node_ids
                .iter()
                .enumerate()
                .map(|(i, &node_id)| MemoryZoneConfig {
                    id: format!("zone{}", i),
                    size: if i == 0 {
                        zone_size + remainder
                    } else {
                        zone_size
                    },
                    host_numa_node: Some(node_id),
                    shared,
                    hugepages,
                    hugepage_size,
                    ..Default::default()
                })
                .collect();

            // When zones are used, CH expects memory.size = 0 and zones provide the total.
            memory.size = 0;
            memory.zones = Some(zones);
        }

        // Guest NUMA topology: advertise a single node 0 containing all vCPUs
        if !placement.memory_zone_ids.is_empty() || !placement.cpu_pinning.is_empty() {
            let boot_vcpus = sdk_config.cpus.as_ref().map(|c| c.boot_vcpus).unwrap_or(1);
            let zone_ids = placement.memory_zone_ids.clone();

            sdk_config.numa = Some(vec![NumaConfig {
                guest_numa_id: 0,
                cpus: Some((0..boot_vcpus).collect()),
                memory_zones: if zone_ids.is_empty() {
                    None
                } else {
                    Some(zone_ids)
                },
                distances: None,
                pci_segments: None,
                device_id: None,
            }]);
        }
    }

    pub(super) fn proto_cpus_to_sdk(cpus: &ProtoCpusConfig) -> CpusConfig {
        CpusConfig {
            boot_vcpus: cpus.boot_vcpus,
            max_vcpus: cpus.max_vcpus,
            topology: cpus
                .topology
                .as_ref()
                .map(|t| Box::new(Self::proto_topology_to_sdk(t))),
            kvm_hyperv: cpus.kvm_hyperv,
            max_phys_bits: cpus.max_phys_bits,
            affinity: None,
            features: None,
            nested: None,
        }
    }

    fn proto_topology_to_sdk(topology: &ProtoCpuTopology) -> models::CpuTopology {
        models::CpuTopology {
            threads_per_core: topology.threads_per_core,
            cores_per_die: topology.cores_per_die,
            dies_per_package: topology.dies_per_package,
            packages: topology.packages,
        }
    }

    pub(super) fn proto_memory_to_sdk(memory: &ProtoMemoryConfig) -> MemoryConfig {
        MemoryConfig {
            size: memory.size,
            hotplug_size: memory.hotplug_size,
            hotplugged_size: None,
            mergeable: memory.mergeable,
            hotplug_method: None,
            shared: memory.shared,
            hugepages: memory.hugepages,
            hugepage_size: memory.hugepage_size,
            prefault: memory.prefault,
            thp: memory.thp,
            zones: None,
        }
    }

    fn proto_payload_to_sdk(payload: &ProtoPayloadConfig) -> PayloadConfig {
        PayloadConfig {
            firmware: payload.firmware.clone(),
            kernel: payload.kernel.clone(),
            cmdline: payload.cmdline.clone(),
            initramfs: payload.initramfs.clone(),
            igvm: None,
            host_data: None,
        }
    }

    fn is_qcow2(path: &str) -> bool {
        use std::io::Read;
        let mut f = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut magic = [0u8; 4];
        f.read_exact(&mut magic)
            .map(|_| magic == [0x51, 0x46, 0x49, 0xfb])
            .unwrap_or(false)
    }

    pub(super) fn proto_disk_to_sdk(disk: &ProtoDiskConfig) -> models::DiskConfig {
        models::DiskConfig {
            path: disk.path.clone(),
            readonly: disk.readonly,
            direct: disk.direct,
            iommu: None,
            num_queues: disk.num_queues,
            queue_size: disk.queue_size,
            vhost_user: disk.vhost_user,
            vhost_socket: disk.vhost_socket.clone(),
            rate_limiter_config: disk
                .rate_limiter
                .as_ref()
                .map(|r| Box::new(Self::proto_rate_limiter_to_sdk(r))),
            pci_segment: disk.pci_segment,
            id: Some(disk.id.clone()),
            serial: disk.serial.clone(),
            rate_limit_group: disk.rate_limit_group.clone(),
            queue_affinity: None,
            backing_files: None,
            // Detect qcow2 by magic bytes (QFI\xfb); otherwise force Raw to
            // prevent CH from autodetecting and disabling sector 0 writes,
            // which breaks ext4 superblock updates on raw images.
            image_type: Some(
                if disk.path.as_deref().map(Self::is_qcow2).unwrap_or(false) {
                    ImageType::Qcow2
                } else {
                    ImageType::Raw
                },
            ),
            sparse: None,
        }
    }

    pub(super) fn proto_net_to_sdk(net: &ProtoNetConfig) -> models::NetConfig {
        models::NetConfig {
            tap: net.tap.clone(),
            ip: net.ip.clone(),
            mask: net.mask.clone(),
            mac: net.mac.clone(),
            host_mac: net.host_mac.clone(),
            mtu: net.mtu,
            iommu: net.iommu,
            num_queues: net.num_queues,
            queue_size: net.queue_size,
            vhost_user: net.vhost_user,
            vhost_socket: net.vhost_socket.clone(),
            vhost_mode: net.vhost_mode.map(|m| {
                if m == ProtoVhostMode::Server as i32 {
                    "Server".to_string()
                } else {
                    "Client".to_string()
                }
            }),
            id: Some(net.id.clone()),
            pci_segment: net.pci_segment,
            rate_limiter_config: net
                .rate_limiter
                .as_ref()
                .map(|r| Box::new(Self::proto_rate_limiter_to_sdk(r))),
            offload_tso: net.offload_tso,
            offload_ufo: net.offload_ufo,
            offload_csum: net.offload_csum,
        }
    }

    fn proto_rng_to_sdk(rng: &ProtoRngConfig) -> models::RngConfig {
        models::RngConfig {
            src: rng.src.clone(),
            iommu: rng.iommu,
        }
    }

    pub(super) fn proto_console_to_sdk(console: &ProtoConsoleConfig) -> models::ConsoleConfig {
        let mode = match ProtoConsoleMode::try_from(console.mode) {
            Ok(ProtoConsoleMode::Off) => ConsoleMode::Off,
            Ok(ProtoConsoleMode::Pty) => ConsoleMode::Pty,
            Ok(ProtoConsoleMode::Tty) => ConsoleMode::Tty,
            Ok(ProtoConsoleMode::File) => ConsoleMode::File,
            Ok(ProtoConsoleMode::Socket) => ConsoleMode::Socket,
            Ok(ProtoConsoleMode::Null) => ConsoleMode::Null,
            _ => ConsoleMode::Null,
        };

        models::ConsoleConfig {
            file: console.file.clone(),
            socket: console.socket.clone(),
            mode,
            iommu: console.iommu,
        }
    }

    pub(super) fn proto_vsock_to_sdk(
        vsock: &ProtoVsockConfig,
    ) -> Result<SdkVsockConfig, VmManagerError> {
        let cid = vsock
            .cid
            .ok_or_else(|| VmManagerError::InvalidConfig("vsock.cid is required".into()))?;
        let socket = vsock
            .socket
            .clone()
            .ok_or_else(|| VmManagerError::InvalidConfig("vsock.socket is required".into()))?;

        Ok(SdkVsockConfig {
            cid,
            socket,
            iommu: vsock.iommu,
            pci_segment: vsock.pci_segment,
            id: vsock.id.clone(),
        })
    }

    pub(super) fn proto_vfio_device_to_sdk(device: &ProtoVfioDeviceConfig) -> models::DeviceConfig {
        models::DeviceConfig {
            path: device.path.clone(),
            iommu: device.iommu,
            pci_segment: device.pci_segment,
            id: Some(device.id.clone()),
            x_nv_gpudirect_clique: None,
        }
    }

    fn proto_rate_limiter_to_sdk(
        rate_limiter: &ProtoRateLimiterConfig,
    ) -> models::RateLimiterConfig {
        models::RateLimiterConfig {
            bandwidth: rate_limiter
                .bandwidth
                .as_ref()
                .map(|b| Box::new(Self::proto_token_bucket_to_sdk(b))),
            ops: rate_limiter
                .ops
                .as_ref()
                .map(|o| Box::new(Self::proto_token_bucket_to_sdk(o))),
        }
    }

    fn proto_token_bucket_to_sdk(bucket: &ProtoTokenBucket) -> models::TokenBucket {
        models::TokenBucket {
            size: bucket.size,
            refill_time: bucket.refill_time,
            one_time_burst: bucket.one_time_burst,
        }
    }
}
