//! Tests of the single project resolution (T38.1).
//!
//! What matters here is not that the code runs, but that the precedence is
//! fixed and that the selection is read from disk on every call: the bug
//! this module closes was a daemon that resolved its project once, at
//! start-up, and never saw an `antos use` again.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use antos_protocol::ProjectOrigin;

/// A workspace with `projects` as first-level directories, plus an empty
/// state directory.
fn workspace_with(projects: &[&str]) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "antos-projects-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let workspace = root.join("workspace");
    let state = root.join("state");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    for p in projects {
        std::fs::create_dir_all(workspace.join(p)).unwrap();
    }
    (workspace, state)
}

fn cleanup(workspace: &Path) {
    if let Some(root) = workspace.parent() {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn test_without_selection_the_scope_is_the_workspace() {
    let (workspace, state) = workspace_with(&["api"]);
    let status = resolve(&workspace, &state, None);
    assert!(status.active.is_none());
    assert_eq!(status.origin, ProjectOrigin::None);
    assert_eq!(scope_dir(&workspace, &state, None), workspace);
    cleanup(&workspace);
}

#[test]
fn test_the_selection_is_read_from_disk_on_every_call() {
    // El corazón del ticket: un `antos use` posterior al arranque del
    // demonio tiene que verse sin reiniciar nada. Aquí se simula
    // escribiendo el fichero entre dos resoluciones.
    let (workspace, state) = workspace_with(&["api", "web"]);
    assert!(resolve(&workspace, &state, None).active.is_none());

    write_selection(&state, Some("api")).unwrap();
    let status = resolve(&workspace, &state, None);
    assert_eq!(status.active_name(), Some("api"));
    assert_eq!(status.origin, ProjectOrigin::Selection);

    write_selection(&state, Some("web")).unwrap();
    assert_eq!(resolve(&workspace, &state, None).active_name(), Some("web"));

    write_selection(&state, None).unwrap();
    assert!(resolve(&workspace, &state, None).active.is_none());
    cleanup(&workspace);
}

#[test]
fn test_an_explicit_request_wins_over_the_persistent_selection() {
    let (workspace, state) = workspace_with(&["api", "web"]);
    write_selection(&state, Some("api")).unwrap();
    let status = resolve(&workspace, &state, Some("web"));
    assert_eq!(status.active_name(), Some("web"));
    assert_eq!(status.origin, ProjectOrigin::Request);
    cleanup(&workspace);
}

#[test]
fn test_a_selection_pointing_nowhere_degrades_to_the_workspace() {
    // Un proyecto borrado no puede dejar al demonio sin poder responder.
    let (workspace, state) = workspace_with(&["api"]);
    write_selection(&state, Some("se-borro")).unwrap();
    let status = resolve(&workspace, &state, None);
    assert!(status.active.is_none());
    assert_eq!(status.origin, ProjectOrigin::None);
    cleanup(&workspace);
}

#[test]
fn test_a_name_with_a_path_is_refused() {
    // El nombre llega por IPC y se une al workspace: sin esto, `..` saca
    // al sistema del recinto.
    for name in ["../etc", "/etc", "a/b", "..", ".", "", "  "] {
        assert!(
            validate_name(name).is_err(),
            "«{name}» tendría que rechazarse"
        );
    }
    assert!(validate_name("api-service").is_ok());
    assert!(validate_name(" api ").is_ok(), "los espacios se recortan");
}

#[test]
fn test_select_refuses_a_project_that_does_not_exist_and_does_not_persist_it() {
    let (workspace, state) = workspace_with(&["api"]);
    assert!(select(&workspace, &state, Some("fantasma")).is_err());
    assert!(
        read_selection(&state).is_none(),
        "un fallo no deja selección a medias"
    );

    let status = select(&workspace, &state, Some("api")).unwrap();
    assert_eq!(status.active_name(), Some("api"));
    assert_eq!(read_selection(&state).as_deref(), Some("api"));

    let cleared = select(&workspace, &state, None).unwrap();
    assert!(cleared.active.is_none());
    assert!(read_selection(&state).is_none());
    cleanup(&workspace);
}

#[test]
fn test_select_refuses_a_traversal_even_if_the_directory_exists() {
    let (workspace, state) = workspace_with(&["api"]);
    // `workspace/../workspace` existe, y aun así no es un nombre válido.
    assert!(select(&workspace, &state, Some("../workspace")).is_err());
    assert!(read_selection(&state).is_none());
    cleanup(&workspace);
}

#[test]
fn test_list_marks_the_active_project_and_nothing_else() {
    let (workspace, state) = workspace_with(&["api", "web", "docs"]);
    write_selection(&state, Some("web")).unwrap();
    let projects = list(&workspace, &state);
    assert_eq!(projects.len(), 3);
    let active: Vec<_> = projects.iter().filter(|p| p.is_active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "web");
    cleanup(&workspace);
}

#[test]
fn test_the_cleared_markers_mean_no_project() {
    let (workspace, state) = workspace_with(&["api"]);
    for marker in ["none", "system", ""] {
        std::fs::write(selection_path(&state), marker).unwrap();
        assert!(
            read_selection(&state).is_none(),
            "«{marker}» significa sin proyecto"
        );
        assert!(resolve(&workspace, &state, None).active.is_none());
    }
    cleanup(&workspace);
}
