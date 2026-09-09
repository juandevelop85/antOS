//! antOS Wayland/GTK4 Layer Shell Intent Bar & Agent Mission Center (T4.1 & T4.2).
//!
//! Provides the primary desktop interface for developer intentions, semantic git context,
//! rich diff inspections, multi-agent antFlow state visualizations, and the Kanban Ticket Board (`Super + A`).

extern crate antos_protocol as antos_protocol;

use antos_protocol::{
    inject_workspace_args, is_app_query, match_applications, AgentRole, Event, FlowState, FlowTask,
    GitRepoStatus, LauncherAppItem, Line, Proposal, Request, TicketStatus, TicketSummary, Tier,
};
use gtk4::gdk::Display;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry, GestureClick,
    Image, Label, Orientation, PolicyType, ScrolledWindow,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};

const BAR_WIDTH: i32 = 820;

/// Resolves the antOS daemon UNIX socket path.
fn socket_path() -> PathBuf {
    if let Some(v) = std::env::var_os("ANTOS_SOCKET").or_else(|| std::env::var_os("SYSO_SOCKET")) {
        return PathBuf::from(v);
    }
    let state_dir = std::env::var_os("ANTOS_STATE")
        .or_else(|| std::env::var_os("SYSO_STATE"))
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

    app.connect_activate(build_ui);

    app.run_with_args::<&str>(&[]);
}

