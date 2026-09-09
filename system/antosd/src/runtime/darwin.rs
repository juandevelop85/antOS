//! macOS (Darwin) platform runtime implementation (T22.4).
//!
//! Integrates Apple Seatbelt (`sandbox-exec`) for filesystem confinement,
//! process-watchdog–based resource monitoring (no cgroups on Darwin), and
//! simulated eBPF telemetry. Reports Hypervisor.framework availability for
//! hardware-assisted virtualisation.

use antos_protocol::{CapabilityFidelity, RuntimeCapabilitiesMatrix};

use super::{PlatformRuntime, ProcessWatchdogMonitor, QuotaMonitor, SimulatedTelemetry, TelemetryProvider};
use crate::sandbox::{self, Sandbox};

/// macOS runtime: Seatbelt sandbox, process watchdog quotas, simulated telemetry.
pub struct DarwinRuntime {
    seatbelt_available: bool,
    hypervisor_available: bool,
}

impl DarwinRuntime {
    pub fn new() -> Self {
        Self {
            seatbelt_available: std::path::Path::new(sandbox::seatbelt::SANDBOX_EXEC).exists(),
            hypervisor_available: probe_hypervisor_framework(),
        }
    }
}

impl PlatformRuntime for DarwinRuntime {
    fn name(&self) -> &'static str {
        "Darwin"
    }

    fn sandbox_provider(&self) -> Box<dyn Sandbox> {
        if self.seatbelt_available {
            Box::new(sandbox::seatbelt::Seatbelt)
        } else {
            Box::new(sandbox::SinRecinto)
        }
    }

    fn quota_monitor(&self) -> Box<dyn QuotaMonitor> {
        Box::new(ProcessWatchdogMonitor)
    }

    fn telemetry_provider(&self) -> Box<dyn TelemetryProvider> {
        Box::new(SimulatedTelemetry)
    }

    fn capabilities_matrix(&self) -> RuntimeCapabilitiesMatrix {
        RuntimeCapabilitiesMatrix {
            landlock_lsm: CapabilityFidelity::Unsupported,
            seatbelt: if self.seatbelt_available {
                CapabilityFidelity::Native
            } else {
                CapabilityFidelity::Unsupported
            },
            cgroups_v2: CapabilityFidelity::Unsupported,
            ebpf_supervision: CapabilityFidelity::Simulated,
            kvm_hypervisor: if self.hypervisor_available {
                CapabilityFidelity::Native
            } else {
                CapabilityFidelity::Unsupported
            },
            wayland_desktop: probe_wayland_fidelity(),
        }
    }
}

/// Checks for Hypervisor.framework availability on macOS.
///
/// The framework exists on all Apple Silicon and recent Intel Macs with SIP
/// enabled. We probe by checking the framework bundle path.
fn probe_hypervisor_framework() -> bool {
    std::path::Path::new("/System/Library/Frameworks/Hypervisor.framework").exists()
}

/// Determines the Wayland desktop fidelity on macOS.
///
/// Wayland is not native on macOS. If `$WAYLAND_DISPLAY` is set (e.g.
/// through XWayland or a bridge), we report it as emulated; otherwise
/// it is reported as a headless / web-bridge fallback.
fn probe_wayland_fidelity() -> CapabilityFidelity {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        CapabilityFidelity::Emulated
    } else {
        CapabilityFidelity::Simulated
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn darwin_runtime_name() {
        let rt = DarwinRuntime::new();
        assert_eq!(rt.name(), "Darwin");
    }

    #[test]
    fn darwin_matrix_landlock_unsupported() {
        let rt = DarwinRuntime::new();
        let m = rt.capabilities_matrix();
        assert_eq!(m.landlock_lsm, CapabilityFidelity::Unsupported);
        assert_eq!(m.cgroups_v2, CapabilityFidelity::Unsupported);
    }

    #[test]
    fn darwin_seatbelt_is_native_when_binary_exists() {
        let rt = DarwinRuntime::new();
        let m = rt.capabilities_matrix();
        // On a real macOS machine, sandbox-exec should exist.
        if std::path::Path::new("/usr/bin/sandbox-exec").exists() {
            assert_eq!(m.seatbelt, CapabilityFidelity::Native);
        }
    }

    #[test]
    fn darwin_sandbox_provider_returns_valid_sandbox() {
        let rt = DarwinRuntime::new();
        let sb = rt.sandbox_provider();
        assert!(!sb.name().is_empty());
    }

    #[test]
    fn darwin_quota_monitor_is_watchdog() {
        let rt = DarwinRuntime::new();
        let qm = rt.quota_monitor();
        assert_eq!(qm.name(), "process-watchdog");
        assert!(!qm.enforces_hard_limits());
    }

    #[test]
    fn darwin_telemetry_is_simulated() {
        let rt = DarwinRuntime::new();
        let tp = rt.telemetry_provider();
        assert_eq!(tp.name(), "simulated");
        assert!(!tp.has_kernel_probes());
    }
}
