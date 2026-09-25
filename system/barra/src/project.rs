//! El proyecto sobre el que opera la barra (T38.2).
//!
//! Hasta este ticket la barra no sabía sobre qué actuaba: mandaba su propio
//! directorio de trabajo como ámbito —en una sesión de greetd, `$HOME`— y el
//! badge de git caía siempre a su texto de reserva («🌿 antOS») porque ese
//! directorio no es un repositorio. No ha mostrado un estado real nunca en
//! el escritorio.
//!
//! Ahora el ámbito es el proyecto activo que resuelve el demonio (T38.1),
//! el mismo que fija `antos use` en una terminal. La barra lo pregunta cada
//! pocos segundos con el temporizador que ya tenía, así que un cambio hecho
//! en la terminal aparece aquí sin reiniciar nada, y uno hecho aquí lo ve
//! la terminal en su siguiente orden.

use crate::session::{request_one, run_offthread};
use crate::widgets::{empty_box, make_label, make_selectable_label, render_error};
use antos_protocol::{Event, ProjectStatus, ProjectSummary, Request};
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Entry, GestureClick, Orientation, ScrolledWindow};
use std::cell::RefCell;
use std::rc::Rc;

/// Lo último que el demonio dijo sobre el ámbito. `None` mientras no ha
/// contestado: al arrancar, o con el demonio caído.
pub(crate) type Scope = Rc<RefCell<Option<ProjectStatus>>>;

/// Los widgets que intervienen al elegir proyecto. Todos son referencias
/// baratas de clonar (GTK cuenta referencias; `Scope` es un `Rc`).
#[derive(Clone)]
pub(crate) struct ProjectUi {
    pub(crate) scope: Scope,
    pub(crate) chip: Button,
    pub(crate) content: GtkBox,
    pub(crate) content_scroll: ScrolledWindow,
    pub(crate) decisions: GtkBox,
    pub(crate) input: Entry,
}

// ─────────────────────────────────────────────────────────── lógica pura

/// Ámbito de una petición: `(workspace_path, project)`.
///
/// Con respuesta del demonio, el directorio es **su** workspace y el
/// proyecto, el activo: el `cwd` de la barra deja de decidir nada. Sin
/// respuesta todavía se conserva lo de antes —el `cwd`— para no dejar la
/// barra muda en el primer instante.
pub(crate) fn request_scope(status: Option<&ProjectStatus>) -> (String, Option<String>) {
    match status {
        Some(status) => (
            status.workspace.clone(),
            status.active_name().map(str::to_string),
        ),
        None => (current_dir_string(), None),
    }
}

/// El directorio sobre el que se opera: el del proyecto activo, o el
/// workspace si no hay ninguno.
pub(crate) fn scope_dir(status: Option<&ProjectStatus>) -> String {
    match status {
        Some(status) => status
            .active
            .as_ref()
            .map(|p| p.path.clone())
            .unwrap_or_else(|| status.workspace.clone()),
        None => current_dir_string(),
    }
}

fn current_dir_string() -> String {
    std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".into())
}

/// Texto del chip de la cabecera. Responde a la primera pregunta que uno se
/// hace al mirar la barra: «¿esto sobre qué actúa?».
pub(crate) fn chip_text(status: &ProjectStatus) -> String {
    match &status.active {
        Some(project) => match &project.branch {
            Some(branch) => {
                let mark = if project.dirty { "*" } else { "✓" };
                format!("🌿 {} · {branch} {mark}", project.name)
            }
            // Sin git, sin rama que enseñar y sin «limpio» que afirmar.
            None => format!("📁 {}", project.name),
        },
        None => "📁 workspace".to_string(),
    }
}

/// `@nombre resto de la intención` → `Some(("nombre", "resto…"))`.
///
/// Es el atajo de teclado para cambiar de proyecto sin soltar el teclado,
/// coherente con los prefijos que la barra ya entendía (`agente:`,
/// `ticket:`). `@` a secas pide la lista.
pub(crate) fn parse_at_command(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim().strip_prefix('@')?;
    match rest.split_once(char::is_whitespace) {
        Some((name, tail)) => Some((name, tail.trim())),
        None => Some((rest, "")),
    }
}

/// Proyectos cuyo nombre contiene `filter`, sin distinguir mayúsculas.
pub(crate) fn filter_projects<'a>(
    projects: &'a [ProjectSummary],
    filter: &str,
) -> Vec<&'a ProjectSummary> {
    let needle = filter.to_lowercase();
    projects
        .iter()
        .filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle))
        .collect()
}

// ────────────────────────────────────────────────────────────── vista

/// Guarda el ámbito y repinta el chip.
fn apply_status(ui: &ProjectUi, status: ProjectStatus) {
    ui.chip.set_label(&chip_text(&status));
    for class in ["offline", "clean", "dirty"] {
        ui.chip.remove_css_class(class);
    }
    if let Some(project) = &status.active {
        if project.branch.is_some() {
            ui.chip
                .add_css_class(if project.dirty { "dirty" } else { "clean" });
        }
    }
    *ui.scope.borrow_mut() = Some(status);
}

