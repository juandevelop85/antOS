//! Gestor del entorno de escritorio gráfico Wayland para antOS (T13.0).
//!
//! Este módulo supervisa la sesión gráfica de antOS, detecta compositores
//! activos compatibles con wlroots/layer-shell (Labwc, Sway), expone los atajos de teclado
//! globales del sistema y gestiona la generación declarativa de configuraciones.

use antos_protocol::{DesktopHotkey, DesktopSessionStatus};
use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Gestor de la sesión gráfica de escritorio de antOS.
pub struct DesktopManager;

impl DesktopManager {
    /// Obtiene el catálogo de atajos de teclado globales registrados en el escritorio.
    pub fn get_hotkeys() -> Vec<DesktopHotkey> {
        vec![
            DesktopHotkey {
                key: "Super+Space".into(),
                action: "toggle_intent_bar".into(),
                description: "Abrir o enfocar la barra de intenciones de antOS".into(),
            },
            DesktopHotkey {
                key: "Super+A".into(),
                action: "toggle_agent_center".into(),
                description: "Desplegar el Centro de Control de Agentes y Tablero de Tickets"
                    .into(),
            },
            DesktopHotkey {
                key: "Super+Return".into(),
                action: "open_terminal".into(),
                description: "Abrir terminal virtual interactiva de desarrollo (vte)".into(),
            },
            DesktopHotkey {
                key: "Super+D".into(),
                action: "open_diff_viewer".into(),
                description: "Abrir visor interactivo de diffs y reversión granular".into(),
            },
            DesktopHotkey {
                key: "Super+E".into(),
                action: "open_editor".into(),
                description: "Abrir el editor de texto predeterminado (Neovim)".into(),
            },
            DesktopHotkey {
                key: "Super+W".into(),
                action: "open_dev_workspace".into(),
                description: "Abrir el espacio de trabajo integrado Dev TUI (Neovim + antOS)"
                    .into(),
            },
            DesktopHotkey {
                key: "Super+Q".into(),
                action: "close_window".into(),
                description: "Cerrar la ventana enfocada actualmente".into(),
            },
            DesktopHotkey {
                key: "Alt+Tab".into(),
                action: "next_window".into(),
                description: "Conmutar a la siguiente ventana".into(),
            },
            DesktopHotkey {
                key: "Super+F".into(),
                action: "toggle_fullscreen".into(),
                description: "Alternar modo pantalla completa".into(),
            },
            DesktopHotkey {
                key: "Super+Shift+E".into(),
                action: "exit_session".into(),
                description: "Finalizar la sesión gráfica de escritorio".into(),
            },
        ]
    }

    /// Consulta el estado actual de la sesión de escritorio Wayland.
    pub fn get_status() -> DesktopSessionStatus {
        let wayland_display = env::var("WAYLAND_DISPLAY").ok();
        let running_in_wayland = wayland_display.is_some()
            || env::var("XDG_SESSION_TYPE")
                .map(|v| v == "wayland")
                .unwrap_or(false);

        // Detectar compositor activo mediante pgrep o variables de entorno
        let compositor_name = if let Ok(comp) = env::var("ANTOS_COMPOSITOR") {
            comp
        } else if is_process_running("labwc") {
            "labwc (wlroots)".into()
        } else if is_process_running("sway") {
            "sway (wlroots)".into()
        } else if is_process_running("hyprland") {
            "hyprland".into()
        } else if running_in_wayland {
            "wayland-generic".into()
        } else {
            "ninguno (sesión no gráfica o headless)".into()
        };

        let is_running =
            running_in_wayland || is_process_running("labwc") || is_process_running("sway");

        DesktopSessionStatus {
            running: is_running,
            compositor_name,
            wayland_display,
            active_clients_count: if is_running { 1 } else { 0 },
            registered_hotkeys: Self::get_hotkeys(),
        }
    }

    /// Prepara e instala las configuraciones declarativas del escritorio en `~/.config/labwc`.
    pub fn sync_configuration(workspace_path: &Path) -> Result<PathBuf> {
        let home_dir = env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        let target_dir = home_dir.join(".config").join("labwc");
        fs::create_dir_all(&target_dir)
            .with_context(|| format!("No se pudo crear {}", target_dir.display()))?;

        if let Some(source_desktop_dir) = find_desktop_dir(workspace_path) {
            let source_rc = source_desktop_dir.join("rc.xml");
            let source_autostart = source_desktop_dir.join("autostart");

            if source_rc.exists() {
                let _ = fs::copy(&source_rc, target_dir.join("rc.xml"));
            } else {
                let default_rc = include_str!("../../desktop/rc.xml");
                let _ = fs::write(target_dir.join("rc.xml"), default_rc);
            }

            if source_autostart.exists() {
                let _ = fs::copy(&source_autostart, target_dir.join("autostart"));
            }
        } else {
            let default_rc = include_str!("../../desktop/rc.xml");
            let _ = fs::write(target_dir.join("rc.xml"), default_rc);
        }

        Ok(target_dir)
    }

    /// Lanza o diagnostica la sesión de escritorio mediante `start-session.sh`.
    pub fn start_session(workspace_path: &Path, nested: bool) -> Result<String> {
        let desktop_dir = find_desktop_dir(workspace_path).ok_or_else(|| {
            anyhow::anyhow!("No se encontró el directorio system/desktop en el espacio de trabajo")
        })?;
        let script = desktop_dir.join("start-session.sh");
        if !script.exists() {
            anyhow::bail!("No se encontró el script de inicio {}", script.display());
        }

        let mut cmd = Command::new("bash");
        cmd.arg(&script);
        if nested {
            cmd.arg("--nested");
        }

        let output = cmd
            .output()
            .with_context(|| format!("Error al ejecutar {}", script.display()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() && !stderr.is_empty() {
            Ok(format!("{}\n{}", stdout, stderr))
        } else {
            Ok(stdout)
        }
    }
}

/// Localiza el directorio `system/desktop` navegando hacia arriba en el árbol de rutas.
fn find_desktop_dir(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join("system").join("desktop");
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    if let Ok(cwd) = env::current_dir() {
        let candidate = cwd.join("system").join("desktop");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Comprueba de forma segura si un proceso está en ejecución.
fn is_process_running(proc_name: &str) -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg(proc_name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_desktop_hotkeys_definition() {
        let hotkeys = DesktopManager::get_hotkeys();
        assert!(!hotkeys.is_empty());
        assert!(hotkeys.iter().any(|h| h.key == "Super+Space"));
        assert!(hotkeys.iter().any(|h| h.key == "Super+A"));
        assert!(hotkeys.iter().any(|h| h.key == "Super+Return"));
        assert!(hotkeys.iter().any(|h| h.key == "Super+D"));
        assert!(hotkeys.iter().any(|h| h.key == "Super+E"));
        assert!(hotkeys.iter().any(|h| h.key == "Super+W"));
    }

    #[test]
    fn test_desktop_status_inspection() {
        let status = DesktopManager::get_status();
        assert_eq!(
            status.registered_hotkeys.len(),
            DesktopManager::get_hotkeys().len()
        );
        // En entorno de test sin display, informa estado no ejecutable limpiamente
        if env::var("WAYLAND_DISPLAY").is_err() {
            assert!(!status.compositor_name.is_empty());
        }
    }

    #[test]
    fn test_desktop_sync_configuration() {
        let temp_dir = env::temp_dir().join(format!("test_antos_desktop_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let cwd = env::current_dir().expect("cwd");
        let sync_res = DesktopManager::sync_configuration(&cwd);
        assert!(sync_res.is_ok());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