/// Constructs the Wayland Layer Shell window and widgets.
fn build_ui(app: &Application) {
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

/// Asynchronously queries the active repository Git status.
fn query_git_status_async(git_badge: Label) {
    std::thread::spawn(move || {
        let path = socket_path();
        let Ok(mut stream) = UnixStream::connect(&path) else {
            gtk4::glib::idle_add_local(move || {
                git_badge.set_text("● offline");
                git_badge.add_css_class("offline");
                gtk4::glib::ControlFlow::Break
            });
            return;
        };

        let current_dir = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into());

        let req = Request::QueryGitStatus {
            workspace_path: current_dir,
        };

        if let Ok(json) = serde_json::to_string(&req) {
            let _ = writeln!(stream, "{json}");
            let _ = stream.flush();

            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok() {
                if let Ok(event) = serde_json::from_str::<Event>(line.trim()) {
                    if let Event::GitStatus(status) = event {
                        gtk4::glib::idle_add_local(move || {
                            update_git_badge(&git_badge, &status);
                            gtk4::glib::ControlFlow::Break
                        });
                        return;
                    }
                }
            }
        }

        gtk4::glib::idle_add_local(move || {
            git_badge.set_text("🌿 antOS");
            gtk4::glib::ControlFlow::Break
        });
    });
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

/// Asynchronously queries consolidated system telemetry (eBPF, Profiler, Pair, Mesh).
fn query_telemetry_async(
    ebpf_badge: Label,
    profiler_badge: Label,
    pair_badge: Label,
    mesh_badge: Label,
) {
    std::thread::spawn(move || {
        let path = socket_path();
        let Ok(mut stream) = UnixStream::connect(&path) else {
            return;
        };

        let req = Request::QueryBarraTelemetry;
        if let Ok(json) = serde_json::to_string(&req) {
            let _ = writeln!(stream, "{json}");
            let _ = stream.flush();

            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok() {
                if let Ok(Event::BarraTelemetryStatus(t)) =
                    serde_json::from_str::<Event>(line.trim())
                {
                    let eb = ebpf_badge.clone();
                    let pb = profiler_badge.clone();
                    let prb = pair_badge.clone();
                    let mb = mesh_badge.clone();
                    gtk4::glib::idle_add_local(move || {
                        update_telemetry_badges(&eb, &pb, &prb, &mb, &t);
                        gtk4::glib::ControlFlow::Break
                    });
                }
            }
        }
    });
}

fn update_telemetry_badges(
    ebpf_badge: &Label,
    profiler_badge: &Label,
    pair_badge: &Label,
    mesh_badge: &Label,
    telemetry: &antos_protocol::BarraTelemetry,
) {
    // 1. eBPF LSM Guard
    if telemetry.ebpf_violations_count > 0 {
        ebpf_badge.set_text(&format!("🛡️ eBPF: !{}", telemetry.ebpf_violations_count));
        ebpf_badge.remove_css_class("active");
        ebpf_badge.add_css_class("alert");
    } else if telemetry.ebpf_lsm_active {
        ebpf_badge.set_text("🛡️ LSM");
        ebpf_badge.remove_css_class("alert");
        ebpf_badge.add_css_class("active");
    } else {
        ebpf_badge.set_text("🛡️ eBPF");
        ebpf_badge.remove_css_class("alert");
        ebpf_badge.remove_css_class("active");
    }

    // 2. Profiler Memory & CPU
    let rss_mb = telemetry.profiler_rss_bytes as f64 / (1024.0 * 1024.0);
    profiler_badge.set_text(&format!("⚡ {:.0}M", rss_mb));
    if telemetry.profiler_cpu_percent > 50.0 {
        profiler_badge.add_css_class("alert");
    } else {
        profiler_badge.remove_css_class("alert");
    }

    // 3. Pair Programming / Coder
    if let Some(ref session) = telemetry.active_pair_session {
        pair_badge.set_text(&format!("👥 Coder ({session})"));
        pair_badge.add_css_class("active");
    } else {
        pair_badge.set_text("👥 Coder");
        pair_badge.remove_css_class("active");
    }

    // 4. antMesh P2P Nodes
    if telemetry.mesh_peers_count > 0 {
        mesh_badge.set_text(&format!("🌐 {} peers", telemetry.mesh_peers_count));
        mesh_badge.add_css_class("active");
    } else {
        mesh_badge.set_text("🌐 Mesh");
        mesh_badge.remove_css_class("active");
    }
}

/// Asynchronously fetches tickets and active tasks to construct the Kanban Board UI.
fn load_kanban_board_async(
    content: GtkBox,
    input: Entry,
    stream_writer: Rc<RefCell<Option<UnixStream>>>,
) {
    content.append(&make_label(
        "Cargando Centro de Control y Tablero...",
        "radio",
    ));

    std::thread::spawn(move || {
        let path = socket_path();
        let current_dir = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into());

        let mut tickets = Vec::new();
        let mut flows = Vec::new();

        if let Ok(mut stream) = UnixStream::connect(&path) {
            // 1. Fetch tickets
            let req_tickets = Request::ListTickets {
                workspace_path: current_dir.clone(),
            };
            if let Ok(json) = serde_json::to_string(&req_tickets) {
                let _ = writeln!(stream, "{json}");
                let _ = stream.flush();

                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() {
                    if let Ok(Event::TicketList(list)) = serde_json::from_str::<Event>(line.trim())
                    {
                        tickets = list;
                    }
                }
            }

            // 2. Fetch flows
            let req_flows = Request::ListFlows {
                workspace_path: current_dir,
            };
            if let Ok(json) = serde_json::to_string(&req_flows) {
                let _ = writeln!(stream, "{json}");
                let _ = stream.flush();

                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() {
                    if let Ok(Event::FlowList(list)) = serde_json::from_str::<Event>(line.trim()) {
                        flows = list;
                    }
                }
            }
        }

        gtk4::glib::idle_add_local(move || {
            empty_box(&content);
            render_kanban_view(
                &content,
                &tickets,
                &flows,
                input.clone(),
                stream_writer.clone(),
            );
            gtk4::glib::ControlFlow::Break
        });
    });
}