/// Pregunta al demonio sobre qué se opera y repinta el chip. Se llama al
/// arrancar y en cada tic del temporizador de la barra: así se entera de un
/// `antos use` hecho en una terminal.
pub(crate) fn refresh_status(ui: &ProjectUi) {
    let ui = ui.clone();
    run_offthread(
        || request_one(&Request::QueryProjectStatus),
        move |answer| match answer {
            Ok(Event::ProjectStatus(status)) => apply_status(&ui, status),
            // Un demonio que no conoce la petición (anterior a T38.1): se
            // deja el chip como está en vez de afirmar nada.
            Ok(_) => {}
            Err(_) => {
                ui.chip.set_label("● offline");
                ui.chip.add_css_class("offline");
            }
        },
    );
}

/// Fija el proyecto activo (o lo limpia con `None`) y después llama a
/// `then` con el resultado ya aplicado a la vista.
fn use_project<F>(ui: &ProjectUi, name: Option<String>, then: F)
where
    F: Fn(Result<ProjectStatus, String>) + 'static,
{
    let ui = ui.clone();
    run_offthread(
        move || match request_one(&Request::UseProject { name }) {
            Ok(Event::ProjectChanged(status)) => Ok(status),
            Ok(Event::Error(err)) => Err(err),
            Ok(_) => Err("el demonio no entendió el cambio de proyecto".to_string()),
            Err(err) => Err(err),
        },
        move |result: Result<ProjectStatus, String>| {
            if let Ok(status) = &result {
                apply_status(&ui, status.clone());
            }
            then(result);
        },
    );
}

/// Abre el selector en el panel, con los proyectos que contengan `filter`.
pub(crate) fn open_selector(ui: &ProjectUi, filter: String) {
    ui.content_scroll.set_visible(true);
    empty_box(&ui.decisions);
    ui.decisions.set_visible(false);
    empty_box(&ui.content);
    ui.content.append(&make_label("Buscando proyectos…", "radio"));

    let ui = ui.clone();
    run_offthread(
        || match request_one(&Request::ListProjects) {
            Ok(Event::ProjectList(projects)) => Ok(projects),
            Ok(Event::Error(err)) => Err(err),
            Ok(_) => Err("el demonio no sabe listar proyectos (¿anterior a T38.1?)".to_string()),
            Err(err) => Err(err),
        },
        move |result| {
            empty_box(&ui.content);
            match result {
                Ok(projects) => render_list(&ui, &projects, &filter),
                Err(err) => render_error(&ui.content, &err),
            }
        },
    );
}

/// `@nombre resto`: cambia de proyecto y, si hay resto, lo lanza como
/// intención **sobre el proyecto nuevo**. Si el nombre no existe tal cual,
/// abre el selector filtrado por él en vez de fallar: `@api` encuentra
/// `api-service`.
pub(crate) fn switch_from_input(ui: &ProjectUi, name: &str, rest: &str) {
    if name.is_empty() {
        open_selector(ui, String::new());
        return;
    }
    let ui_then = ui.clone();
    let wanted = name.to_string();
    let rest = rest.to_string();
    use_project(ui, Some(name.to_string()), move |result| match result {
        Ok(status) => {
            if rest.is_empty() {
                show_confirmation(&ui_then, &status);
            } else {
                // Solo con el cambio confirmado: lanzar el resto contra el
                // proyecto anterior sería peor que no lanzarlo.
                ui_then.input.set_text(&rest);
                ui_then.input.emit_activate();
            }
        }
        Err(_) => open_selector(&ui_then, wanted.clone()),
    });
}

fn show_confirmation(ui: &ProjectUi, status: &ProjectStatus) {
    ui.content_scroll.set_visible(true);
    empty_box(&ui.content);
    let name = status.active_name().unwrap_or("workspace (sin proyecto)");
    ui.content.append(&make_label(
        &format!("✓ proyecto activo: {name}"),
        "ok",
    ));
    ui.content.append(&make_label(
        "La terminal lo ve en su siguiente orden: es la misma selección que `antos use`.",
        "paso",
    ));
    ui.input.grab_focus();
}

fn render_list(ui: &ProjectUi, projects: &[ProjectSummary], filter: &str) {
    ui.content.append(&make_label("PROYECTOS DEL WORKSPACE", "etiqueta"));

    // Primera tarjeta: volver al ámbito del workspace. Es la forma visible
    // de `antos use --clear`.
    ui.content.append(&card(
        ui,
        "📁 workspace",
        "sin proyecto: operar sobre todo el workspace",
        None,
    ));

    let matching = filter_projects(projects, filter);
    if matching.is_empty() {
        let text = if filter.is_empty() {
            "No hay proyectos en el workspace todavía. Crea uno con «crea un proyecto en rust llamado demo».".to_string()
        } else {
            format!("Ningún proyecto contiene «{filter}».")
        };
        ui.content.append(&make_selectable_label(&text, "paso"));
        return;
    }
    for project in matching {
        let branch = project
            .branch
            .as_deref()
            .map(|b| format!(" · {b}{}", if project.dirty { " *" } else { "" }))
            .unwrap_or_default();
        let marker = if project.is_active { "● " } else { "" };
        ui.content.append(&card(
            ui,
            &format!("{marker}{}", project.name),
            &format!("{}{branch}", project.language),
            Some(project.name.clone()),
        ));
    }
}

