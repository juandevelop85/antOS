//! Lanzador de aplicaciones gráficas embebido en la barra (T25.4): resultados
//! de búsqueda, catálogo de aplicaciones instaladas y despacho de lanzamiento.

use crate::session::run_offthread;
use crate::socket_path;
use crate::widgets::{empty_box, make_label, truncate_str};
use antos_protocol::{inject_workspace_args, Event, LauncherAppItem, Request};
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box as GtkBox, GestureClick, Image, Orientation};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::rc::Rc;

/// Renders the launcher match cards inside the launcher box.
pub(crate) fn render_launcher_results(
    launcher_box: &GtkBox,
    items: &[LauncherAppItem],
    selected_idx: usize,
    window: ApplicationWindow,
) {
    empty_box(launcher_box);

    for (i, app) in items.iter().take(6).enumerate() {
        let card = GtkBox::new(Orientation::Horizontal, 12);
        card.add_css_class("launcher-card");
        if i == selected_idx {
            card.add_css_class("selected");
        }

        // Icon or emoji avatar
        let icon_widget = create_app_icon_widget(app);
        card.append(&icon_widget);

        // Details (Title + Subtitle)
        let details_box = GtkBox::new(Orientation::Vertical, 2);
        details_box.set_hexpand(true);

        let title_box = GtkBox::new(Orientation::Horizontal, 8);
        let title = make_label(&app.name, "launcher-title");
        title_box.append(&title);

        let source_badge = make_label(&format!("({})", app.source), "launcher-source");
        title_box.append(&source_badge);

        if app.is_editor {
            let ws_badge = make_label("📁 workspace", "launcher-ws-badge");
            title_box.append(&ws_badge);
        }
        details_box.append(&title_box);

        let desc_text = app
            .generic_name
            .as_deref()
            .or(app.comment.as_deref())
            .unwrap_or(&app.exec);
        let subtitle = make_label(&truncate_str(desc_text, 60), "launcher-subtitle");
        details_box.append(&subtitle);

        card.append(&details_box);

        // Shortcut hint on right
        let hint_label = if i == selected_idx {
            make_label("↵ Enter", "launcher-hint active")
        } else {
            make_label("", "launcher-hint")
        };
        card.append(&hint_label);

        // Clicking a card directly launches it
        let gesture = GestureClick::new();
        let app_clone = app.clone();
        let window_clone = window.clone();
        gesture.connect_released(move |_, _, _, _| {
            launch_desktop_application_async(app_clone.clone(), window_clone.clone());
        });
        card.add_controller(gesture);

        launcher_box.append(&card);
    }
}

/// Updates the CSS classes for selection in the launcher cards.
pub(crate) fn update_launcher_selection(launcher_box: &GtkBox, selected_idx: usize) {
    let mut current = launcher_box.first_child();
    let mut index = 0;
    while let Some(child) = current {
        if index == selected_idx {
            child.add_css_class("selected");
        } else {
            child.remove_css_class("selected");
        }
        current = child.next_sibling();
        index += 1;
    }
}

/// Creates an icon widget for an application.
fn create_app_icon_widget(app: &LauncherAppItem) -> GtkBox {
    let box_widget = GtkBox::new(Orientation::Horizontal, 0);
    box_widget.add_css_class("launcher-icon-box");

    // Try icon path if it exists on disk
    if let Some(ref path_str) = app.icon_path {
        let p = std::path::Path::new(path_str);
        if p.exists() {
            let img = Image::from_file(p);
            img.set_pixel_size(28);
            box_widget.append(&img);
            return box_widget;
        }
    }

    // Try system themed icon
    if let Some(ref icon_name) = app.icon {
        let img = Image::from_icon_name(icon_name);
        img.set_pixel_size(28);
        box_widget.append(&img);
        return box_widget;
    }

    // Fallback emoji icon based on category
    let emoji = if app.is_editor {
        "💻"
    } else if app
        .categories
        .iter()
        .any(|c| c == "Network" || c == "WebBrowser")
    {
        "🌐"
    } else if app.categories.iter().any(|c| c == "TerminalEmulator") {
        "⚡"
    } else {
        "📦"
    };

    let label = make_label(emoji, "launcher-icon-emoji");
    box_widget.append(&label);
    box_widget
}

/// Dispatches the launch request to the daemon with automatic workspace context injection.
pub(crate) fn launch_desktop_application_async(app: LauncherAppItem, window: ApplicationWindow) {
    run_offthread(
        move || {
            let workspace = std::env::var("ANTOS_WORKSPACE").unwrap_or_else(|_| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            });

            let args = inject_workspace_args(&app, &workspace, &[]);

            let path = socket_path();
            if let Ok(mut stream) = UnixStream::connect(&path) {
                let req = Request::LaunchApp {
                    id: app.id.clone(),
                    workspace: Some(workspace.clone()),
                    args: args.clone(),
                };

                if let Ok(json) = serde_json::to_string(&req) {
                    let _ = writeln!(stream, "{json}");
                    let _ = stream.flush();
                }
            } else {
                // Local fallback spawn if daemon socket is unreachable
                let wayland_display =
                    std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());
                let first_token = app.exec.split_whitespace().next().unwrap_or(&app.id);
                let mut cmd = std::process::Command::new(first_token);
                for a in &args {
                    cmd.arg(a);
                }
                cmd.env("WAYLAND_DISPLAY", wayland_display);
                cmd.env("XDG_CURRENT_DESKTOP", "antOS");
                cmd.env("ANTOS_WORKSPACE", &workspace);
                let _ = cmd.spawn();
            }
        },
        move |()| window.close(),
    );
}

