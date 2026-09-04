//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};

// ============================================================================
// Ephemeral MicroVMs and Hypervisor Isolation (KVM / Cloud-Hypervisor)
// ============================================================================

// ----------------------------------------------------------- microvms (T16.1)

/// Configuration to instantiate an ephemeral microVM with hardware isolation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrovmConfig {
    pub vm_id: String,
    pub vcpu_count: u8,
    pub memory_mb: u32,
    pub kernel_image: String,
    pub initrd_image: Option<String>,
    pub overlay_disk: Option<String>,
    pub vsock_port: u32,
    pub command: Option<String>,
}

impl Default for MicrovmConfig {
    fn default() -> Self {
        Self {
            vm_id: "vm-default".into(),
            vcpu_count: 2,
            memory_mb: 512,
            kernel_image: "/boot/antos-vmlinuz".into(),
            initrd_image: None,
            overlay_disk: None,
            vsock_port: 5252,
            command: None,
        }
    }
}

/// Health and support status of KVM / Cloud-Hypervisor hypervisor engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrovmStatus {
    pub kvm_available: bool,
    pub hypervisor_engine: String,
    pub active_vms_count: usize,
    pub total_memory_allocated_mb: u32,
    pub vsock_supported: bool,
    pub kernel_version: String,
}

/// Active or registered ephemeral microVM instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrovmInstance {
    pub id: String,
    pub pid: u32,
    pub vcpus: u8,
    pub memory_mb: u32,
    pub vsock_port: u32,
    pub status: String,
    pub created_at: String,
    pub command: Option<String>,
}

/// Result of executing a command inside an ephemeral microVM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrovmExecResult {
    pub vm_id: String,
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub success: bool,
}

