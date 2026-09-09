//! Bare-metal platform runtime stub (T22.4).
//!
//! Placeholder for the future antOS kernel. When running on bare metal, none
//! of the hosted-OS subsystems (Landlock, Seatbelt, cgroups, eBPF) exist;
//! instead the kernel will expose its own syscall-based isolation primitives.
//!
//! For now this runtime reports everything as `Unsupported` and uses the
//! no-op sandbox and simulated telemetry providers.

use antos_protocol::{CapabilityFidelity, RuntimeCapabilitiesMatrix};

use super::{
    PlatformRuntime, ProcessWatchdogMonitor, QuotaMonitor, SimulatedTelemetry, TelemetryProvider,
};
use crate::sandbox::{self, Sandbox};

/// Bare-metal runtime for the antOS kernel (future).
pub struct BareMetalRuntime;

impl PlatformRuntime for BareMetalRuntime {
    fn name(&self) -> &'static str {
        "antOS BareMetal"
    }

    fn sandbox_provider(&self) -> Box<dyn Sandbox> {
        Box::new(sandbox::SinRecinto)
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
            seatbelt: CapabilityFidelity::Unsupported,
            cgroups_v2: CapabilityFidelity::Unsupported,
            ebpf_supervision: CapabilityFidelity::Unsupported,
            kvm_hypervisor: CapabilityFidelity::Unsupported,
            wayland_desktop: CapabilityFidelity::Unsupported,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn bare_metal_runtime_name() {
        assert_eq!(BareMetalRuntime.name(), "antOS BareMetal");
    }

    #[test]
    fn bare_metal_matrix_all_unsupported() {
        let m = BareMetalRuntime.capabilities_matrix();
        assert_eq!(m.landlock_lsm, CapabilityFidelity::Unsupported);
        assert_eq!(m.seatbelt, CapabilityFidelity::Unsupported);
        assert_eq!(m.cgroups_v2, CapabilityFidelity::Unsupported);
        assert_eq!(m.ebpf_supervision, CapabilityFidelity::Unsupported);
        assert_eq!(m.kvm_hypervisor, CapabilityFidelity::Unsupported);
        assert_eq!(m.wayland_desktop, CapabilityFidelity::Unsupported);
    }

    #[test]
    fn bare_metal_sandbox_is_none() {
        let sb = BareMetalRuntime.sandbox_provider();
        // SinRecinto has a specific name.
        assert!(!sb.name().is_empty());
    }
}