/// Loads installed applications from antpkg and Flatpak via the antOS daemon.
pub(crate) fn load_installed_apps_async(installed_apps: Rc<RefCell<Vec<LauncherAppItem>>>) {
    run_offthread(
        || -> Vec<LauncherAppItem> {
            let path = socket_path();
            let mut apps = Vec::new();

            if let Ok(mut stream) = UnixStream::connect(&path) {
                // 1. Query antpkg desktop apps
                let req = Request::ListDesktopApps;
                if let Ok(json) = serde_json::to_string(&req) {
                    let _ = writeln!(stream, "{json}");
                    let _ = stream.flush();

                    let mut reader = BufReader::new(&stream);
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_ok() {
                        if let Ok(Event::DesktopAppList(list)) =
                            serde_json::from_str::<Event>(line.trim())
                        {
                            for item in list {
                                apps.push(LauncherAppItem::from_desktop_summary(&item));
                            }
                        }
                    }
                }

                // 2. Query general apps (Flatpak / Host)
                if let Ok(mut stream2) = UnixStream::connect(&path) {
                    let req_apps = Request::ListApps { source: None };
                    if let Ok(json) = serde_json::to_string(&req_apps) {
                        let _ = writeln!(stream2, "{json}");
                        let _ = stream2.flush();

                        let mut reader2 = BufReader::new(&stream2);
                        let mut line2 = String::new();
                        if reader2.read_line(&mut line2).is_ok() {
                            if let Ok(Event::AppList(list)) =
                                serde_json::from_str::<Event>(line2.trim())
                            {
                                for item in list {
                                    if !apps.iter().any(|a| a.id == item.id) {
                                        apps.push(LauncherAppItem::from_desktop_app(&item));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // If daemon returned no apps (e.g. initial run or offline), provide curated defaults
            if apps.is_empty() {
                apps = get_default_catalog_apps();
            }

            apps
        },
        move |apps| {
            *installed_apps.borrow_mut() = apps;
        },
    );
}

/// Fallback catalog apps for demo and offline execution.
fn get_default_catalog_apps() -> Vec<LauncherAppItem> {
    vec![
        LauncherAppItem::new(
            "firefox",
            "Firefox",
            Some("Web Browser".to_string()),
            Some("Navegador web libre y seguro".to_string()),
            "firefox %u",
            Some("firefox".to_string()),
            None,
            vec!["Network".to_string(), "WebBrowser".to_string()],
            "antpkg",
        ),
        LauncherAppItem::new(
            "chromium",
            "Chromium",
            Some("Web Browser".to_string()),
            Some("Navegador Chromium portable".to_string()),
            "chromium",
            Some("chromium".to_string()),
            None,
            vec!["Network".to_string(), "WebBrowser".to_string()],
            "antpkg",
        ),
        LauncherAppItem::new(
            "vscode",
            "Visual Studio Code",
            Some("Code Editor".to_string()),
            Some("Editor de código extensible".to_string()),
            "code --ozone-platform=wayland %F",
            Some("vscode".to_string()),
            None,
            vec!["Development".to_string(), "IDE".to_string()],
            "antpkg",
        ),
        LauncherAppItem::new(
            "zed",
            "Zed",
            Some("Code Editor".to_string()),
            Some("Editor de código ultrarrápido en Rust".to_string()),
            "zed",
            Some("zed".to_string()),
            None,
            vec!["Development".to_string(), "IDE".to_string()],
            "antpkg",
        ),
        LauncherAppItem::new(
            "cursor",
            "Cursor",
            Some("AI Code Editor".to_string()),
            Some("Editor de código potenciado por IA".to_string()),
            "cursor --ozone-platform=wayland",
            Some("cursor".to_string()),
            None,
            vec!["Development".to_string(), "IDE".to_string()],
            "antpkg",
        ),
        LauncherAppItem::new(
            "postman",
            "Postman",
            Some("API Platform".to_string()),
            Some("Suite de pruebas de API REST".to_string()),
            "postman",
            Some("postman".to_string()),
            None,
            vec!["Development".to_string()],
            "antpkg",
        ),
        LauncherAppItem::new(
            "alacritty",
            "Alacritty",
            Some("Terminal".to_string()),
            Some("Terminal acelerado por GPU".to_string()),
            "alacritty",
            Some("alacritty".to_string()),
            None,
            vec!["System".to_string(), "TerminalEmulator".to_string()],
            "antpkg",
        ),
    ]
}
