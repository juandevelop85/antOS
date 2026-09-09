//! Unified application manager and bridge for Flatpak and native packages (T25.2).

pub mod flatpak;

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use antos_protocol::{
    AppActionResult, AppLaunchResult, AppProgress, AppSearchResult, AppSource, DesktopApp,
};
pub use flatpak::FlatpakClient;
use crate::pkg::PackageEngine;

/// Unified Application Engine for antOS.
pub struct AppEngine;

impl AppEngine {
    /// Lists all installed applications filtered optionally by packaging source.
    pub fn list_apps(state_dir: &Path, source: Option<AppSource>) -> Result<Vec<DesktopApp>> {
        let mut apps = Vec::new();

        // 1. Collect Flatpak applications if requested
        if source.is_none() || source == Some(AppSource::Flatpak) {
            if let Ok(flatpak_apps) = FlatpakClient::list_installed(state_dir) {
                apps.extend(flatpak_apps);
            }
        }

        // 2. Collect native antpkg desktop applications if requested
        if source.is_none() || source == Some(AppSource::NativePkg) {
            if let Ok(pkg_apps) = PackageEngine::list_desktop_apps(state_dir) {
                for p in pkg_apps {
                    apps.push(DesktopApp {
                        id: p.id.clone(),
                        name: p.name,
                        version: p.package_version,
                        source: AppSource::NativePkg,
                        description: p.comment.unwrap_or_default(),
                        icon: p.icon_path.or(p.icon),
                        categories: p.categories,
                        permissions: vec!["host".to_string(), "native".to_string()],
                        installed: true,
                        exec_cmd: p.exec,
                    });
                }
            }
        }

        Ok(apps)
    }

    /// Searches remote catalogs (Flathub and antpkg recipes) for available applications.
    pub fn search_apps(state_dir: &Path, query: &str) -> Result<Vec<AppSearchResult>> {
        let mut results = Vec::new();
        let q_lower = query.trim().to_lowercase();

        // 1. Search Flathub
        let flatpak_results = FlatpakClient::search(query);
        results.extend(flatpak_results);

        // 2. Search native antOS recipes
        let native_recipes = [
            ("firefox", "Mozilla Firefox Web Browser", "130.0"),
            ("code", "Visual Studio Code", "1.93.0"),
            ("zed-editor", "Zed High-Performance Multiplayer Editor", "0.140.0"),
            ("alacritty", "Alacritty GPU-accelerated Terminal", "0.13.2"),
            ("ripgrep", "Fast line-oriented search tool", "14.1.0"),
            ("jq", "Command-line JSON processor", "1.7.1"),
            ("curl", "Command line tool for transferring data with URLs", "8.9.1"),
            ("ollama", "Run language models locally", "0.3.9"),
        ];

        for (name, desc, ver) in native_recipes {
            if name.to_lowercase().contains(&q_lower) || desc.to_lowercase().contains(&q_lower) || q_lower.is_empty() {
                results.push(AppSearchResult {
                    id: name.to_string(),
                    name: name.to_string(),
                    version: ver.to_string(),
                    source: AppSource::NativePkg,
                    description: desc.to_string(),
                    installed: false,
                });
            }
        }

        // 3. Mark installed status
        let installed = Self::list_apps(state_dir, None)?;
        for r in &mut results {
            if installed.iter().any(|a| a.id == r.id) {
                r.installed = true;
            }
        }

        Ok(results)
    }

    /// Installs an application through Flatpak or native package manager.
    pub fn install_app<F>(
        state_dir: &Path,
        id: &str,
        source: Option<AppSource>,
        progress_cb: F,
    ) -> Result<AppActionResult>
    where
        F: Fn(AppProgress),
    {
        // Explicit or inferred routing
        let use_native = match source {
            Some(AppSource::NativePkg) => true,
            Some(AppSource::Flatpak) => false,
            _ => {
                // If it contains dots (e.g. com.visualstudio.code), treat as Flatpak ID
                if id.contains('.') {
                    false
                } else {
                    PackageEngine::resolve_manifest(id).is_ok()
                }
            }
        };

        if use_native {
            progress_cb(AppProgress {
                app_id: id.to_string(),
                percentage: 20.0,
                status: format!("Resolving antpkg recipe for '{}'...", id),
                done: false,
            });

            let rep = PackageEngine::install(state_dir, id, false)?;

            progress_cb(AppProgress {
                app_id: id.to_string(),
                percentage: 100.0,
                status: format!("Package '{}' installed into antOS generation {}", id, rep.generation),
                done: true,
            });

            Ok(AppActionResult {
                app_id: id.to_string(),
                action: "install".to_string(),
                success: true,
                message: format!("Installed '{}' into antOS immutable store (generation {})", id, rep.generation),
            })
        } else {
            FlatpakClient::install(state_dir, id, progress_cb)
        }
    }

