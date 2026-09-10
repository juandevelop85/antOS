//! antOS eBPF LSM Sentinel and Syscall Supervisor (T11.1).
//!
//! ## Estado de implementación (T31.14)
//!
//! Este módulo es un panel de auditoría **simulado**, no un supervisor
//! eBPF real. Ningún código aquí (ni en el resto del árbol: no hay
//! dependencia de `libbpf`, `aya`, ni ninguna llamada al syscall `bpf(2)`)
//! carga un programa LSM. Lo que existe es:
//!
//! - Un `VecDeque<EbpfSecurityEvent>` en memoria, protegido por un `Mutex`
//!   ([`EBPF_STATE`]), con persistencia a `.antos/ebpf_audit.json`.
//! - [`EbpfSentinelEngine::record_event`], que cualquier llamador puede
//!   invocar directamente para añadir un evento — no hay ningún hook del
//!   kernel detrás disparándolo.
//! - [`EbpfSentinelEngine::simulate_violation`], usado por `antos ebpf
//!   simulate` y por la petición IPC `SimulateEbpfViolation`, que fabrica un
//!   evento de ejemplo (nombre de proceso y motivo son texto fijo elegido
//!   por el `hook`, no observados de ningún proceso real).
//! - [`EbpfSentinelEngine::is_lsm_supported`], que solo comprueba si el
//!   *kernel* del host anuncia el LSM `bpf` en
//!   `/sys/kernel/security/lsm` — una propiedad de la plataforma, cierta en
//!   la mayoría de distros Linux modernas, que no implica que este módulo
//!   esté usándolo.
//!
//! [`EbpfStatus::backend`] refleja esto explícitamente
//! (`EbpfBackend::Simulated` hoy) para que ni el CLI ni la barra puedan
//! confundir este panel con vigilancia real del kernel. Implementar un
//! backend `LinuxBpf` de verdad —adjuntar sondas a
//! `bprm_check_security`/`file_open`/`socket_connect` vía `aya` o
//! `libbpf-rs`— es un ticket propio de infraestructura, no una corrección
//! puntual de esta auditoría.

