//! antOS MicroVM Lifecycle Registry (T16.1).
//!
//! ## Estado real del aislamiento (T31.4)
//!
//! Este módulo gestiona un **registro** de instancias de "microVM": un
//! fichero JSON con id, PID, vCPUs y memoria declarados. No lanza ningún
//! hipervisor, ni KVM, ni Cloud-Hypervisor, ni Firecracker, ni un canal
//! `vsock` real — `spawn_vm` fabrica un PID (`std::process::id() + 1000 + …`)
//! y jamás abre `/dev/kvm` para nada más que consultarlo en `get_status`.
//! `exec_vm` corre el comando **en el anfitrión**, no dentro de una VM
//! aislada: pasa por `sandbox::run` con la política más restrictiva posible
//! (ver [`exec_policy`]), que en Linux confina de verdad con Landlock y en
//! macOS con Seatbelt — la misma frontera que usa cualquier otra capacidad
//! del demonio, ni mejor ni peor. Cuando esta plataforma no ofrece ningún
//! recinto (`sandbox::SinRecinto`), el comando corre sin confinar y
//! `get_status().hypervisor_engine` lo sigue etiquetando como emulado.
//!
//! Se documenta así, en vez de prometer una frontera de hipervisor que no
//! existe, porque un consumidor —el autopilot, un rol de antFlow, un
//! plugin— que confiara en esa promesa estaría ejecutando sin recinto real.
//! Implementar aislamiento real por hipervisor (lanzar el binario de la VM y
//! hablar por `vsock`) queda fuera del alcance de este módulo tal como está
//! hoy; si se aborda, es un ticket propio, no una corrección puntual.