/// Una tarjeta pulsable del selector, con el mismo aspecto que las del
/// lanzador (y, como ellas desde T37.2, cursor de puntero).
fn card(ui: &ProjectUi, title: &str, subtitle: &str, pick: Option<String>) -> GtkBox {
    let row = GtkBox::new(Orientation::Vertical, 2);
    row.add_css_class("launcher-card");
    row.set_cursor_from_name(Some("pointer"));
    row.append(&make_label(title, "launcher-title"));
    row.append(&make_label(subtitle, "launcher-subtitle"));

    let gesture = GestureClick::new();
    let ui = ui.clone();
    gesture.connect_released(move |_, _, _, _| {
        let ui_then = ui.clone();
        use_project(&ui, pick.clone(), move |result| match result {
            Ok(status) => show_confirmation(&ui_then, &status),
            Err(err) => {
                empty_box(&ui_then.content);
                render_error(&ui_then.content, &err);
            }
        });
    });
    row.add_controller(gesture);
    row
}

#[cfg(test)]
mod tests {
    //! La lógica pura del selector. Solo corren en Linux —en el Mac de
    //! desarrollo este crate ni siquiera compila— y **no** las ejecuta
    //! `nix build .#antos-barra`, que lleva `doCheck = false`. Hay que
    //! pedirlas: `nix develop .#antos-barra --command cargo test` desde
    //! `system/barra`.
    use super::*;
    use antos_protocol::ProjectOrigin;

    fn project(name: &str, branch: Option<&str>, dirty: bool) -> ProjectSummary {
        ProjectSummary {
            name: name.into(),
            path: format!("/w/{name}"),
            language: "rust".into(),
            branch: branch.map(str::to_string),
            dirty,
            is_active: true,
        }
    }

    fn status(active: Option<ProjectSummary>) -> ProjectStatus {
        ProjectStatus {
            origin: if active.is_some() {
                ProjectOrigin::Selection
            } else {
                ProjectOrigin::None
            },
            active,
            workspace: "/w".into(),
        }
    }

    #[test]
    fn test_the_chip_says_what_the_bar_operates_on() {
        assert_eq!(chip_text(&status(None)), "📁 workspace");
        assert_eq!(
            chip_text(&status(Some(project("api", Some("main"), false)))),
            "🌿 api · main ✓"
        );
        assert_eq!(
            chip_text(&status(Some(project("api", Some("feat"), true)))),
            "🌿 api · feat *"
        );
        // Sin git no hay rama que enseñar, ni «limpio» que afirmar.
        assert_eq!(
            chip_text(&status(Some(project("notas", None, false)))),
            "📁 notas"
        );
    }

    #[test]
    fn test_the_at_prefix_splits_project_and_intent() {
        assert_eq!(parse_at_command("@api"), Some(("api", "")));
        assert_eq!(
            parse_at_command("@api crea la rama auth"),
            Some(("api", "crea la rama auth"))
        );
        assert_eq!(parse_at_command("  @api   libera 3000 "), Some(("api", "libera 3000")));
        assert_eq!(parse_at_command("@"), Some(("", "")));
        assert_eq!(parse_at_command("libera 3000"), None);
        assert_eq!(parse_at_command("correo a juan@ejemplo"), None, "solo al principio");
    }

    #[test]
    fn test_the_scope_comes_from_the_daemon_not_from_the_cwd() {
        let with_project = status(Some(project("api", Some("main"), false)));
        assert_eq!(
            request_scope(Some(&with_project)),
            ("/w".to_string(), Some("api".to_string()))
        );
        assert_eq!(scope_dir(Some(&with_project)), "/w/api");

        let without = status(None);
        assert_eq!(request_scope(Some(&without)), ("/w".to_string(), None));
        assert_eq!(scope_dir(Some(&without)), "/w");
    }

    #[test]
    fn test_the_filter_finds_by_substring_ignoring_case() {
        let all = vec![
            project("api-service", None, false),
            project("web", None, false),
            project("Docs-API", None, false),
        ];
        let names: Vec<_> = filter_projects(&all, "api").iter().map(|p| p.name.clone()).collect();
        assert_eq!(names, vec!["api-service", "Docs-API"]);
        assert_eq!(filter_projects(&all, "").len(), 3);
        assert!(filter_projects(&all, "nada").is_empty());
    }
}
