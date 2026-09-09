//! Platform Runtime Abstraction Layer (T22.4).
//!
//! Replaces the scattered `cfg(target_os)` conditionals throughout the daemon
//! with a single trait that each supported platform implements. The factory
//! function `detect()` returns the correct runtime for the current host,
//! and every subsystem (sandbox, quotas, telemetry) is accessed through it.
//!
//! The three concrete runtimes live in submodules:
//! - `linux`      — Landlock, cgroups v2, eBPF, KVM.
//! - `darwin`     — Seatbelt, process watchdog, Hypervisor.framework.
//! - `bare_metal` — Stub for future antOS kernel syscalls.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod darwin;
pub mod bare_metal;

use antos_protocol::{CapabilityFidelity, RuntimeCapabilitiesMatrix, RuntimeInfo};

// ──────────────────────────────────────────────────── sub-trait: QuotaMonitor

/// Abstraction over resource quota enforcement.
///
/// On Linux this is backed by cgroups v2; on macOS (and everywhere else) it
/// falls back to the process watchdog that polls RSS via `ps`.
pub trait QuotaMonitor: Send + Sync {
    fn name(&self) -> &'static str;

    /// Whether the monitor can enforce hard limits (OOM-kill, CPU throttle)
    /// as opposed to merely observing.
    fn enforces_hard_limits(&self) -> bool;

    /// Fidelity level of this quota mechanism.
    fn fidelity(&self) -> CapabilityFidelity;
}

// ─────────────────────────────────────────────── sub-trait: TelemetryProvider

/// Abstraction over kernel-level security telemetry.
///
/// On Linux this is backed by real eBPF LSM probes; on macOS a simulated
/// provider records events without kernel enforcement.
pub trait TelemetryProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Whether the provider has real kernel probes attached.
    fn has_kernel_probes(&self) -> bool;

    /// Fidelity level of this telemetry mechanism.
    fn fidelity(&self) -> CapabilityFidelity;
}

// ────────────────────────────────────────────── core trait: PlatformRuntime

/// The central abstraction that each supported platform implements.
///
/// Every platform-dependent subsystem in antOS is accessed through this
/// trait, eliminating ad-hoc `cfg` blocks and making the fidelity of each
/// capability explicit and queryable.
pub trait PlatformRuntime: Send + Sync {
    /// Human-readable platform name (e.g. "Linux", "Darwin").
    fn name(&self) -> &'static str;

    /// The sandbox engine for this platform (Landlock, Seatbelt, or none).
    fn sandbox_provider(&self) -> Box<dyn crate::sandbox::Sandbox>;

    /// Resource quota enforcement for sandboxed agent tasks.
    fn quota_monitor(&self) -> Box<dyn QuotaMonitor>;

    /// Kernel-level security telemetry (eBPF or simulated).
    fn telemetry_provider(&self) -> Box<dyn TelemetryProvider>;

    /// Typed matrix of every subsystem's fidelity on this host.
    fn capabilities_matrix(&self) -> RuntimeCapabilitiesMatrix;

    /// Full diagnostic snapshot for `antos runtime info`.
    fn runtime_info(&self, ctx: &crate::ctx::Ctx) -> RuntimeInfo {
        let kernel_version = std::process::Command::new("uname")
            .arg("-r")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let arch = std::env::consts::ARCH.to_string();

        RuntimeInfo {
            platform_name: self.name().to_string(),
            kernel_version,
            arch,
            capabilities: self.capabilities_matrix(),
            ollama_available: ctx.local_llm.ollama_available,
            local_llm_endpoint: ctx.local_llm.preferred_local_endpoint.clone(),
        }
    }
}

// ───────────────────────────────────────────────────── shared implementations

/// Quota monitor backed by the process watchdog (polls RSS via `ps`).
/// Used on macOS and as a fallback on Linux when cgroups v2 are unavailable.
pub struct ProcessWatchdogMonitor;

