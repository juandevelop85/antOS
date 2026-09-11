//! Sesión de intención contra el demonio antOS: apertura del socket,
//! hilo lector de eventos y renderizado de propuestas / tareas de `antFlow`.

use crate::socket_path;
use crate::widgets::{empty_box, level_css_class, make_label, render_error, truncate_str};
use antos_protocol::{Event, FlowState, FlowTask, Line, Proposal, Request, Tier};
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Entry, Orientation, PolicyType, ScrolledWindow};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};

/// Ejecuta `work` en un hilo de trabajo —solo I/O de socket, su resultado debe
/// ser `Send`— y aplica `apply` en el hilo principal de GTK con ese resultado.
///
/// Los objetos de `gtk4` y los `Rc` que toca `apply` nunca salen del hilo
/// principal: solo cruza el valor `T: Send`. Es el mismo patrón que
/// `dispatch_intent` / `listen_events` (un hilo que solo hace I/O y `send`, y un
/// `timeout_add_local` que consume el `Receiver` y toca la interfaz),
/// generalizado para un único resultado. Sondear cada 50 ms es más que
/// suficiente para una respuesta de socket local y evita quemar el bucle
/// principal.
pub(crate) fn run_offthread<T, W, A>(work: W, apply: A)
where
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    A: Fn(T) + 'static,
{
    let (sender, receiver) = channel::<T>();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(50), move || match receiver
        .try_recv()
    {
        Ok(value) => {
            apply(value);
            gtk4::glib::ControlFlow::Break
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => gtk4::glib::ControlFlow::Continue,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => gtk4::glib::ControlFlow::Break,
    });
}

/// Connects to the daemon socket and starts the reader thread.
pub(crate) fn start_session(
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

pub(crate) fn listen_events(
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
