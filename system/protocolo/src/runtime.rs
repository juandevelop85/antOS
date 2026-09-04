//! Platform runtime abstraction types (T22.4).
//!
//! Defines the IPC contract for the runtime capabilities matrix, fidelity
//! levels, and the full diagnostic report produced by `antos runtime info`
//! and consumed by `antos doctor`.

use serde::{Deserialize, Serialize};

/// How faithfully a subsystem is supported on the current host.
///
/// The four levels avoid the ambiguity of a simple boolean: callers know
/// exactly whether a feature is hardware-backed, software-emulated, faked
/// for testing, or entirely absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityFidelity {
    /// Full kernel / hardware support — the real thing.
    Native,
    /// Functional substitute with weaker guarantees (e.g. process watchdog
    /// instead of cgroups v2).
    Emulated,
    /// Stub that records events but cannot enforce anything. Useful for
    /// development on unsupported platforms.
    Simulated,
    /// Not available at all on this platform.
    Unsupported,
}

impl CapabilityFidelity {
    /// Human-readable label for terminal output.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Native => "Native",
            Self::Emulated => "Emulated",
            Self::Simulated => "Simulated",
            Self::Unsupported => "Unsupported",
        }
    }

    /// Whether the capability provides real enforcement (not just logging).
    pub fn is_enforcing(&self) -> bool {
        matches!(self, Self::Native | Self::Emulated)
    }
}

/// Typed matrix describing the fidelity of every platform-dependent subsystem
/// on the current host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCapabilitiesMatrix {
    /// Landlock LSM (Linux kernel sandbox).
    pub landlock_lsm: CapabilityFidelity,
    /// Apple Seatbelt (`sandbox-exec`).
    pub seatbelt: CapabilityFidelity,
    /// Cgroups v2 resource control.
    pub cgroups_v2: CapabilityFidelity,
    /// eBPF LSM supervision probes.
    pub ebpf_supervision: CapabilityFidelity,
    /// Hardware-assisted virtualisation (KVM / Hypervisor.framework).
    pub kvm_hypervisor: CapabilityFidelity,
    /// Wayland desktop compositor.
    pub wayland_desktop: CapabilityFidelity,
}

impl Default for RuntimeCapabilitiesMatrix {
    fn default() -> Self {
        Self {
            landlock_lsm: CapabilityFidelity::Unsupported,
            seatbelt: CapabilityFidelity::Unsupported,
            cgroups_v2: CapabilityFidelity::Unsupported,
            ebpf_supervision: CapabilityFidelity::Unsupported,
            kvm_hypervisor: CapabilityFidelity::Unsupported,
            wayland_desktop: CapabilityFidelity::Unsupported,
        }
    }
}

/// Full diagnostic snapshot of the runtime environment.
///
/// Produced by `antos runtime info` and used by `antos doctor` for the
/// enhanced capabilities report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInfo {
    /// Human-readable platform name (e.g. "Linux", "Darwin", "antOS BareMetal").
    pub platform_name: String,
    /// Kernel or OS version string.
    pub kernel_version: String,
    /// CPU architecture (e.g. "x86_64", "aarch64").
    pub arch: String,
    /// Per-subsystem capability fidelity.
    pub capabilities: RuntimeCapabilitiesMatrix,
    /// Whether an Ollama instance is reachable.
    pub ollama_available: bool,
    /// Local LLM endpoint URL if detected.
    pub local_llm_endpoint: Option<String>,
}
