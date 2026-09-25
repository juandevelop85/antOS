//! antOS Wayland/GTK4 Layer Shell Intent Bar & Agent Mission Center (T4.1 & T4.2).
//!
//! Provides the primary desktop interface for developer intentions, semantic git context,
//! rich diff inspections, multi-agent antFlow state visualizations, and the Kanban Ticket Board (`Super + A`).

extern crate antos_protocol as antos_protocol;

mod git_status;
mod kanban;
mod launcher;
mod session;
mod telemetry;
mod tray;
mod ui;
mod widgets;

use gtk4::gdk::Display;
use gtk4::prelude::*;
use gtk4::{Application, CssProvider};
use std::path::PathBuf;

pub(crate) const BAR_WIDTH: i32 = 820;

/// Altura fija del área de respuestas.
///
/// La barra tiene dos tamaños y solo dos: compacta mientras no hay nada que
/// enseñar, y esta altura en cuanto lo hay. Antes seguía al contenido, así
/// que un plan largo la estiraba media pantalla y la respuesta siguiente la
/// encogía; en una superficie de layer-shell anclada arriba eso es un salto
/// visible en cada mensaje.
pub(crate) const CONTENT_HEIGHT: i32 = 420;

/// Reads an environment variable, falling back to its `SYSO_*` predecessor
/// with a one-line deprecation notice on `stderr` (T31.11: retiring the
/// `syso` compatibility layer over one soft-transition cycle instead of
/// dropping `SYSO_*` in silence). Small enough to duplicate here rather than
/// pull in a dependency on `antosd` — `antos-barra` only depends on
/// `antos-protocol` today.
fn env_with_legacy_fallback(current: &str, legacy: &str) -> Option<std::ffi::OsString> {
    if let Some(v) = std::env::var_os(current) {
        return Some(v);
    }
    let v = std::env::var_os(legacy)?;
    eprintln!("antOS · aviso: {legacy} está obsoleta, usa {current} en su lugar (T31.11)");
    Some(v)
}

/// Resolves the antOS daemon UNIX socket path.
pub(crate) fn socket_path() -> PathBuf {
    if let Some(v) = env_with_legacy_fallback("ANTOS_SOCKET", "SYSO_SOCKET") {
        return PathBuf::from(v);
    }
    let state_dir = env_with_legacy_fallback("ANTOS_STATE", "SYSO_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".antos"));
    state_dir.join("antos.sock")
}

fn main() {
    let app = Application::builder()
        .application_id("dev.antos.barra")
        .build();

    app.connect_startup(|_| {
        let provider = CssProvider::new();
        #[allow(deprecated)]
        provider.load_from_data(include_str!("estilo.css"));
        if let Some(display) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });

    app.connect_activate(ui::build_ui);

    app.run_with_args::<&str>(&[]);
}