use crate::util::lock_or_recover;
use antos_protocol::{
    EbpfBackend, EbpfHookKind, EbpfSecurityAction, EbpfSecurityEvent, EbpfStatus,
    NotificationAction, NotificationItem, NotificationKind,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const RING_BUFFER_CAPACITY: usize = 1024;

static EBPF_STATE: Mutex<EbpfState> = Mutex::new(EbpfState {
    total_events_captured: 0,
    total_violations_blocked: 0,
    ring_buffer: VecDeque::new(),
});

#[derive(Serialize, Deserialize, Default)]
struct EbpfPersistedState {
    total_events_captured: usize,
    total_violations_blocked: usize,
    events: Vec<EbpfSecurityEvent>,
}

struct EbpfState {
    total_events_captured: usize,
    total_violations_blocked: usize,
    ring_buffer: VecDeque<EbpfSecurityEvent>,
}

pub struct EbpfSentinelEngine;

impl EbpfSentinelEngine {
    pub fn global() -> Self {
        Self
    }

    /// Resolves the current workspace using Ctx::discover with fallback to current directory.
    fn current_workspace() -> PathBuf {
        crate::ctx::Ctx::discover()
            .map(|c| c.workspace)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    /// Resolves the storage path for audit logs (`.antos/ebpf_audit.json`).
    fn storage_path(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("ebpf_audit.json")
    }

    /// Loads persisted events from disk into the in-memory state.
    pub fn load_state(&self, workspace: &Path) {
        let path = Self::storage_path(workspace);
        if !path.exists() {
            return;
        }

        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(persisted) = serde_json::from_str::<EbpfPersistedState>(&content) {
                let mut state = lock_or_recover(&EBPF_STATE);
                state.total_events_captured = persisted.total_events_captured;
                state.total_violations_blocked = persisted.total_violations_blocked;
                state.ring_buffer.clear();
                for ev in persisted.events {
                    if state.ring_buffer.len() >= RING_BUFFER_CAPACITY {
                        state.ring_buffer.pop_front();
                    }
                    state.ring_buffer.push_back(ev);
                }
            }
        }
    }

    /// Persists current events to disk.
    fn persist_state(&self, workspace: &Path) {
        let path = Self::storage_path(workspace);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let state = lock_or_recover(&EBPF_STATE);
        let persisted = EbpfPersistedState {
            total_events_captured: state.total_events_captured,
            total_violations_blocked: state.total_violations_blocked,
            events: state.ring_buffer.iter().cloned().collect(),
        };

        if let Ok(json) = serde_json::to_string_pretty(&persisted) {
            let _ = fs::write(path, json);
        }
    }

    /// Checks whether the *host kernel* advertises the `bpf` LSM. This says
    /// nothing about whether this module is using it — see the module-level
    /// doc comment: the backend today is always [`EbpfBackend::Simulated`].
    pub fn is_lsm_supported() -> bool {
        #[cfg(target_os = "linux")]
        {
            let lsm_path = Path::new("/sys/kernel/security/lsm");
            if let Ok(content) = fs::read_to_string(lsm_path) {
                return content.contains("bpf");
            }
        }
        false
    }

    /// Returns the list of active kernel hooks / probes.
    pub fn active_probes() -> Vec<String> {
        vec![
            "lsm/bprm_check_security".into(),
            "lsm/file_open".into(),
            "lsm/socket_connect".into(),
            "tracepoint/syscalls/sys_enter".into(),
        ]
    }

    /// Queries current status of the eBPF sentinel and ring buffer telemetry.
    pub fn status(&self) -> Result<EbpfStatus> {
        let ws = Self::current_workspace();
        self.load_state(&ws);

        let state = lock_or_recover(&EBPF_STATE);
        let lsm_enabled = Self::is_lsm_supported();

        Ok(EbpfStatus {
            available: true,
            lsm_enabled,
            // T31.14: siempre `Simulated` — ningún backend `LinuxBpf` está
            // implementado en el árbol todavía (ver doc del módulo).
            backend: EbpfBackend::Simulated,
            active_probes: Self::active_probes(),
            total_events_captured: state.total_events_captured,
            total_violations_blocked: state.total_violations_blocked,
            ring_buffer_capacity: RING_BUFFER_CAPACITY,
            ring_buffer_utilization: state.ring_buffer.len(),
        })
    }

    /// Retrieves the most recent security events from the ring buffer.
    pub fn get_audit_log(&self, limit: usize) -> Vec<EbpfSecurityEvent> {
        let ws = Self::current_workspace();
        self.load_state(&ws);

        let state = lock_or_recover(&EBPF_STATE);
        let count = limit.min(state.ring_buffer.len());
        state
            .ring_buffer
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect()
    }

    /// Filters audit events for a specific process ID.
    pub fn trace_pid(&self, pid: u32) -> Vec<EbpfSecurityEvent> {
        let ws = Self::current_workspace();
        self.load_state(&ws);

        let state = lock_or_recover(&EBPF_STATE);
        state
            .ring_buffer
            .iter()
            .filter(|e| e.pid == pid)
            .cloned()
            .collect()
    }

    /// Records a new kernel security event into the ring buffer.
    pub fn record_event(
        &self,
        hook: EbpfHookKind,
        pid: u32,
        comm: &str,
        target_resource: &str,
        action_taken: EbpfSecurityAction,
        violation_reason: Option<&str>,
    ) -> EbpfSecurityEvent {
        let ws = Self::current_workspace();
        self.load_state(&ws);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let event = {
            let mut state = lock_or_recover(&EBPF_STATE);
            state.total_events_captured += 1;

            if action_taken == EbpfSecurityAction::Blocked {
                state.total_violations_blocked += 1;
            }

            let ev = EbpfSecurityEvent {
                id: format!("ebpf-{}", state.total_events_captured),
                timestamp_ms: now,
                pid,
                comm: comm.to_string(),
                hook,
                target_resource: target_resource.to_string(),
                action_taken,
                violation_reason: violation_reason.map(ToString::to_string),
            };

            if state.ring_buffer.len() >= RING_BUFFER_CAPACITY {
                state.ring_buffer.pop_front();
            }
            state.ring_buffer.push_back(ev.clone());
            ev
        };

        self.persist_state(&ws);

        // If blocked, trigger asynchronous alert to notification inbox (T8.2)
        if action_taken == EbpfSecurityAction::Blocked {
            let reason_str =
                violation_reason.unwrap_or("Intento de violación de sandbox detectado por eBPF");
            let notif = NotificationItem {
                id: format!("ebpf-notif-{}", now),
                ticket_id: "EBPF-LSM".into(),
                title: format!("🚨 Alerta Kernel eBPF LSM: Evasión Bloqueada (PID {pid})"),
                body: format!("Proceso «{comm}» intentó acceder a «{target_resource}» vía hook «{:?}». Acción: Bloqueado por el kernel. Motivo: {reason_str}", hook),
                kind: NotificationKind::SecurityAlert,
                created_at: now / 1000,
                read: false,
                actions: vec![NotificationAction::Reject, NotificationAction::Dismiss],
            };
            let _ = crate::notification::NotificationEngine::global().notify(&ws, notif);
        }

        event
    }

    /// Simulates an unauthorized access attempt to test sentinel detection and alerts.
    pub fn simulate_violation(
        &self,
        hook: EbpfHookKind,
        target_resource: &str,
    ) -> EbpfSecurityEvent {
        let (comm, reason) = match hook {
            EbpfHookKind::SocketConnect => (
                "curl",
                "Conexión de red saliente hacia IP no declarada en el radio de impacto",
            ),
            EbpfHookKind::FileOpen => (
                "cat",
                "Intento de lectura no autorizada de credenciales sensibles fuera del sandbox",
            ),
            EbpfHookKind::BprmCheckSecurity => (
                "bash",
                "Ejecución de binario no autorizado bloqueada por política LSM",
            ),
            EbpfHookKind::SyscallTrace => (
                "ptrace_agent",
                "Llamada al sistema ptrace denegada en espacio de sandbox",
            ),
        };

        self.record_event(
            hook,
            std::process::id(),
            comm,
            target_resource,
            EbpfSecurityAction::Blocked,
            Some(reason),
        )
    }

    /// Clears the in-memory and persisted ring buffer (for tests).
    pub fn reset(&self) {
        let ws = Self::current_workspace();
        let path = Self::storage_path(&ws);
        let _ = fs::remove_file(path);

        let mut state = lock_or_recover(&EBPF_STATE);
        state.total_events_captured = 0;
        state.total_violations_blocked = 0;
        state.ring_buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_ebpf_status_and_probes() {
        let _guard = TEST_LOCK.lock().unwrap();
        let engine = EbpfSentinelEngine::global();
        let status = engine.status().expect("status");
        assert!(status.available);
        assert!(!status.active_probes.is_empty());
        assert_eq!(status.ring_buffer_capacity, RING_BUFFER_CAPACITY);
    }

    #[test]
    fn test_ebpf_status_reports_the_simulated_backend_honestly() {
        // T31.14: no hay backend `LinuxBpf` implementado en el árbol, así
        // que `status()` nunca debe reportar otra cosa que `Simulated` —
        // sea cual sea el LSM que anuncie el kernel del host que ejecuta el
        // test (`lsm_enabled` es independiente de esto).
        let _guard = TEST_LOCK.lock().unwrap();
        let engine = EbpfSentinelEngine::global();
        let status = engine.status().expect("status");
        assert_eq!(status.backend, EbpfBackend::Simulated);
    }

    #[test]
    fn test_record_allowed_and_blocked_events() {
        let _guard = TEST_LOCK.lock().unwrap();
        let engine = EbpfSentinelEngine::global();
        engine.reset();

        let ev1 = engine.record_event(
            EbpfHookKind::FileOpen,
            1001,
            "cargo",
            "Cargo.toml",
            EbpfSecurityAction::Allowed,
            None,
        );
        assert_eq!(ev1.action_taken, EbpfSecurityAction::Allowed);
        assert_eq!(ev1.comm, "cargo");

        let ev2 = engine.record_event(
            EbpfHookKind::SocketConnect,
            1002,
            "agent_worker",
            "10.0.0.1:8080",
            EbpfSecurityAction::Blocked,
            Some("Network access undeclared"),
        );
        assert_eq!(ev2.action_taken, EbpfSecurityAction::Blocked);

        let status = engine.status().expect("status");
        assert_eq!(status.total_events_captured, 2);
        assert_eq!(status.total_violations_blocked, 1);

        let log = engine.get_audit_log(10);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].id, ev2.id); // Most recent first
    }

    #[test]
    fn test_simulate_violation_and_trace_pid() {
        let _guard = TEST_LOCK.lock().unwrap();
        let engine = EbpfSentinelEngine::global();
        engine.reset();

        let my_pid = std::process::id();
        let ev = engine.simulate_violation(EbpfHookKind::FileOpen, "/etc/shadow");
        assert_eq!(ev.pid, my_pid);
        assert_eq!(ev.action_taken, EbpfSecurityAction::Blocked);
        assert_eq!(ev.target_resource, "/etc/shadow");

        let trace = engine.trace_pid(my_pid);
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].id, ev.id);
    }
}
