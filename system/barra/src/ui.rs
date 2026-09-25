//! Ensamblado de la ventana principal (superficie Wayland Layer Shell) y
//! cableado de todos los widgets de la barra.

use crate::kanban::load_kanban_board_async;
use crate::project::{
    open_selector, parse_at_command, refresh_status, scope_dir, switch_from_input, ProjectUi,
    Scope,
};
use crate::launcher::{
    launch_desktop_application_async, load_installed_apps_async, render_launcher_results,
    update_launcher_selection,
};
use crate::session::{listen_events, start_session, start_session_request};
use crate::telemetry::query_telemetry_async;
use crate::widgets::{action_button, empty_box, make_label, render_error, render_waiting};
use crate::{BAR_WIDTH, CONTENT_HEIGHT};
use antos_protocol::{is_app_query, match_applications, LauncherAppItem, Request};
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Entry, Image, Label, Orientation,
    PolicyType, ScrolledWindow,
};
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

    // Icono de bandeja SNI (T30.8, T32.4): canal reactivo sin sondeo activo.
    // `async-channel` (ya presente en el árbol de dependencias vía `ksni`)
    // sustituye a `glib::MainContext::channel`, retirado de glib-rs en 0.19;
    // el consumidor se cablea más abajo con `glib::spawn_future_local`, una
    // vez existen `window`/`input` — ver `tray.rs`.
    let (tray_tx, tray_rx) = async_channel::unbounded::<()>();
    crate::tray::spawn(tray_tx);

    let frame = GtkBox::new(Orientation::Vertical, 12);
    frame.add_css_class("marco");
    window.set_child(Some(&frame));

    // Top status context bar (Git status + Planner selector + Dry run badge + Kanban toggle)
    let header_bar = GtkBox::new(Orientation::Horizontal, 8);
    header_bar.add_css_class("header-bar");
    header_bar.set_hexpand(true);

    header_bar.append(&build_brand());

    // Chip de proyecto (T38.2): dice sobre qué actúa la barra y, al
    // pulsarlo, deja cambiarlo. Sustituye al badge de git, que preguntaba
    // por el directorio donde arrancó el proceso —en una sesión de greetd,
    // `$HOME`— y por eso nunca mostró más que su texto de reserva.
    let project_chip = action_button("🌿 …", "badge-button");
    project_chip.add_css_class("git-badge");
    project_chip.add_css_class("project-chip");
    project_chip.set_tooltip_text(Some(
        "Proyecto activo. Pulsa para cambiarlo, o escribe @nombre en la intención.",
    ));
    header_bar.append(&project_chip);
    let scope: Scope = Rc::new(RefCell::new(None));

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

    let planner_btn = action_button("⚡ local", "badge-button");
    header_bar.append(&planner_btn);

    let dry_run_btn = action_button("🛡️ live", "badge-button");
    header_bar.append(&dry_run_btn);

    let kanban_btn = action_button("📊 tablero", "badge-button");
    header_bar.append(&kanban_btn);

    frame.append(&header_bar);

    // Main intent input entry
    let input = Entry::builder()
        .placeholder_text("¿Qué quieres que antOS haga? (ej. T5.1, libera 3000, crea rama auth)...")
        .build();
    input.add_css_class("intencion");
    input.set_hexpand(true);

    // Fila de intención (T37.1): el mismo `❯` brillante y la pista `↵` que
    // el escritorio simulado de la web. La `Entry` no cambia: solo se mete
    // dentro de una caja que dibuja el marco.
    let intent_row = GtkBox::new(Orientation::Horizontal, 10);
    intent_row.add_css_class("intent");
    let glyph = Label::new(Some("❯"));
    glyph.add_css_class("intent-glyph");
    intent_row.append(&glyph);
    intent_row.append(&input);
    let hint = Label::new(Some("↵"));
    hint.add_css_class("intent-hint");
    hint.set_valign(Align::Center);
    intent_row.append(&hint);
    frame.append(&intent_row);

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
        let pill = action_button(pill_label, "suggestion-pill");
        let input_ref = input.clone();
        let query_str = pill_query.to_string();
        pill.connect_clicked(move |_| {
            input_ref.set_text(&query_str);
            input_ref.emit_activate();
            // El botón ya no roba el foco (`action_button`), pero si venía
            // de otro sitio —una tarjeta del lanzador, el teclado— esto
            // deja el punto de inserción al final del texto que se acaba
            // de poner, listo para seguir escribiendo.
            input_ref.grab_focus();
            input_ref.set_position(-1);
        });
        suggestions_box.append(&pill);
    }
    frame.append(&suggestions_box);

    // Dynamic response and proposals container.
    //
    // Va dentro de un `ScrolledWindow` de altura fija: sin él, la superficie
    // de layer-shell sigue al contenido y la barra cambiaba de tamaño con
    // cada mensaje —un plan largo la estiraba media pantalla y el siguiente
    // la encogía— (reportado usando el escritorio, 2026-09-25). Ahora la
    // barra tiene dos tamaños y solo dos: compacta mientras no hay nada que
    // enseñar, y `CONTENT_HEIGHT` en cuanto lo hay, pase lo que pase con el
    // largo de la respuesta.
    let content = GtkBox::new(Orientation::Vertical, 10);
    let content_scroll = ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .min_content_height(CONTENT_HEIGHT)
        .max_content_height(CONTENT_HEIGHT)
        .build();
    content_scroll.add_css_class("content-scroll");
    // Oculto hasta que haya algo: una barra recién abierta no tiene por qué
    // ocupar media pantalla en blanco.
    content_scroll.set_visible(false);
    frame.append(&content_scroll);

    // Fila de decisión, **fuera** del panel que se desplaza.
    //
    // Los botones de aprobar y descartar vivían dentro del contenido, y con
    // el panel de altura fija un plan largo los dejaba por debajo del
    // pliegue: había que buscarlos con la rueda para poder decidir
    // (reportado, 2026-09-25). Una decisión que no se ve es una decisión
    // que no se toma. Aquí están siempre al pie de la barra, y la fila solo
    // existe mientras hay algo que decidir.
    let decisions = GtkBox::new(Orientation::Horizontal, 10);
    decisions.add_css_class("decisiones");
    decisions.set_halign(Align::End);
    decisions.set_visible(false);
    frame.append(&decisions);

    // Seguir el final del panel cuando llega contenido nuevo, **solo** si el
    // usuario ya estaba abajo. Con la altura fija esto deja de ser un lujo:
    // el resultado de una aprobación se añade al final y, sin esto, caería
    // fuera de la vista y volvería a parecer que no ha pasado nada. Si el
    // usuario ha subido a releer algo, no se le arrastra.
    {
        let at_bottom: Rc<Cell<bool>> = Rc::new(Cell::new(true));
        let adjustment = content_scroll.vadjustment();
        let seen = at_bottom.clone();
        adjustment.connect_value_changed(move |adj| {
            seen.set(adj.value() + adj.page_size() >= adj.upper() - 4.0);
        });
        let follow = at_bottom.clone();
        adjustment.connect_changed(move |adj| {
            if follow.get() {
                adj.set_value(adj.upper() - adj.page_size());
            }
        });
    }

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
        let scope_ref = scope.clone();
        kanban_btn.connect_clicked(move |_| {
            empty_box(&content_ref);
            load_kanban_board_async(
                content_ref.clone(),
                input_ref.clone(),
                writer_ref.clone(),
                &scope_ref,
            );
        });
    }

    // Click en el icono de bandeja → alterna expandir/colapsar (T30.8, T32.4).
    // `tray::spawn` manda por `tray_tx` desde su propio hilo (`Tray::activate`,
    // disparado por el host SNI); aquí una tarea local del bucle GLib duerme
    // en `recv().await` y solo despierta cuando llega un evento — sin ningún
    // sondeo periódico por temporizador. Termina sola si el emisor se cierra.
    {
        let window_ref = window.clone();
        let input_ref = input.clone();
        let is_expanded_ref = is_expanded.clone();
        gtk4::glib::spawn_future_local(async move {
            while tray_rx.recv().await.is_ok() {
                set_bar_expanded(
                    &window_ref,
                    &input_ref,
                    &is_expanded_ref,
                    !is_expanded_ref.get(),
                );
            }
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

    // El ámbito de la barra (T38.2): chip, selector y atajo `@nombre`.
    let project_ui = ProjectUi {
        scope: scope.clone(),
        chip: project_chip.clone(),
        content: content.clone(),
        content_scroll: content_scroll.clone(),
        decisions: decisions.clone(),
        input: input.clone(),
    };
    {
        let ui = project_ui.clone();
        project_chip.connect_clicked(move |_| open_selector(&ui, String::new()));
    }

    // Query initial project scope and system telemetry
    refresh_status(&project_ui);
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
        let ui = project_ui.clone();
        gtk4::glib::timeout_add_local(std::time::Duration::from_secs(3), move || {
            query_telemetry_async(eb.clone(), pb.clone(), prb.clone(), mb.clone());
            // Así se entera la barra de un `antos use` hecho en una
            // terminal, sin suscripción ni reinicio (T38.1 / T38.2).
            refresh_status(&ui);
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
        let content_scroll_ref = content_scroll.clone();
        let decisions_ref = decisions.clone();
        let scope_activate = scope.clone();
        let project_ui_activate = project_ui.clone();

        input.connect_activate(move |entry| {
            let text = entry.text().to_string();
            let text_trimmed = text.trim();
            if text_trimmed.is_empty() {
                return;
            }

            // `@nombre resto` (T38.2): cambia de proyecto sin soltar el
            // teclado y, si hay resto, lo lanza sobre el proyecto nuevo.
            // Va antes que todo lo demás: `@api` no es una aplicación ni una
            // intención.
            if let Some((name, rest)) = parse_at_command(text_trimmed) {
                entry.set_text("");
                switch_from_input(&project_ui_activate, name, rest);
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

            // La intención ya está capturada en `text_trimmed`: la caja se
            // vacía en cuanto se envía, que es lo que uno espera de una
            // línea de órdenes — al volver la respuesta, el input está
            // listo para la siguiente sin tener que borrar la anterior.
            entry.set_text("");

            // A partir de aquí hay algo que enseñar: el área de respuestas
            // aparece con su altura fija y ya no cambia de tamaño. La
            // decisión pendiente de la intención anterior, si la había,
            // deja de tener sentido.
            content_scroll_ref.set_visible(true);
            empty_box(&decisions_ref);
            decisions_ref.set_visible(false);

            if text_trimmed == "panel" || text_trimmed == "board" || text_trimmed == "tablero" {
                empty_box(&content);
                load_kanban_board_async(
                    content.clone(),
                    input_ref.clone(),
                    stream_writer.clone(),
                    &scope_activate,
                );
                return;
            }

            // Aquí se desactivaba el input mientras durase la sesión, y eso
            // causaba dos fallos a la vez (2026-09-25): desactivar el widget
            // que tiene el foco se lo quita, la ventana se queda sin foco y
            // el `EventControllerFocus` de más arriba colapsa la barra —de
            // ahí que al pulsar Enter se cerrara sola—; y la rama
            // `Event::Proposal` nunca lo reactivaba, así que con un plan en
            // pantalla el input quedaba muerto con el texto viejo dentro.
            // El input se queda vivo: pulsar Enter otra vez abre una sesión
            // nueva y la anterior se cierra al reemplazar su `stream`.
            empty_box(&content);

            let selected_planner = planner_ref.borrow().clone();
            let is_dry = *dry_ref.borrow();

            // T33.4: dos prefijos abren diálogos de agente por la misma
            // sesión. `agente: <objetivo>` → un run con herramientas
            // (`AgentRun`); `ticket: T1.2` (o «desarrolla ticket T1.2») →
            // el pipeline de roles (`StartFlow`). Lo demás es una intención.
            let session = if let Some(goal) = strip_prefix_ci(text_trimmed, "agente:")
                .or_else(|| strip_prefix_ci(text_trimmed, "agent:"))
            {
                start_session_request(Request::AgentRun {
                    goal: goal.trim().to_string(),
                    provider: None,
                    toolset: None,
                    budget: None,
                    dry_run: is_dry,
                })
            } else if let Some(ticket) = strip_prefix_ci(text_trimmed, "ticket:")
                .or_else(|| strip_prefix_ci(text_trimmed, "desarrolla ticket "))
                .or_else(|| strip_prefix_ci(text_trimmed, "desarrolla el ticket "))
            {
                // El ticket vive en el proyecto activo (T38.2), no en el
                // directorio donde arrancó el proceso de la barra.
                let workspace_path = scope_dir(scope_activate.borrow().as_ref());
                start_session_request(Request::StartFlow {
                    workspace_path,
                    ticket_id: ticket.trim().to_uppercase(),
                })
            } else {
                start_session(text_trimmed, selected_planner, is_dry)
            };

            match session {
                Ok((stream, events)) => {
                    *stream_writer.borrow_mut() = Some(stream);
                    render_waiting(&content);
                    listen_events(
                        events,
                        content.clone(),
                        decisions_ref.clone(),
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
    let scope_key = scope.clone();
    key_controller.connect_key_pressed(move |_, key, _, modifier| {
        if key == gtk4::gdk::Key::Escape {
            set_bar_expanded(&window_ref, &input_ref, &is_expanded_ref, false);
            return gtk4::glib::Propagation::Stop;
        }
        if (key == gtk4::gdk::Key::a || key == gtk4::gdk::Key::A)
            && modifier.contains(gtk4::gdk::ModifierType::SUPER_MASK)
        {
            empty_box(&content_ref);
            load_kanban_board_async(
                content_ref.clone(),
                input_ref.clone(),
                writer_ref.clone(),
                &scope_key,
            );
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
        load_kanban_board_async(content.clone(), input.clone(), stream_writer.clone(), &scope);
    } else if let Some(text) = initial_intent {
        input.set_text(&text);
        input.emit_activate();
    }
}

/// Icono de la app embebido en el binario (T37.1): 64×64, derivado de
/// `system/desktop/assets/antos-icon.png` con el recorte redondeado de
/// `system/nixos/branding.nix`.
const ICON_PNG: &[u8] = include_bytes!("icono.png");

/// Marca de la cabecera (T37.1): icono + wordmark «ant» «OS». Si GTK no
/// puede decodificar el icono, la marca sale solo con el texto.
fn build_brand() -> GtkBox {
    let brand = GtkBox::new(Orientation::Horizontal, 8);
    brand.add_css_class("brand");
    brand.set_valign(Align::Center);

    match gtk4::gdk::Texture::from_bytes(&gtk4::glib::Bytes::from_static(ICON_PNG)) {
        Ok(texture) => {
            let icon = Image::from_paintable(Some(&texture));
            icon.set_pixel_size(26);
            icon.add_css_class("brand-icon");
            brand.append(&icon);
        }
        Err(err) => eprintln!("antOS · aviso: no se pudo cargar el icono de la barra: {err}"),
    }

    let wordmark = GtkBox::new(Orientation::Horizontal, 0);
    let ant = Label::new(Some("ant"));
    ant.add_css_class("wordmark-ant");
    let os = Label::new(Some("OS"));
    os.add_css_class("wordmark-os");
    wordmark.append(&ant);
    wordmark.append(&os);
    brand.append(&wordmark);
    brand
}

/// `strip_prefix` sin distinguir mayúsculas ASCII y sin cortar un carácter
/// multibyte a medias (T33.4).
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let n = prefix.len();
    if text.len() < n || !text.is_char_boundary(n) {
        return None;
    }
    if text[..n].eq_ignore_ascii_case(prefix) {
        Some(&text[n..])
    } else {
        None
    }
}