use crate::exec::Change;
use crate::sandbox::{self, Policy};
use anyhow::{bail, Context, Result};
use antos_protocol::{MicrovmConfig, MicrovmExecResult, MicrovmInstance, MicrovmStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Result of running a shell command inside the sandboxed executor process
/// (T31.4). Serialized to JSON and carried back through `sandbox::run`'s
/// plain `Vec<String>` output channel, since that protocol wasn't designed
/// to carry structured exec results — see `Change::HostShellExec`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostExecOutcome {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// The most restrictive policy this module ever grants: no declared writes,
/// no declared reads beyond the sandbox's own defaults, no network, no
/// access to secrets. An ad hoc `vm exec` command carries no capability
/// declaration to derive a tighter or looser radius from — the safe default
/// is to assume it needs nothing beyond running and producing output, and
/// require an explicit, separate capability grant for anything more (T31.4).
pub fn exec_policy() -> Policy {
    Policy::default()
}

/// Actually runs `command` via `sh -c` (T31.4).
///
/// This is the one legitimate `sh -c` in this module — a MicroVM `exec` is
/// fundamentally "run this shell command", the same kind of request `ci.rs`
/// serves for its pipeline stages. What changed is *where* it's allowed to
/// run: this function must only ever be invoked from inside the sandboxed
/// executor process, reached through `Change::HostShellExec` +
/// `sandbox::run` — never directly from `exec_vm` in the broker process,
/// which would run it unconfined.
pub fn run_host_shell_command(command: &str) -> HostExecOutcome {
    if command.trim().is_empty() {
        return HostExecOutcome { exit_code: 0, stdout: String::new(), stderr: String::new() };
    }

    match std::process::Command::new("sh").arg("-c").arg(command).output() {
        Ok(output) => HostExecOutcome {
            exit_code: output.status.code().unwrap_or(0),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
        Err(e) => HostExecOutcome {
            exit_code: -1,
            stdout: String::new(),
            stderr: format!("failed to execute host shell command: {e}"),
        },
    }
}

pub struct MicrovmManager;

impl MicrovmManager {
    /// Detects KVM availability and hypervisor capabilities on the host.
    pub fn get_status(state_dir: &Path) -> Result<MicrovmStatus> {
        let kvm_path = Path::new("/dev/kvm");
        let kvm_available = kvm_path.exists();

        let hypervisor_engine = if kvm_available {
            if Path::new("/usr/bin/cloud-hypervisor").exists() || Path::new("/usr/local/bin/cloud-hypervisor").exists() {
                "Cloud-Hypervisor / KVM (Nativo)".to_string()
            } else if Path::new("/usr/bin/firecracker").exists() {
                "Firecracker / KVM (Nativo)".to_string()
            } else {
                "antOS MicroVM Engine (KVM Hardware Assist)".to_string()
            }
        } else if cfg!(target_os = "macos") {
            "Hypervisor.framework (macOS Virtualization)".to_string()
        } else {
            "antOS MicroVM Emulated Sandbox (No KVM)".to_string()
        };

        let active_vms = Self::list_vms(state_dir).unwrap_or_default();
        let total_mem: u32 = active_vms.iter().map(|vm| vm.memory_mb).sum();

        let kernel_version = std::process::Command::new("uname")
            .arg("-r")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "desconocido".to_string());

        Ok(MicrovmStatus {
            kvm_available,
            hypervisor_engine,
            active_vms_count: active_vms.len(),
            total_memory_allocated_mb: total_mem,
            vsock_supported: kvm_available || cfg!(target_os = "linux"),
            kernel_version,
        })
    }

    /// Spawns a new ephemeral microVM instance based on the provided configuration.
    pub fn spawn_vm(state_dir: &Path, config: &MicrovmConfig) -> Result<MicrovmInstance> {
        if config.vcpu_count == 0 || config.vcpu_count > 64 {
            bail!("vCPU count must be between 1 and 64 (got {})", config.vcpu_count);
        }
        if config.memory_mb < 64 || config.memory_mb > 65536 {
            bail!("Memory must be between 64 MB and 64 GB (got {} MB)", config.memory_mb);
        }

        let mut vms = Self::list_vms(state_dir).unwrap_or_default();
        if vms.iter().any(|v| v.id == config.vm_id) {
            bail!("MicroVM with ID «{}» is already running", config.vm_id);
        }

        // Allocate a dedicated vsock CID / port
        let vsock_port = if config.vsock_port != 0 {
            config.vsock_port
        } else {
            5252 + (vms.len() as u32)
        };

        // Determine mock or real PID
        let pid = std::process::id() + 1000 + (vms.len() as u32);
        let created_at = chrono::Local::now().to_rfc3339();

        let instance = MicrovmInstance {
            id: config.vm_id.clone(),
            pid,
            vcpus: config.vcpu_count,
            memory_mb: config.memory_mb,
            vsock_port,
            status: "running".into(),
            created_at,
            command: config.command.clone(),
        };

        vms.push(instance.clone());
        Self::save_vms(state_dir, &vms)?;

        Ok(instance)
    }

    /// Runs `command` on the host, confined by `sandbox::for_host()`
    /// (T31.4) — see this module's doc comment for why that's what
    /// "executing inside the microVM" actually means today.
    ///
    /// Requires `vm_id` to name a registered instance (so `vm exec` on an
    /// unregistered or already-destroyed id still fails the way it always
    /// did), but the registry entry itself carries no capability
    /// declaration to sandbox against — every command runs under the same
    /// [`exec_policy`], the most restrictive one this module grants.
    pub fn exec_vm(state_dir: &Path, vm_id: &str, command: &str) -> Result<MicrovmExecResult> {
        let vms = Self::list_vms(state_dir)?;
        if !vms.iter().any(|v| v.id == vm_id) {
            bail!("MicroVM «{vm_id}» not found or not active");
        }

        let start = Instant::now();

        let (exit_code, stdout, stderr) = if command.trim().is_empty() {
            (0, String::new(), String::new())
        } else {
            let sandbox = sandbox::for_host();
            let changes = vec![Change::HostShellExec { command: command.to_string() }];

            match sandbox::run(sandbox.as_ref(), &changes, &exec_policy()) {
                Ok(outputs) => match outputs.first() {
                    Some(raw) => match serde_json::from_str::<HostExecOutcome>(raw) {
                        Ok(outcome) => (outcome.exit_code, outcome.stdout, outcome.stderr),
                        Err(e) => (-1, String::new(), format!("malformed sandboxed executor response: {e}")),
                    },
                    None => (-1, String::new(), "sandboxed executor returned no output".to_string()),
                },
                // Sandboxing failure must never fall back to running the
                // command unconfined — surface it as the failed exec it is.
                Err(e) => (-1, String::new(), format!("sandboxed execution failed: {e:#}")),
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let success = exit_code == 0;

        Ok(MicrovmExecResult {
            vm_id: vm_id.to_string(),
            command: command.to_string(),
            exit_code,
            stdout,
            stderr,
            duration_ms,
            success,
        })
    }

    /// Lists all currently active or registered microVM instances.
    pub fn list_vms(state_dir: &Path) -> Result<Vec<MicrovmInstance>> {
        let path = Self::store_path(state_dir);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read microVM registry from {}", path.display()))?;
        let list: Vec<MicrovmInstance> = serde_json::from_str(&data).unwrap_or_default();
        Ok(list)
    }

    /// Kills and cleans up an active microVM instance.
    pub fn kill_vm(state_dir: &Path, vm_id: &str) -> Result<()> {
        let mut vms = Self::list_vms(state_dir)?;
        let initial_len = vms.len();
        vms.retain(|v| v.id != vm_id);

        if vms.len() == initial_len {
            bail!("MicroVM «{vm_id}» was not found");
        }

        Self::save_vms(state_dir, &vms)?;
        Ok(())
    }

    fn store_path(state_dir: &Path) -> PathBuf {
        state_dir.join("microvms.json")
    }

    fn save_vms(state_dir: &Path, vms: &[MicrovmInstance]) -> Result<()> {
        let path = Self::store_path(state_dir);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(vms)?;
        fs::write(&path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_microvm_lifecycle_spawn_list_exec_kill() {
        let temp_dir = std::env::temp_dir().join(format!("antos_vm_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let cfg = MicrovmConfig {
            vm_id: "vm-qa-agent".into(),
            vcpu_count: 2,
            memory_mb: 256,
            kernel_image: "/boot/antos-vmlinuz".into(),
            initrd_image: None,
            overlay_disk: None,
            vsock_port: 5252,
            command: None,
        };

        // 1. Spawn
        let instance = MicrovmManager::spawn_vm(&temp_dir, &cfg).expect("spawn vm");
        assert_eq!(instance.id, "vm-qa-agent");
        assert_eq!(instance.vcpus, 2);
        assert_eq!(instance.memory_mb, 256);
        assert_eq!(instance.vsock_port, 5252);
        assert_eq!(instance.status, "running");

        // 2. Prevent duplicate spawn
        let dup_err = MicrovmManager::spawn_vm(&temp_dir, &cfg);
        assert!(dup_err.is_err());

        // 3. List
        let vms = MicrovmManager::list_vms(&temp_dir).expect("list vms");
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].id, "vm-qa-agent");

        // 4. Exec — routes through `sandbox::run`, which re-execs
        // `std::env::current_exe()` with the `__ejecutar` subcommand
        // (T31.4). Inside `cargo test` that binary is the test harness
        // itself, not `antos`, so it can't dispatch that subcommand and the
        // sandboxed round trip fails — the same architectural limitation
        // every other `sandbox::run` caller in this codebase already has
        // (none of them are unit-tested at this exact boundary either; see
        // `session.rs` and `cli/commands/system.rs`). What this test can and
        // does verify: `exec_vm` never panics, never falls back to running
        // the command unconfined, and reports the sandboxing failure
        // through the normal `MicrovmExecResult` shape rather than an
        // opaque `Err` — see `test_run_host_shell_command_*` below for
        // direct coverage of the actual execution logic that runs once
        // inside the confined executor.
        let exec_res = MicrovmManager::exec_vm(&temp_dir, "vm-qa-agent", "echo 'in microvm'").expect("exec_vm must not error out itself");
        assert!(!exec_res.success, "sandboxing cannot succeed inside the test harness — see comment above");
        assert_eq!(exec_res.exit_code, -1);
        assert!(exec_res.stdout.is_empty(), "must never leak unconfined output when sandboxing fails");
        assert!(
            exec_res.stderr.contains("sandboxed execution failed"),
            "failure must be attributed to sandboxing, not silently swallowed: {}",
            exec_res.stderr
        );

        // 5. Status
        let status = MicrovmManager::get_status(&temp_dir).expect("get status");
        assert_eq!(status.active_vms_count, 1);
        assert_eq!(status.total_memory_allocated_mb, 256);

        // 6. Kill
        MicrovmManager::kill_vm(&temp_dir, "vm-qa-agent").expect("kill vm");
        let vms_after = MicrovmManager::list_vms(&temp_dir).expect("list vms after");
        assert_eq!(vms_after.len(), 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_microvm_invalid_constraints() {
        let temp_dir = std::env::temp_dir().join(format!("antos_vm_invalid_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let cfg_zero_cpu = MicrovmConfig {
            vm_id: "vm-zero".into(),
            vcpu_count: 0,
            memory_mb: 256,
            kernel_image: "/boot/antos-vmlinuz".into(),
            initrd_image: None,
            overlay_disk: None,
            vsock_port: 5252,
            command: None,
        };
        assert!(MicrovmManager::spawn_vm(&temp_dir, &cfg_zero_cpu).is_err());

        let cfg_low_mem = MicrovmConfig {
            vm_id: "vm-low".into(),
            vcpu_count: 1,
            memory_mb: 16,
            kernel_image: "/boot/antos-vmlinuz".into(),
            initrd_image: None,
            overlay_disk: None,
            vsock_port: 5252,
            command: None,
        };
        assert!(MicrovmManager::spawn_vm(&temp_dir, &cfg_low_mem).is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    // ---------------------------------------------------------------- T31.4

    #[test]
    fn test_run_host_shell_command_captures_stdout_and_exit_code() {
        let outcome = run_host_shell_command("printf '%s' 'hello from antOS'");
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.stdout, "hello from antOS");
        assert!(outcome.stderr.is_empty());
    }

    #[test]
    fn test_run_host_shell_command_captures_nonzero_exit_and_stderr() {
        let outcome = run_host_shell_command("echo 'boom' 1>&2; exit 7");
        assert_eq!(outcome.exit_code, 7);
        assert!(outcome.stderr.contains("boom"));
    }

    #[test]
    fn test_run_host_shell_command_empty_command_is_a_no_op() {
        let outcome = run_host_shell_command("   ");
        assert_eq!(outcome.exit_code, 0);
        assert!(outcome.stdout.is_empty());
        assert!(outcome.stderr.is_empty());
    }

    #[test]
    fn test_exec_policy_is_the_most_restrictive_default() {
        // No declared writes, reads, or network — an ad hoc `vm exec`
        // command carries no capability declaration to derive a tighter or
        // looser radius from, so the safe default is the empty policy.
        let policy = exec_policy();
        assert!(policy.writes.is_empty());
        assert!(policy.reads.is_empty());
        assert!(!policy.network);
        assert!(policy.allowed_secrets.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_exec_policy_seatbelt_profile_denies_writes_and_network() {
        // On macOS, `sandbox::for_host()` picks Seatbelt whenever
        // `/usr/bin/sandbox-exec` exists (true on every supported macOS
        // host). Confirms the actual SBPL profile `exec_vm` would run
        // under denies both by default, directly — without needing the
        // full self-re-exec round trip that `test_microvm_lifecycle_*`
        // can't exercise inside the test harness.
        let profile = crate::sandbox::seatbelt::sbpl(&exec_policy());
        assert!(profile.contains("(deny file-write*)"));
        assert!(profile.contains("(deny network*)"));
    }
}
