//! Asynchronous Notification and Agent Approval Tray (Ticket T8.2).
//!
//! Manages desktop and daemon notifications, alerts for multi-agent flow transitions,
//! and handles direct one-click approvals, rejections, and worktree rollbacks.

use crate::util::lock_or_recover;
use antos_protocol::{NotificationAction, NotificationItem, NotificationKind};
use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct NotificationEngine {
    _private: (),
}

static NOTIF_LOCK: Mutex<()> = Mutex::new(());

impl NotificationEngine {
    pub fn global() -> Self {
        Self { _private: () }
    }

    /// Resolves the storage path for notifications (`.antos/notifications.json`).
    fn storage_path(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("notifications.json")
    }

    /// Lists all notifications, loading them from `.antos/notifications.json`.
    pub fn list(&self, workspace: &Path) -> Result<Vec<NotificationItem>> {
        let _guard = lock_or_recover(&NOTIF_LOCK);
        let path = Self::storage_path(workspace);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(Vec::new());
        }

        let list: Vec<NotificationItem> = serde_json::from_str(&content).unwrap_or_default();
        Ok(list)
    }

    /// Adds a new notification item and persists it atomically.
    pub fn notify(&self, workspace: &Path, mut item: NotificationItem) -> Result<()> {
        let _guard = lock_or_recover(&NOTIF_LOCK);
        let path = Self::storage_path(workspace);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut current = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str::<Vec<NotificationItem>>(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        if item.created_at == 0 {
            item.created_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
        }

        current.retain(|n| n.id != item.id);
        current.insert(0, item);

        // Keep maximum 50 notifications
        if current.len() > 50 {
            current.truncate(50);
        }

        let json = serde_json::to_string_pretty(&current)?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Executes an action on a notification (Approve, Reject, Dismiss, ViewDiff).
    pub fn handle_action(
        &self,
        workspace: &Path,
        notification_id: &str,
        action: NotificationAction,
    ) -> Result<(bool, String)> {
        let mut notifs = self.list(workspace)?;
        let notif_idx = notifs.iter().position(|n| n.id == notification_id);

        let notif = match notif_idx {
            Some(idx) => notifs[idx].clone(),
            None => bail!("notificación «{notification_id}» no encontrada"),
        };

        let message = match action {
            NotificationAction::Approve => {
                match crate::flow::FlowEngine::global().approve_task(&notif.ticket_id, true) {
                    Ok(_) => format!(
                        "Aprobación concedida: cambios del ticket {} fusionados con éxito.",
                        notif.ticket_id
                    ),
                    Err(e) => format!(
                        "Error al aprobar cambios del ticket {}: {e:#}",
                        notif.ticket_id
                    ),
                }
            }
            NotificationAction::Reject => {
                match crate::flow::FlowEngine::global().approve_task(&notif.ticket_id, false) {
                    Ok(_) => format!(
                        "Rollback completado: cambios del ticket {} revertidos y worktree limpiado.",
                        notif.ticket_id
                    ),
                    Err(e) => format!("Error al revertir cambios del ticket {}: {e:#}", notif.ticket_id),
                }
            }
            NotificationAction::Dismiss => {
                format!("Notificación «{notification_id}» descartada.")
            }
            NotificationAction::ViewDiff => {
                format!("Inspección de diff solicitada para {}.", notif.ticket_id)
            }
        };

        // Update read status or remove if dismissed
        if let Some(idx) = notif_idx {
            if action == NotificationAction::Dismiss {
                notifs.remove(idx);
            } else {
                notifs[idx].read = true;
            }

            let _guard = lock_or_recover(&NOTIF_LOCK);
            let path = Self::storage_path(workspace);
            let json = serde_json::to_string_pretty(&notifs)?;
            fs::write(&path, json)?;
        }

        Ok((true, message))
    }

    /// Clears all read notifications from disk.
    pub fn clear(&self, workspace: &Path) -> Result<usize> {
        let _guard = lock_or_recover(&NOTIF_LOCK);
        let path = Self::storage_path(workspace);
        if !path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&path)?;
        let mut notifs: Vec<NotificationItem> = serde_json::from_str(&content).unwrap_or_default();
        let initial_len = notifs.len();
        notifs.retain(|n| !n.read);

        let removed = initial_len - notifs.len();
        let json = serde_json::to_string_pretty(&notifs)?;
        fs::write(&path, json)?;
        Ok(removed)
    }
}

