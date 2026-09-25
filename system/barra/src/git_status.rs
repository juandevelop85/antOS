//! Consulta asíncrona del estado Git del workspace activo, usada para el
//! badge `🌿` de la cabecera de la barra.

use crate::session::run_offthread;
use crate::socket_path;
use antos_protocol::{Event, GitRepoStatus, Request};
use gtk4::prelude::*;
use gtk4::Label;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

/// Resultado de `query_git_status_async`, despachado en el hilo principal.
enum GitBadgeUpdate {
    Offline,
    Status(GitRepoStatus),
    Fallback,
}

/// Asynchronously queries the active repository Git status.
pub(crate) fn query_git_status_async(git_badge: Label) {
    run_offthread(
        || -> GitBadgeUpdate {
            let path = socket_path();
            let Ok(mut stream) = UnixStream::connect(&path) else {
                return GitBadgeUpdate::Offline;
            };

            let current_dir = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".into());

            // `project: None` = el proyecto activo que resuelva el demonio
            // (T38.1), lo mismo que enviaba la barra antes de que existiera
            // el campo. Elegirlo desde la barra es T38.2.
            let req = Request::QueryGitStatus {
                workspace_path: current_dir,
                project: None,
            };

            if let Ok(json) = serde_json::to_string(&req) {
                let _ = writeln!(stream, "{json}");
                let _ = stream.flush();

                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() {
                    if let Ok(Event::GitStatus(status)) = serde_json::from_str::<Event>(line.trim())
                    {
                        return GitBadgeUpdate::Status(status);
                    }
                }
            }

            GitBadgeUpdate::Fallback
        },
        move |update| match update {
            GitBadgeUpdate::Offline => {
                git_badge.set_text("● offline");
                git_badge.add_css_class("offline");
            }
            GitBadgeUpdate::Status(status) => update_git_badge(&git_badge, &status),
            GitBadgeUpdate::Fallback => git_badge.set_text("🌿 antOS"),
        },
    );
}

fn update_git_badge(badge: &Label, status: &GitRepoStatus) {
    let branch = status.branch.as_deref().unwrap_or("HEAD");
    let dirty_count = status.modified.len() + status.untracked.len();
    if status.clean {
        badge.set_text(&format!("🌿 {branch} ✓"));
        badge.add_css_class("clean");
    } else {
        badge.set_text(&format!("🌿 {branch} *{dirty_count}"));
        badge.add_css_class("dirty");
    }
}