/// Renders the 4-column visual Kanban board and active agent monitor.
fn render_kanban_view(
    content: &GtkBox,
    tickets: &[TicketSummary],
    flows: &[FlowTask],
    input: Entry,
    stream_writer: Rc<RefCell<Option<UnixStream>>>,
) {
    let sheet = GtkBox::new(Orientation::Vertical, 12);
    sheet.add_css_class("hoja");
    sheet.add_css_class("flow");

    // Title and Agent Monitor Graph
    sheet.append(&make_label(
        "antOS · CENTRO DE CONTROL DE AGENTES & TABLERO KANBAN",
        "etiqueta",
    ));

    let agent_bar = GtkBox::new(Orientation::Horizontal, 12);
    agent_bar.add_css_class("agent-monitor-bar");

    let roles = [
        ("📐 Arquitecto", AgentRole::Architect),
        ("💻 Coder", AgentRole::Coder),
        ("🧪 QA / Tester", AgentRole::QA),
        ("🛡️ Auditor", AgentRole::Auditor),
    ];

    for (role_title, role) in roles {
        let active = flows.iter().any(|f| f.current_role == Some(role));
        let badge = make_label(role_title, "agent-badge");
        if active {
            badge.add_css_class("active");
        } else {
            badge.add_css_class("idle");
        }
        agent_bar.append(&badge);
    }
    sheet.append(&agent_bar);

    // 4 Kanban Columns
    let columns_container = GtkBox::new(Orientation::Horizontal, 10);
    columns_container.add_css_class("kanban-columns");
    columns_container.set_homogeneous(true);

    let col_backlog = create_kanban_column(
        "⏳ BACKLOG",
        tickets.iter().filter(|t| t.status == TicketStatus::Pending),
        input.clone(),
        stream_writer.clone(),
        true,
    );
    let col_progress = create_kanban_column(
        "🔄 EN PROGRESO",
        tickets
            .iter()
            .filter(|t| t.status == TicketStatus::InProgress),
        input.clone(),
        stream_writer.clone(),
        false,
    );
    let col_review = create_kanban_column(
        "🔍 EN REVISIÓN",
        tickets
            .iter()
            .filter(|t| t.status == TicketStatus::InReview),
        input.clone(),
        stream_writer.clone(),
        false,
    );
    let col_done = create_kanban_column(
        "✅ COMPLETADO",
        tickets
            .iter()
            .filter(|t| t.status == TicketStatus::Completed),
        input.clone(),
        stream_writer.clone(),
        false,
    );

    columns_container.append(&col_backlog);
    columns_container.append(&col_progress);
    columns_container.append(&col_review);
    columns_container.append(&col_done);

    let scrolled_kanban = ScrolledWindow::builder()
        .child(&columns_container)
        .min_content_height(280)
        .max_content_height(420)
        .propagate_natural_height(true)
        .build();
    sheet.append(&scrolled_kanban);

    content.append(&sheet);
}

fn create_kanban_column<'a, I>(
    title: &str,
    tickets: I,
    input: Entry,
    stream_writer: Rc<RefCell<Option<UnixStream>>>,
    show_dispatch_btn: bool,
) -> GtkBox
where
    I: Iterator<Item = &'a TicketSummary>,
{
    let col = GtkBox::new(Orientation::Vertical, 8);
    col.add_css_class("kanban-column");

    let header = make_label(title, "kanban-column-header");
    col.append(&header);

    let items_box = GtkBox::new(Orientation::Vertical, 6);

    for t in tickets {
        let card = GtkBox::new(Orientation::Vertical, 4);
        card.add_css_class("kanban-card");

        let id_lbl = make_label(&format!("{} · {}", t.id, t.phase), "kanban-card-id");
        let title_lbl = make_label(&t.title, "kanban-card-title");
        card.append(&id_lbl);
        card.append(&title_lbl);

        if show_dispatch_btn {
            let dispatch_btn = Button::with_label("🚀 Despachar");
            dispatch_btn.add_css_class("dispatch-btn");
            let tid = t.id.clone();
            let input_ref = input.clone();
            let writer_ref = stream_writer.clone();
            dispatch_btn.connect_clicked(move |_| {
                input_ref.set_text(&format!("desarrolla ticket {tid}"));
                if let Some(stream) = writer_ref.borrow_mut().as_mut() {
                    let current_dir = std::env::current_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| ".".into());
                    let req = Request::StartFlow {
                        workspace_path: current_dir,
                        ticket_id: tid.clone(),
                    };
                    if let Ok(json) = serde_json::to_string(&req) {
                        let _ = writeln!(stream, "{json}");
                        let _ = stream.flush();
                    }
                }
                input_ref.emit_activate();
            });
            card.append(&dispatch_btn);
        }

        items_box.append(&card);
    }

    col.append(&items_box);
    col
}