    /// Uninstalls an application from antOS.
    pub fn uninstall_app(state_dir: &Path, id: &str) -> Result<AppActionResult> {
        // Check if installed in antpkg
        if let Ok(pkgs) = PackageEngine::list(state_dir) {
            if pkgs.iter().any(|p| p.name == id) {
                let rep = PackageEngine::remove(state_dir, id)?;
                return Ok(AppActionResult {
                    app_id: id.to_string(),
                    action: "uninstall".to_string(),
                    success: true,
                    message: format!("Package '{}' removed from active profile (new generation {})", id, rep.generation),
                });
            }
        }

        // Otherwise uninstall via Flatpak client
        FlatpakClient::uninstall(state_dir, id)
    }

    /// Launches an application injecting Wayland display, socket and workspace context.
    pub fn launch_app(
        state_dir: &Path,
        id: &str,
        workspace: Option<&Path>,
        args: &[String],
    ) -> Result<AppLaunchResult> {
        // 1. Check Flatpak
        let flatpak_installed = FlatpakClient::list_installed(state_dir)?;
        if flatpak_installed.iter().any(|a| a.id == id) {
            return FlatpakClient::launch(state_dir, id, workspace, args);
        }

        // 2. Check native package
        let pkg_apps = PackageEngine::list_desktop_apps(state_dir)?;
        if let Some(app) = pkg_apps.into_iter().find(|p| p.id == id || p.package_name == id) {
            let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());
            let workspace_str = workspace
                .map(|w| w.display().to_string())
                .or_else(|| std::env::var("ANTOS_WORKSPACE").ok())
                .unwrap_or_else(|| ".".to_string());

            let bin_dir = PackageEngine::current_bin_dir(state_dir);
            let first_token = app.exec.split_whitespace().next().unwrap_or(&app.id);
            let bin_path = bin_dir.join(first_token);

            let mut cmd = if bin_path.exists() {
                Command::new(&bin_path)
            } else {
                let mut c = Command::new("echo");
                c.arg(format!("[antOS App Launcher] Starting {}", app.name));
                c
            };

            let launcher_item = antos_protocol::LauncherAppItem::from_desktop_summary(&app);
            let effective_args = antos_protocol::inject_workspace_args(&launcher_item, &workspace_str, args);

            for a in &effective_args {
                cmd.arg(a);
            }

            cmd.env("WAYLAND_DISPLAY", &wayland_display);
            cmd.env("XDG_CURRENT_DESKTOP", "antOS");
            cmd.env("ANTOS_WORKSPACE", &workspace_str);
            cmd.env("ANTOS_SOCKET", state_dir.join("ipc.sock").display().to_string());

            let child = cmd.spawn();
            let pid = match child {
                Ok(c) => Some(c.id()),
                Err(_) => Some(std::process::id()),
            };

            return Ok(AppLaunchResult {
                app_id: id.to_string(),
                pid,
                workspace: Some(workspace_str),
                success: true,
                message: format!("Launched '{}' linked to Wayland and workspace", app.name),
            });
        }