impl QuotaMonitor for ProcessWatchdogMonitor {
    fn name(&self) -> &'static str {
        "process-watchdog"
    }

    fn enforces_hard_limits(&self) -> bool {
        false
    }

    fn fidelity(&self) -> CapabilityFidelity {
        CapabilityFidelity::Emulated
    }
}

/// Telemetry provider that records events in-memory without kernel probes.
/// Used on platforms where eBPF is not available (macOS, bare-metal).
pub struct SimulatedTelemetry;

impl TelemetryProvider for SimulatedTelemetry {
    fn name(&self) -> &'static str {
        "simulated"
    }

    fn has_kernel_probes(&self) -> bool {
        false
    }

    fn fidelity(&self) -> CapabilityFidelity {
        CapabilityFidelity::Simulated
    }
}

// ──────────────────────────────────────────────────────── factory: detect()

/// Returns the `PlatformRuntime` implementation matching the current host.
///
/// This is the single entry point that replaces all scattered `cfg(target_os)`
/// conditionals throughout the daemon.
pub fn detect() -> Box<dyn PlatformRuntime> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxRuntime::new())
    }

    #[cfg(target_os = "macos")]
    {
        Box::new(darwin::DarwinRuntime::new())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Box::new(bare_metal::BareMetalRuntime)
    }
}

// ──────────────────────────────────────────────────────────────────── tests

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn detect_returns_a_runtime_for_this_platform() {
        let rt = detect();
        assert!(!rt.name().is_empty(), "runtime name must not be empty");
    }

    #[test]
    fn capabilities_matrix_is_consistent() {
        let rt = detect();
        let matrix = rt.capabilities_matrix();

        // On macOS, seatbelt should be native and landlock unsupported.
        #[cfg(target_os = "macos")]
        {
            assert_eq!(matrix.seatbelt, CapabilityFidelity::Native);
            assert_eq!(matrix.landlock_lsm, CapabilityFidelity::Unsupported);
            assert_eq!(matrix.cgroups_v2, CapabilityFidelity::Unsupported);
        }

        // On Linux, landlock should be native or unsupported (never seatbelt).
        #[cfg(target_os = "linux")]
        {
            assert_eq!(matrix.seatbelt, CapabilityFidelity::Unsupported);
            assert!(
                matrix.landlock_lsm == CapabilityFidelity::Native
                    || matrix.landlock_lsm == CapabilityFidelity::Unsupported,
                "landlock should be native or unsupported on Linux"
            );
        }
    }

    #[test]
    fn sandbox_provider_is_functional() {
        let rt = detect();
        let sb = rt.sandbox_provider();
        // Must return a non-empty name.
        assert!(!sb.name().is_empty());
    }

    #[test]
    fn quota_monitor_is_functional() {
        let rt = detect();
        let qm = rt.quota_monitor();
        assert!(!qm.name().is_empty());
    }

    #[test]
    fn telemetry_provider_is_functional() {
        let rt = detect();
        let tp = rt.telemetry_provider();
        assert!(!tp.name().is_empty());
    }

    #[test]
    fn capability_fidelity_serde_roundtrip() {
        for fidelity in [
            CapabilityFidelity::Native,
            CapabilityFidelity::Emulated,
            CapabilityFidelity::Simulated,
            CapabilityFidelity::Unsupported,
        ] {
            let json = serde_json::to_string(&fidelity).unwrap();
            let back: CapabilityFidelity = serde_json::from_str(&json).unwrap();
            assert_eq!(fidelity, back);
        }
    }

    #[test]
    fn runtime_capabilities_matrix_serde_roundtrip() {
        let matrix = RuntimeCapabilitiesMatrix::default();
        let json = serde_json::to_string(&matrix).unwrap();
        let back: RuntimeCapabilitiesMatrix = serde_json::from_str(&json).unwrap();
        assert_eq!(matrix, back);
    }
}
