//! antOS Wayland/GTK4 Layer Shell Intent Bar (T4.1).
//!
//! Provides the primary desktop interface for developer intentions, semantic git context,
//! rich diff inspections, multi-agent antFlow state visualizations, and safety approvals.

use antos_protocolo::{AgentRole, Evento, FlowState, FlowTask, GitRepoStatus, Line, Peticion, Propuesta, Tier};
use gtk4::gdk::Display;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry, Label,
    Orientation, PolicyType, ScrolledWindow,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};

const BAR_WIDTH: i32 = 740;

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
    window.set_margin(Edge::Top, 110);

    let frame = GtkBox::new(Orientation::Vertical, 12);
    frame.add_css_class("marco");
    window.set_child(Some(&frame));

    // Top status context bar (Git status + Planner selector + Dry run badge)
    let header_bar = GtkBox::new(Orientation::Horizontal, 8);
    header_bar.add_css_class("header-bar");
    header_bar.set_hexpand(true);

    let git_badge = make_label("🌿 checking git...", "badge");
    git_badge.add_css_class("git-badge");
    header_bar.append(&git_badge);

    let planner_btn = Button::with_label("⚡ local");
    planner_btn.add_css_class("badge-button");
    header_bar.append(&planner_btn);

    let dry_run_btn = Button::with_label("🛡️ live");
    dry_run_btn.add_css_class("badge-button");
    header_bar.append(&dry_run_btn);

    frame.append(&header_bar);

    // Main intent input entry
    let input = Entry::builder()
        .placeholder_text("¿Qué quieres que antOS haga? (ej. T4.2, libera 3000, crea rama auth)...")
        .build();
    input.add_css_class("intencion");
    frame.append(&input);

    // Quick suggestions pills
    let suggestions_box = GtkBox::new(Orientation::Horizontal, 6);
    suggestions_box.add_css_class("suggestions-box");

    let suggestions = [
        ("🚀 T4.2", "desarrolla ticket T4.2"),
        ("🔍 ports", "diagnostica puertos"),
        ("🌿 branch", "crea rama feature/auth"),
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

    // Query initial git status for header badge
    query_git_status_async(git_badge.clone());

    // Connect input activation
    {
        let content = content.clone();
        let stream_writer = stream_writer.clone();
        let input_ref = input.clone();
        let planner_ref = current_planner.clone();
        let dry_ref = dry_run_state.clone();

        input.connect_activate(move |entry| {
            let text = entry.text().to_string();
            if text.trim().is_empty() {
                return;
            }
            input_ref.set_sensitive(false);
            empty_box(&content);

            let selected_planner = planner_ref.borrow().clone();
            let is_dry = *dry_ref.borrow();

            match start_session(&text, selected_planner, is_dry) {
                Ok((stream, events)) => {
                    *stream_writer.borrow_mut() = Some(stream);
                    render_waiting(&content);
                    listen_events(events, content.clone(), stream_writer.clone(), input_ref.clone());
                }
                Err(err) => render_error(&content, &err),
            }
        });
    }

    // Escape key closes window without confirming
    let key_controller = gtk4::EventControllerKey::new();
    let window_ref = window.clone();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape {
            window_ref.close();
            return gtk4::glib::Propagation::Stop;
        }
        gtk4::glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    window.present();

    // Check optional CLI intent argument e.g. --intencion "..."
    let initial_intent: Option<String> = {
        let args: Vec<String> = std::env::args().collect();
        args.iter()
            .position(|a| a == "--intencion" || a == "--intent")
            .and_then(|i| args.get(i + 1).cloned())
    };

    if let Some(text) = initial_intent {
        input.set_text(&text);
        input.emit_activate();
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

        let req = Peticion::ConsultarEstadoGit {
            workspace_path: current_dir,
        };

        if let Ok(json) = serde_json::to_string(&req) {
            let _ = writeln!(stream, "{json}");
            let _ = stream.flush();

            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok() {
                if let Ok(event) = serde_json::from_str::<Evento>(line.trim()) {
                    if let Evento::EstadoGit(status) = event {
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
    let branch = status.rama.as_deref().unwrap_or("HEAD");
    let dirty_count = status.modificados.len() + status.sin_seguimiento.len();
    if status.limpio {
        badge.set_text(&format!("🌿 {branch} ✓"));
        badge.add_css_class("clean");
    } else {
        badge.set_text(&format!("🌿 {branch} *{dirty_count}"));
        badge.add_css_class("dirty");
    }
}

/// Connects to the daemon socket and starts the reader thread.
fn start_session(
    text: &str,
    planner: Option<String>,
    dry_run: bool,
) -> Result<(UnixStream, Receiver<Evento>), String> {
    let path = socket_path();
    let stream = UnixStream::connect(&path).map_err(|e| {
        format!(
            "no hay demonio antOS en {}: {e}\nArráncalo en otra terminal con: antos demonio",
            path.display()
        )
    })?;

    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let request = Peticion::Intencion {
        texto: text.to_string(),
        planificador: planner,
        seco: dry_run,
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
                    if let Ok(event) = serde_json::from_str::<Evento>(line.trim()) {
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
    events: Receiver<Evento>,
    content: GtkBox,
    stream_writer: Rc<RefCell<Option<UnixStream>>>,
    input: Entry,
) {
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(35), move || {
        while let Ok(event) = events.try_recv() {
            match event {
                Evento::Inicio { .. } => {}
                Evento::Nota(text) => {
                    empty_box(&content);
                    content.append(&make_label(&format!("antOS: {text}"), "radio"));
                }
                Evento::Propuesta(proposal) => {
                    empty_box(&content);
                    render_proposal(&content, &proposal, stream_writer.clone());
                }
                Evento::EstadoFlow(Some(task)) => {
                    empty_box(&content);
                    render_flow_task(&content, &task, stream_writer.clone());
                }
                Evento::TransicionFlow {
                    estado_nuevo,
                    rol,
                    detalle,
                    ..
                } => {
                    let role_label = rol.map(|r| r.name()).unwrap_or("System");
                    let transition_text = format!("{}: {} [{}]", estado_nuevo.label(), detalle, role_label);
                    content.append(&make_label(&transition_text, "paso"));
                }
                Evento::Salida(text) => content.append(&make_label(&text, "paso")),
                Evento::Resultado(result) => {
                    let class = if result.ok { "ok" } else { "error" };
                    content.append(&make_label(&result.mensaje, class));
                    input.set_sensitive(true);
                    input.set_text("");
                }
                Evento::Error(err_msg) => {
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
    proposal: &Propuesta,
    stream_writer: Rc<RefCell<Option<UnixStream>>>,
) {
    let sheet = GtkBox::new(Orientation::Vertical, 10);
    sheet.add_css_class("hoja");
    sheet.add_css_class(level_css_class(proposal.nivel));

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
    for line in &proposal.cambios {
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
        ("escribe", &proposal.radio.escribe),
        ("borra", &proposal.radio.borra),
        ("lee", &proposal.radio.lee),
        ("SISTEMA", &proposal.radio.sistema),
        ("red", &proposal.radio.red),
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
            proposal.nivel.label(),
            proposal.razones.join("; ")
        ),
        "nivel",
    );
    tier_label.add_css_class(level_css_class(proposal.nivel));
    sheet.append(&tier_label);

    sheet.append(&make_label(
        &format!(
            "recinto   {} — {}",
            proposal.recinto.motor, proposal.recinto.garantiza
        ),
        "radio",
    ));

    content.append(&sheet);

    // 4 · Decision buttons
    if proposal.nivel == Tier::Auto || proposal.seco {
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
                let response = Peticion::Aprobacion(decision);
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
        &format!("Ticket: {} | Estado: {}", task.ticket_id, task.estado.label()),
        "nivel",
    ));

    if let Some(role) = task.rol_actual {
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

    if task.estado == FlowState::ListoParaAprobacion {
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
                    let req = Peticion::AprobarFlow {
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
