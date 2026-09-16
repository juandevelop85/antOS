//! Tablero Kanban de tickets y Centro de Control de Agentes (`Super + A`).

use crate::session::run_offthread;
use crate::socket_path;
use crate::widgets::{empty_box, make_label};
use antos_protocol::{
    AgentRole, Event, FlowBackend, FlowTask, Request, TicketStatus, TicketSummary,
};
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Entry, Orientation, ScrolledWindow};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::rc::Rc;

/// Asynchronously fetches tickets and active tasks to construct the Kanban Board UI.
pub(crate) fn load_kanban_board_async(
    content: GtkBox,
    input: Entry,
    stream_writer: Rc<RefCell<Option<UnixStream>>>,
) {
    content.append(&make_label(
        "Cargando Centro de Control y Tablero...",
        "radio",
    ));

    run_offthread(
        || -> (Vec<TicketSummary>, Vec<FlowTask>) {
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
                        if let Ok(Event::TicketList(list)) =
                            serde_json::from_str::<Event>(line.trim())
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
                        if let Ok(Event::FlowList(list)) =
                            serde_json::from_str::<Event>(line.trim())
                        {
                            flows = list;
                        }
                    }
                }
            }

            (tickets, flows)
        },
        move |(tickets, flows)| {
            empty_box(&content);
            render_kanban_view(
                &content,
                &tickets,
                &flows,
                input.clone(),
                stream_writer.clone(),
            );
        },
    );
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
    // T33.1: si alguna tarea es la simulación de T3.1, el monitor lo dice en
    // vez de presentar los roles como agentes trabajando.
    if flows.iter().any(|f| f.backend == FlowBackend::Simulated) {
        let badge = make_label("⚠ simulación (sin modelo)", "agent-badge");
        badge.add_css_class("idle");
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