/// Connects to the daemon socket and starts the reader thread.
fn start_session(
    text: &str,
    planner: Option<String>,
    dry_run: bool,
) -> Result<(UnixStream, Receiver<Event>), String> {
    let path = socket_path();
    let stream = UnixStream::connect(&path).map_err(|e| {
        format!(
            "no hay demonio antOS en {}: {e}\nArráncalo en otra terminal con: antos demonio",
            path.display()
        )
    })?;

    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let request = Request::Intent {
        text: text.to_string(),
        planner,
        dry_run,
    };

    writeln!(
        writer,
        "{}",
        serde_json::to_string(&request).map_err(|e| e.to_string())?
    )
    .map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;

    let (sender, receiver) = channel();
    let reader_stream = stream.try_clone().map_err(|e| e.to_string())?;

    std::thread::spawn(move || {
        let mut buffer = BufReader::new(reader_stream);
        loop {
            let mut line = String::new();
            match buffer.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Ok(event) = serde_json::from_str::<Event>(line.trim()) {
                        if sender.send(event).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok((writer, receiver))
}

fn listen_events(
    events: Receiver<Event>,
    content: GtkBox,
    stream_writer: Rc<RefCell<Option<UnixStream>>>,
    input: Entry,
) {
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(35), move || {
        while let Ok(event) = events.try_recv() {
            match event {
                Event::Start { .. } => {}
                Event::Note(text) => {
                    empty_box(&content);
                    content.append(&make_label(&format!("antOS: {text}"), "radio"));
                }
                Event::Proposal(proposal) => {
                    empty_box(&content);
                    render_proposal(&content, &proposal, stream_writer.clone());
                }
                Event::FlowStatus(Some(task)) => {
                    empty_box(&content);
                    render_flow_task(&content, &task, stream_writer.clone());
                }
                Event::FlowTransition {
                    new_state,
                    role,
                    detail,
                    model,
                    ..
                } => {
                    let role_label = role.map(|r| r.name()).unwrap_or("System");
                    let model_suffix = model
                        .as_deref()
                        .map(|m| format!(" · {m}"))
                        .unwrap_or_default();
                    let transition_text = format!(
                        "{}: {} [{}{}]",
                        new_state.label(),
                        detail,
                        role_label,
                        model_suffix
                    );
                    content.append(&make_label(&transition_text, "paso"));
                }
                Event::Output(text) => content.append(&make_label(&text, "paso")),
                Event::Result(result) => {
                    let class = if result.ok { "ok" } else { "error" };
                    content.append(&make_label(&result.message, class));
                    input.set_sensitive(true);
                    input.set_text("");
                }
                Event::Error(err_msg) => {
                    render_error(&content, &err_msg);
                    input.set_sensitive(true);
                }
                _ => {}
            }
        }
        gtk4::glib::ControlFlow::Continue
    });
}

fn render_proposal(
    content: &GtkBox,
    proposal: &Proposal,
    stream_writer: Rc<RefCell<Option<UnixStream>>>,
) {
    let sheet = GtkBox::new(Orientation::Vertical, 10);
    sheet.add_css_class("hoja");
    sheet.add_css_class(level_css_class(proposal.tier));

    // 1 · Plan steps
    sheet.append(&make_label("PLAN", "etiqueta"));
    for (i, step) in proposal.plan.steps.iter().enumerate() {
        let args = step
            .args
            .iter()
            .map(|(k, v)| format!("{k}={}", truncate_str(v, 36)))
            .collect::<Vec<_>>()
            .join(" ");
        sheet.append(&make_label(
            &format!("{}. {}  {args}", i + 1, step.capability),
            "paso",
        ));
    }

    // 2 · Syntax highlighted Diff viewer
    sheet.append(&make_label("CAMBIOS", "etiqueta"));
    let diff_list = GtkBox::new(Orientation::Vertical, 0);
    for line in &proposal.changes {
        let (text, css_class) = match line {
            Line::Info(t) => (t.clone(), "info"),
            Line::Add(t) => (format!("+{t}"), "mas"),
            Line::Del(t) => (format!("-{t}"), "menos"),
        };
        let l = make_label(&text, "diff");
        l.add_css_class(css_class);
        diff_list.append(&l);
    }
    let scrolled = ScrolledWindow::builder()
        .child(&diff_list)
        .min_content_height(120)
        .max_content_height(340)
        .propagate_natural_height(true)
        .hscrollbar_policy(PolicyType::Automatic)
        .build();
    sheet.append(&scrolled);

    // 3 · Blast radius breakdown
    sheet.append(&make_label("RADIO DE IMPACTO", "etiqueta"));
    for (name, paths) in [
        ("escribe", &proposal.blast_radius.writes),
        ("borra", &proposal.blast_radius.deletes),
        ("lee", &proposal.blast_radius.reads),
        ("SISTEMA", &proposal.blast_radius.system),
        ("red", &proposal.blast_radius.network),
    ] {
        if !paths.is_empty() {
            sheet.append(&make_label(
                &format!("{name:9} {}", truncate_str(&paths.join(", "), 70)),
                "radio",
            ));
        }
    }

    let tier_label = make_label(
        &format!(
            "nivel     {} — {}",
            proposal.tier.label(),
            proposal.reasons.join("; ")
        ),
        "nivel",
    );
    tier_label.add_css_class(level_css_class(proposal.tier));
    sheet.append(&tier_label);

    sheet.append(&make_label(
        &format!(
            "recinto   {} — {}",
            proposal.enclosure.engine, proposal.enclosure.guarantees
        ),
        "radio",
    ));

    content.append(&sheet);

    // 4 · Decision buttons
    if proposal.tier == Tier::Auto || proposal.dry_run {
        return;
    }

    let button_box = GtkBox::new(Orientation::Horizontal, 10);
    button_box.set_halign(Align::End);

    let discard_btn = Button::with_label("Descartar");
    discard_btn.add_css_class("descartar");
    let approve_btn = Button::with_label("Aprobar");
    approve_btn.add_css_class("aprobar");

    for (btn, decision) in [(&discard_btn, false), (&approve_btn, true)] {
        let stream_ref = stream_writer.clone();
        let box_ref = button_box.clone();
        btn.connect_clicked(move |_| {
            if let Some(stream) = stream_ref.borrow_mut().as_mut() {
                let response = Request::Approval(decision);
                if let Ok(json) = serde_json::to_string(&response) {
                    let _ = writeln!(stream, "{json}");
                    let _ = stream.flush();
                }
            }
            box_ref.set_sensitive(false);
        });
    }

    button_box.append(&discard_btn);
    button_box.append(&approve_btn);
    content.append(&button_box);
}

fn render_flow_task(
    content: &GtkBox,
    task: &FlowTask,
    stream_writer: Rc<RefCell<Option<UnixStream>>>,
) {
    let sheet = GtkBox::new(Orientation::Vertical, 10);
    sheet.add_css_class("hoja");
    sheet.add_css_class("flow");

    sheet.append(&make_label("antFlow · TAREA DE AGENTES", "etiqueta"));
    sheet.append(&make_label(
        &format!(
            "Ticket: {} | Estado: {}",
            task.ticket_id,
            task.state.label()
        ),
        "nivel",
    ));

    if let Some(role) = task.current_role {
        sheet.append(&make_label(
            &format!("Rol Activo: {} ({})", role.name(), role.description()),
            "paso",
        ));
    }

    if let Some(ref diff) = task.diff_preview {
        sheet.append(&make_label("PREVISUALIZACIÓN DE CAMBIOS", "etiqueta"));
        let diff_box = GtkBox::new(Orientation::Vertical, 0);
        for line in diff.lines() {
            let css_class = if line.starts_with('+') {
                "mas"
            } else if line.starts_with('-') {
                "menos"
            } else {
                "info"
            };
            let l = make_label(line, "diff");
            l.add_css_class(css_class);
            diff_box.append(&l);
        }
        let scrolled = ScrolledWindow::builder()
            .child(&diff_box)
            .min_content_height(100)
            .max_content_height(260)
            .propagate_natural_height(true)
            .build();
        sheet.append(&scrolled);
    }

    content.append(&sheet);

    if task.state == FlowState::ReadyForApproval {
        let button_box = GtkBox::new(Orientation::Horizontal, 10);
        button_box.set_halign(Align::End);

        let reject_btn = Button::with_label("Rechazar Flow");
        reject_btn.add_css_class("descartar");
        let approve_btn = Button::with_label("Aprobar & Fusionar");
        approve_btn.add_css_class("aprobar");

        let ticket_id_clone = task.ticket_id.clone();
        for (btn, decision) in [(&reject_btn, false), (&approve_btn, true)] {
            let stream_ref = stream_writer.clone();
            let box_ref = button_box.clone();
            let tid = ticket_id_clone.clone();
            btn.connect_clicked(move |_| {
                if let Some(stream) = stream_ref.borrow_mut().as_mut() {
                    let req = Request::ApproveFlow {
                        ticket_id: tid.clone(),
                        decision,
                    };
                    if let Ok(json) = serde_json::to_string(&req) {
                        let _ = writeln!(stream, "{json}");
                        let _ = stream.flush();
                    }
                }
                box_ref.set_sensitive(false);
            });
        }

        button_box.append(&reject_btn);
        button_box.append(&approve_btn);
        content.append(&button_box);
    }
}

// ---------------------------------------------------------------- helpers

fn level_css_class(tier: Tier) -> &'static str {
    match tier {
        Tier::Auto => "auto",
        Tier::Confirm => "confirm",
        Tier::Grant => "grant",
    }
}

fn make_label(text: &str, class_name: &str) -> Label {
    let l = Label::new(Some(text));
    l.set_halign(Align::Start);
    l.set_xalign(0.0);
    l.set_selectable(true);
    l.add_css_class(class_name);
    l
}

fn render_waiting(content: &GtkBox) {
    content.append(&make_label("antOS pensando...", "radio"));
}

fn render_error(content: &GtkBox, message: &str) {
    empty_box(content);
    for line in message.lines() {
        content.append(&make_label(line, "error"));
    }
}

fn empty_box(gtk_box: &GtkBox) {
    while let Some(child) = gtk_box.first_child() {
        gtk_box.remove(&child);
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    let flat = s.replace('\n', "⏎");
    if flat.chars().count() <= max_len {
        flat
    } else {
        format!("{}…", flat.chars().take(max_len - 1).collect::<String>())
    }
}

// -------------------------------------------------------- launcher helpers (T25.4)

/// Renders the launcher match cards inside the launcher box.
fn render_launcher_results(
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
fn update_launcher_selection(launcher_box: &GtkBox, selected_idx: usize) {
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
fn launch_desktop_application_async(app: LauncherAppItem, window: ApplicationWindow) {
    std::thread::spawn(move || {
        let workspace = std::env::var("ANTOS_WORKSPACE").unwrap_or_else(|_| {
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ".".to_string())
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

        gtk4::glib::idle_add_local(move || {
            window.close();
            gtk4::glib::ControlFlow::Break
        });
    });
}

/// Loads installed applications from antpkg and Flatpak via the antOS daemon.
fn load_installed_apps_async(installed_apps: Rc<RefCell<Vec<LauncherAppItem>>>) {
    std::thread::spawn(move || {
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

        gtk4::glib::idle_add_local(move || {
            *installed_apps.borrow_mut() = apps;
            gtk4::glib::ControlFlow::Break
        });
    });
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