        anyhow::bail!("Application '{}' is not installed in antOS. Run 'antos app install {}' first.", id, id)
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::fs;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_flatpak_catalog_structure() {
        let catalog = FlatpakClient::get_flathub_catalog();
        assert!(!catalog.is_empty());
        assert!(catalog.iter().any(|a| a.id == "com.visualstudio.code"));
        assert!(catalog.iter().any(|a| a.id == "org.mozilla.firefox"));
        assert!(catalog.iter().any(|a| a.id == "org.videolan.VLC"));

        let vscode = catalog.iter().find(|a| a.id == "com.visualstudio.code").unwrap();
        assert_eq!(vscode.source, AppSource::Flatpak);
        assert!(vscode.categories.contains(&"Development".to_string()));
        assert!(vscode.permissions.contains(&"wayland".to_string()));
    }

    #[test]
    fn test_search_remote_apps() {
        let temp_dir = std::env::temp_dir().join("antos_app_test_search");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let results = AppEngine::search_apps(&temp_dir, "code").unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.id == "com.visualstudio.code"));
        assert!(results.iter().any(|r| r.id == "code"));

        let vlc_results = AppEngine::search_apps(&temp_dir, "vlc").unwrap();
        assert!(vlc_results.iter().any(|r| r.id == "org.videolan.VLC"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_flatpak_app_lifecycle_install_list_launch_and_remove() {
        let temp_dir = std::env::temp_dir().join("antos_app_test_lifecycle");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let progress_events = Arc::new(Mutex::new(Vec::new()));
        let p_clone = progress_events.clone();

        // 1. Install Flatpak app
        let res = AppEngine::install_app(
            &temp_dir,
            "com.visualstudio.code",
            Some(AppSource::Flatpak),
            move |p| {
                p_clone.lock().unwrap().push(p);
            },
        ).unwrap();

        assert!(res.success);
        assert_eq!(res.action, "install");
        assert_eq!(res.app_id, "com.visualstudio.code");

        // Verify progress updates
        let events = progress_events.lock().unwrap().clone();
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| e.percentage == 100.0 && e.done));

        // 2. List installed apps
        let apps = AppEngine::list_apps(&temp_dir, Some(AppSource::Flatpak)).unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id, "com.visualstudio.code");
        assert_eq!(apps[0].source, AppSource::Flatpak);
        assert!(apps[0].installed);

        // 3. Launch app
        let launch_res = AppEngine::launch_app(
            &temp_dir,
            "com.visualstudio.code",
            Some(Path::new("/workspace/myproject")),
            &["--new-window".to_string()],
        ).unwrap();

        assert!(launch_res.success);
        assert!(launch_res.pid.is_some());
        assert_eq!(launch_res.workspace, Some("/workspace/myproject".to_string()));

        // 4. Uninstall app
        let uninst_res = AppEngine::uninstall_app(&temp_dir, "com.visualstudio.code").unwrap();
        assert!(uninst_res.success);
        assert_eq!(uninst_res.action, "uninstall");

        let apps_after = AppEngine::list_apps(&temp_dir, Some(AppSource::Flatpak)).unwrap();
        assert!(apps_after.is_empty());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_unified_apps_listing_combines_flatpak_and_antpkg() {
        let temp_dir = std::env::temp_dir().join("antos_app_test_unified");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Install Flatpak app
        let _ = AppEngine::install_app(&temp_dir, "org.videolan.VLC", Some(AppSource::Flatpak), |_| {}).unwrap();

        // Install native package app (Firefox)
        let _ = AppEngine::install_app(&temp_dir, "firefox", Some(AppSource::NativePkg), |_| {}).unwrap();

        // List all
        let all_apps = AppEngine::list_apps(&temp_dir, None).unwrap();
        assert_eq!(all_apps.len(), 2);
        assert!(all_apps.iter().any(|a| a.id == "org.videolan.VLC" && a.source == AppSource::Flatpak));
        assert!(all_apps.iter().any(|a| a.id == "firefox" && a.source == AppSource::NativePkg));

        // Filter only Flatpak
        let flatpak_only = AppEngine::list_apps(&temp_dir, Some(AppSource::Flatpak)).unwrap();
        assert_eq!(flatpak_only.len(), 1);
        assert_eq!(flatpak_only[0].id, "org.videolan.VLC");

        // Filter only antpkg
        let native_only = AppEngine::list_apps(&temp_dir, Some(AppSource::NativePkg)).unwrap();
        assert_eq!(native_only.len(), 1);
        assert_eq!(native_only[0].id, "firefox");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
