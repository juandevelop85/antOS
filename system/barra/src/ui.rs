//! Ensamblado de la ventana principal (superficie Wayland Layer Shell) y
//! cableado de todos los widgets de la barra.

use crate::git_status::query_git_status_async;
use crate::kanban::load_kanban_board_async;
use crate::launcher::{
    launch_desktop_application_async, load_installed_apps_async, render_launcher_results,
    update_launcher_selection,
};
use crate::session::{listen_events, start_session};
use crate::telemetry::query_telemetry_async;
use crate::widgets::{empty_box, make_label, render_error, render_waiting};
use crate::BAR_WIDTH;
use antos_protocol::{is_app_query, match_applications, LauncherAppItem};
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Button, Entry, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::os::unix::net::UnixStream;
use std::rc::Rc;

/// Constructs the Wayland Layer Shell window and widgets.
pub(crate) fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(BAR_WIDTH)
        .build();
    window.add_css_class("fondo");

    // Initialize as a Wayland Layer Shell surface
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_anchor(Edge::Top, true);
    window.set_margin(Edge::Top, 90);

    let frame = GtkBox::new(Orientation::Vertical, 12);
    frame.add_css_class("marco");
    window.set_child(Some(&frame));

    // Top status context bar (Git status + Planner selector + Dry run badge + Kanban toggle)
    let header_bar = GtkBox::new(Orientation::Horizontal, 8);
    header_bar.add_css_class("header-bar");
    header_bar.set_hexpand(true);

    let git_badge = make_label("🌿 checking git...", "badge");
    git_badge.add_css_class("git-badge");
    header_bar.append(&git_badge);

    let ebpf_badge = make_label("🛡️ eBPF", "badge");
    ebpf_badge.add_css_class("ebpf-badge");
    header_bar.append(&ebpf_badge);

    let profiler_badge = make_label("⚡ RSS", "badge");
    profiler_badge.add_css_class("profiler-badge");
    header_bar.append(&profiler_badge);

    let pair_badge = make_label("👥 Coder", "badge");
    pair_badge.add_css_class("pair-badge");
    header_bar.append(&pair_badge);

    let mesh_badge = make_label("🌐 Mesh", "badge");
    mesh_badge.add_css_class("mesh-badge");
    header_bar.append(&mesh_badge);

    let planner_btn = Button::with_label("⚡ local");
    planner_btn.add_css_class("badge-button");
    header_bar.append(&planner_btn);

    let dry_run_btn = Button::with_label("🛡️ live");
    dry_run_btn.add_css_class("badge-button");
    header_bar.append(&dry_run_btn);

    let kanban_btn = Button::with_label("📊 tablero");
    kanban_btn.add_css_class("badge-button");
    header_bar.append(&kanban_btn);

    frame.append(&header_bar);

    // Main intent input entry
    let input = Entry::builder()
        .placeholder_text("¿Qué quieres que antOS haga? (ej. T5.1, libera 3000, crea rama auth)...")
        .build();
    input.add_css_class("intencion");
    frame.append(&input);

    // App launcher floating results container (T25.4)
    let launcher_box = GtkBox::new(Orientation::Vertical, 6);
    launcher_box.add_css_class("launcher-box");
    launcher_box.set_visible(false);
    frame.append(&launcher_box);

    let installed_apps: Rc<RefCell<Vec<LauncherAppItem>>> = Rc::new(RefCell::new(Vec::new()));
    let matching_results: Rc<RefCell<Vec<LauncherAppItem>>> = Rc::new(RefCell::new(Vec::new()));
    let selected_app_index: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));

    // Async load installed apps from daemon (antpkg + flatpak)
    load_installed_apps_async(installed_apps.clone());

    // Quick suggestions pills
    let suggestions_box = GtkBox::new(Orientation::Horizontal, 6);
    suggestions_box.add_css_class("suggestions-box");

    let suggestions = [
        ("📊 Panel", "panel"),
        ("🛡️ eBPF", "muestra las alertas de seguridad ebpf"),
        ("⚡ Profiler", "analiza el rendimiento y hotspots"),
        ("👥 Pair", "inicia pair programming con coder"),
        ("🌐 Mesh", "muestra el estado de la red mesh"),
        ("↩️ undo", "deshacer ultimo cambio"),
    ];

    for (pill_label, pill_query) in suggestions {
        let pill = Button::with_label(pill_label);
        pill.add_css_class("suggestion-pill");
        let input_ref = input.clone();
        let query_str = pill_query.to_string();
        pill.connect_clicked(move |_| {
            input_ref.set_text(&query_str);
            input_ref.emit_activate();
        });
        suggestions_box.append(&pill);
    }
    frame.append(&suggestions_box);

    // Dynamic response and proposals container
    let content = GtkBox::new(Orientation::Vertical, 10);
    frame.append(&content);

    let stream_writer: Rc<RefCell<Option<UnixStream>>> = Rc::new(RefCell::new(None));
    let current_planner: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let dry_run_state: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

    // Connect input search / change for application launcher (T25.4)
    {
        let apps_ref = installed_apps.clone();
        let results_ref = matching_results.clone();
        let selected_ref = selected_app_index.clone();
        let launcher_box_ref = launcher_box.clone();
        let suggestions_ref = suggestions_box.clone();
        let window_clone = window.clone();

        input.connect_changed(move |entry| {
            let text = entry.text().to_string();
            let text_trimmed = text.trim();

            if text_trimmed.is_empty() {
                results_ref.borrow_mut().clear();
                *selected_ref.borrow_mut() = 0;
                empty_box(&launcher_box_ref);
                launcher_box_ref.set_visible(false);
                suggestions_ref.set_visible(true);
                return;
            }

            let apps = apps_ref.borrow();
            if is_app_query(text_trimmed, &apps) {
                let matches = match_applications(text_trimmed, &apps);
                if !matches.is_empty() {
                    *results_ref.borrow_mut() = matches.clone();
                    *selected_ref.borrow_mut() = 0;
                    render_launcher_results(&launcher_box_ref, &matches, 0, window_clone.clone());
                    launcher_box_ref.set_visible(true);
                    suggestions_ref.set_visible(false);
                    return;
                }
            }

            results_ref.borrow_mut().clear();
            *selected_ref.borrow_mut() = 0;
            empty_box(&launcher_box_ref);
            launcher_box_ref.set_visible(false);
            suggestions_ref.set_visible(true);
        });
    }

    // Keyboard navigation for application launcher (Arrow Up / Down, Tab, Return)
    {
        let input_key_controller = gtk4::EventControllerKey::new();
        let results_ref = matching_results.clone();
        let selected_ref = selected_app_index.clone();
        let launcher_box_ref = launcher_box.clone();
        let input_clone = input.clone();
        let window_clone = window.clone();

        input_key_controller.connect_key_pressed(move |_, key, _, _| {
            let results = results_ref.borrow();
            if !results.is_empty() && launcher_box_ref.is_visible() {
                let len = results.len();
                if key == gtk4::gdk::Key::Down {
                    let mut sel = selected_ref.borrow_mut();
                    *sel = (*sel + 1) % len;
                    update_launcher_selection(&launcher_box_ref, *sel);
                    return gtk4::glib::Propagation::Stop;
                } else if key == gtk4::gdk::Key::Up {
                    let mut sel = selected_ref.borrow_mut();
                    *sel = if *sel == 0 { len - 1 } else { *sel - 1 };
                    update_launcher_selection(&launcher_box_ref, *sel);
                    return gtk4::glib::Propagation::Stop;
                } else if key == gtk4::gdk::Key::Tab {
                    let sel = *selected_ref.borrow();
                    if let Some(app) = results.get(sel) {
                        input_clone.set_text(&app.name);
                        input_clone.set_position(-1);
                    }
                    return gtk4::glib::Propagation::Stop;
                } else if key == gtk4::gdk::Key::Return || key == gtk4::gdk::Key::KP_Enter {
                    let sel = *selected_ref.borrow();
                    if let Some(app) = results.get(sel) {
                        launch_desktop_application_async(app.clone(), window_clone.clone());
                        return gtk4::glib::Propagation::Stop;
                    }
                }
            }
            gtk4::glib::Propagation::Proceed
        });
        input.add_controller(input_key_controller);
    }

    // Toggle planner button click
    {
        let planner_ref = current_planner.clone();
        let btn_ref = planner_btn.clone();
        planner_btn.connect_clicked(move |_| {
            let mut p = planner_ref.borrow_mut();
            if p.as_deref() == Some("claude") {
                *p = None;
                btn_ref.set_label("⚡ local");
            } else {
                *p = Some("claude".to_string());
                btn_ref.set_label("🧠 claude");
            }
        });
    }

    // Toggle dry-run button click
    {
        let dry_ref = dry_run_state.clone();
        let btn_ref = dry_run_btn.clone();
        dry_run_btn.connect_clicked(move |_| {
            let mut d = dry_ref.borrow_mut();
            *d = !*d;
            if *d {
                btn_ref.set_label("🛡️ dry-run");
                btn_ref.add_css_class("active");
            } else {
                btn_ref.set_label("🛡️ live");
                btn_ref.remove_css_class("active");
            }
        });
    }

    // Toggle Kanban Board button click
    {
        let content_ref = content.clone();
        let input_ref = input.clone();
        let writer_ref = stream_writer.clone();
        kanban_btn.connect_clicked(move |_| {
            empty_box(&content_ref);
            load_kanban_board_async(content_ref.clone(), input_ref.clone(), writer_ref.clone());
        });
    }

    // Query initial git status and system telemetry
    query_git_status_async(git_badge.clone());
    query_telemetry_async(
        ebpf_badge.clone(),
        profiler_badge.clone(),
        pair_badge.clone(),
        mesh_badge.clone(),
    );

    // Schedule periodic telemetry refresh (every 3 seconds)
    {
        let eb = ebpf_badge.clone();
        let pb = profiler_badge.clone();
        let prb = pair_badge.clone();
        let mb = mesh_badge.clone();
        gtk4::glib::timeout_add_local(std::time::Duration::from_secs(3), move || {
            query_telemetry_async(eb.clone(), pb.clone(), prb.clone(), mb.clone());
            gtk4::glib::ControlFlow::Continue
        });
    }

    // Connect input activation
    {
        let content = content.clone();
        let stream_writer = stream_writer.clone();
        let input_ref = input.clone();
        let planner_ref = current_planner.clone();
        let dry_ref = dry_run_state.clone();
        let results_activate_ref = matching_results.clone();
        let selected_activate_ref = selected_app_index.clone();
        let launcher_box_activate_ref = launcher_box.clone();
        let window_activate_ref = window.clone();

        input.connect_activate(move |entry| {
            let text = entry.text().to_string();
            let text_trimmed = text.trim();
            if text_trimmed.is_empty() {
                return;
            }

            // Check if application launcher is active with a selection
            {
                let results = results_activate_ref.borrow();
                if !results.is_empty() && launcher_box_activate_ref.is_visible() {
                    let sel = *selected_activate_ref.borrow();
                    if let Some(app) = results.get(sel) {
                        launch_desktop_application_async(app.clone(), window_activate_ref.clone());
                        return;
                    }
                }
            }

            if text_trimmed == "panel" || text_trimmed == "board" || text_trimmed == "tablero" {
                empty_box(&content);
                load_kanban_board_async(content.clone(), input_ref.clone(), stream_writer.clone());
                return;
            }

            input_ref.set_sensitive(false);
            empty_box(&content);

            let selected_planner = planner_ref.borrow().clone();
            let is_dry = *dry_ref.borrow();

            match start_session(text_trimmed, selected_planner, is_dry) {
                Ok((stream, events)) => {
                    *stream_writer.borrow_mut() = Some(stream);
                    render_waiting(&content);
                    listen_events(
                        events,
                        content.clone(),
                        stream_writer.clone(),
                        input_ref.clone(),
                    );
                }
                Err(err) => render_error(&content, &err),
            }
        });
    }

    // Escape key closes window without confirming; Super+A opens panel
    let key_controller = gtk4::EventControllerKey::new();
    let window_ref = window.clone();
    let content_ref = content.clone();
    let input_ref = input.clone();
    let writer_ref = stream_writer.clone();
    key_controller.connect_key_pressed(move |_, key, _, modifier| {
        if key == gtk4::gdk::Key::Escape {
            window_ref.close();
            return gtk4::glib::Propagation::Stop;
        }
        if (key == gtk4::gdk::Key::a || key == gtk4::gdk::Key::A)
            && modifier.contains(gtk4::gdk::ModifierType::SUPER_MASK)
        {
            empty_box(&content_ref);
            load_kanban_board_async(content_ref.clone(), input_ref.clone(), writer_ref.clone());
            return gtk4::glib::Propagation::Stop;
        }
        gtk4::glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    window.present();

    // Check optional CLI arguments e.g. --panel or --intencion "..."
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--panel" || a == "--board") {
        load_kanban_board_async(content.clone(), input.clone(), stream_writer.clone());
    } else {
        let initial_intent: Option<String> = args
            .iter()
            .position(|a| a == "--intencion" || a == "--intent")
            .and_then(|i| args.get(i + 1).cloned());

        if let Some(text) = initial_intent {
            input.set_text(&text);
            input.emit_activate();
        }
    }
}