/// Helper to create an approval notification for a finished ticket task.
pub fn notify_ticket_ready_for_review(
    workspace: &Path,
    ticket_id: &str,
    title: &str,
) -> Result<()> {
    let notif = NotificationItem {
        id: format!("notif-{}", ticket_id.to_lowercase().replace('.', "-")),
        ticket_id: ticket_id.to_string(),
        title: format!("Revisión requerida: {ticket_id}"),
        body: format!("El equipo multi-agente antFlow ha completado {title}. Pruebas de QA validadas en worktree."),
        kind: NotificationKind::ApprovalRequired,
        created_at: 0,
        read: false,
        actions: vec![
            NotificationAction::Approve,
            NotificationAction::Reject,
            NotificationAction::ViewDiff,
            NotificationAction::Dismiss,
        ],
    };

    NotificationEngine::global().notify(workspace, notif)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_notification_lifecycle() {
        let temp = std::env::temp_dir().join(format!("test-notifs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = NotificationEngine::global();
        assert_eq!(engine.list(&temp).unwrap().len(), 0);

        let notif = NotificationItem {
            id: "notif-t82".into(),
            ticket_id: "T8.2".into(),
            title: "Prueba de notificación".into(),
            body: "Cuerpo de prueba".into(),
            kind: NotificationKind::TaskFinished,
            created_at: 1000,
            read: false,
            actions: vec![NotificationAction::Dismiss],
        };

        engine.notify(&temp, notif).unwrap();
        let list = engine.list(&temp).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "notif-t82");

        let (ok, msg) = engine
            .handle_action(&temp, "notif-t82", NotificationAction::Dismiss)
            .unwrap();
        assert!(ok);
        assert!(msg.contains("descartada"));

        assert_eq!(engine.list(&temp).unwrap().len(), 0);
        let _ = fs::remove_dir_all(&temp);
    }

    /// T31.7 acceptance criterion, against the real production global lock
    /// (not a synthetic one): poison `NOTIF_LOCK` by panicking on another
    /// thread while holding it, then confirm the public API — `list`, which
    /// takes that same lock via `lock_or_recover` — still works afterward,
    /// in this same process, instead of panicking on the poison.
    #[test]
    fn test_notification_engine_survives_a_poisoned_global_lock() {
        let temp = std::env::temp_dir().join(format!("test-notifs-poison-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let handle = std::thread::spawn(|| {
            let _guard = NOTIF_LOCK.lock().unwrap();
            panic!("intentional panic to poison NOTIF_LOCK for this test");
        });
        assert!(
            handle.join().is_err(),
            "the spawned thread must actually have panicked"
        );

        // A plain `.lock().unwrap()` inside `list`/`notify` would panic
        // again here, taking this test (and, in production, every other
        // caller of NOTIF_LOCK) down with it. `lock_or_recover` must not.
        let engine = NotificationEngine::global();
        assert_eq!(engine.list(&temp).unwrap().len(), 0);

        let notif = NotificationItem {
            id: "notif-after-poison".into(),
            ticket_id: "T31.7".into(),
            title: "Sigue funcionando tras el envenenamiento".into(),
            body: String::new(),
            kind: NotificationKind::TaskFinished,
            created_at: 1000,
            read: false,
            actions: vec![NotificationAction::Dismiss],
        };
        engine.notify(&temp, notif).unwrap();
        assert_eq!(engine.list(&temp).unwrap().len(), 1);

        let _ = fs::remove_dir_all(&temp);
    }
}
