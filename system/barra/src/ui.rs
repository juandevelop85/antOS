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
use std::cell::{Cell, RefCell};
use std::os::unix::net::UnixStream;
use std::rc::Rc;

/// Muestra u oculta la ventana entera (T30.8, seguimiento: reemplaza la
/// franja colapsada por un icono real de bandeja SNI — ver `tray.rs` — así
/// que, colapsada, la barra ya no dibuja nada propio, igual que la ventana
/// de Claude en la barra de menú de macOS). Expandida, retiene el foco de
/// teclado solo mientras el usuario interactúa (`KeyboardMode::OnDemand`,
/// no `Exclusive`) — el `EventControllerFocus` de la ventana dispara el
/// colapso automático al soltarlo (ver el comentario junto a su cableado,
/// más abajo, sobre por qué no se usa `is-active`).
fn set_bar_expanded(
    window: &ApplicationWindow,
    input: &Entry,
    is_expanded: &Rc<Cell<bool>>,
    expand: bool,
) {
    if is_expanded.get() == expand {
        return;
    }
    is_expanded.set(expand);
    if expand {
        window.set_keyboard_mode(KeyboardMode::OnDemand);
        window.present();
        input.grab_focus();
    } else {
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_visible(false);
    }
}

/// Constructs the Wayland Layer Shell window and widgets.
pub(crate) fn build_ui(app: &Application) {
    // La barra arranca oculta (ver más abajo) y solo se muestra al activar
    // el icono de bandeja — sin ninguna ventana visible, `GApplication`
    // daría por terminada la activación y podría salir. `hold()` la
    // mantiene viva mientras dure el proceso; el guardián se filtra a
    // propósito (mismo motivo que el `Handle` de `tray::spawn`).
    std::mem::forget(app.hold());

    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(BAR_WIDTH)
        .build();
    window.add_css_class("fondo");

    // Initialize as a Wayland Layer Shell surface.
    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_anchor(Edge::Top, true);
    window.set_margin(Edge::Top, 90);

    let is_expanded: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    // Icono de bandeja SNI (T30.8, T32.4): canal reactivo GLib sin sondeo activo.
    let (tray_tx, tray_rx) = gtk4::glib::MainContext::channel::<()>(gtk4::glib::Priority::default());
    crate::tray::spawn(tray_tx);

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

    // Click en el icono de bandeja → alterna expandir/colapsar (T30.8, T32.4).
    // `tray::spawn` envía el evento a través del canal reactivo GLib (`tray_rx.attach`),
    // eliminando por completo el sondeo periódico por temporizador.
    {
        let window_ref = window.clone();
        let input_ref = input.clone();
        let is_expanded_ref = is_expanded.clone();
        tray_rx.attach(None, move |()| {
            set_bar_expanded(&window_ref, &input_ref, &is_expanded_ref, !is_expanded_ref.get());
            gtk4::glib::ControlFlow::Continue
        });
    }

    // Perder el foco de teclado estando expandida → colapsar sola (T30.8).
    //
    // `is-active`/`connect_is_active_notify` (probado primero) resultó no
    // fiable: refleja el estado "activated" de un `xdg_toplevel`, y una
    // superficie `zwlr_layer_surface_v1` no tiene ese estado en el
    // protocolo — no hay ningún evento que lo dispare, así que la barra se
    // quedaba expandida indefinidamente. `EventControllerFocus` en cambio
    // se basa en los eventos reales `wl_keyboard::enter`/`leave` de la
    // superficie, que sí le llegan a un layer surface igual que a
    // cualquier otro.
    {
        let window_ref = window.clone();
        let input_ref = input.clone();
        let is_expanded_ref = is_expanded.clone();
        let focus_controller = gtk4::EventControllerFocus::new();
        focus_controller.connect_leave(move |_| {
            if is_expanded_ref.get() {
                set_bar_expanded(&window_ref, &input_ref, &is_expanded_ref, false);
            }
        });
        window.add_controller(focus_controller);
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

    // Escape key collapses the bar instead of closing the window (T30.8):
    // `system/desktop/autostart` no reinicia `antos-barra` si el proceso
    // termina, así que cerrar la ventana aquí la mataba sin más. Super+A
    // sigue abriendo el tablero, pero ahora solo llega mientras la barra
    // está expandida — ver "Efecto secundario conocido" en T30.8.
    let key_controller = gtk4::EventControllerKey::new();
    let window_ref = window.clone();
    let content_ref = content.clone();
    let input_ref = input.clone();
    let writer_ref = stream_writer.clone();
    let is_expanded_ref = is_expanded.clone();
    key_controller.connect_key_pressed(move |_, key, _, modifier| {
        if key == gtk4::gdk::Key::Escape {
            set_bar_expanded(&window_ref, &input_ref, &is_expanded_ref, false);
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

    // La barra arranca oculta (T30.8, seguimiento): sin `--panel`/
    // `--intencion`, el único punto de entrada visible es el icono de
    // bandeja — no hay `window.present()` incondicional aquí.
    //
    // Check optional CLI arguments e.g. --panel or --intencion "..."
    let args: Vec<String> = std::env::args().collect();
    let initial_intent: Option<String> = args
        .iter()
        .position(|a| a == "--intencion" || a == "--intent")
        .and_then(|i| args.get(i + 1).cloned());
    let wants_panel = args.iter().any(|a| a == "--panel" || a == "--board");

    if wants_panel || initial_intent.is_some() {
        set_bar_expanded(&window, &input, &is_expanded, true);
    }
    if wants_panel {
        load_kanban_board_async(content.clone(), input.clone(), stream_writer.clone());
    } else if let Some(text) = initial_intent {
        input.set_text(&text);
        input.emit_activate();
    }
}
