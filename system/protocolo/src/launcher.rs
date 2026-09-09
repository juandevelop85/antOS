//! Application Launcher and Desktop Context Management (T25.4).
//!
//! Provides the data structures, scoring heuristics, fuzzy match algorithms,
//! and workspace context injection used by `system/barra` and `system/antosd`
//! for the Raycast/Spotlight-style Wayland application launcher.

use crate::system::{DesktopApp, DesktopAppSummary};
use serde::{Deserialize, Serialize};

/// Normalized representation of a desktop application ready for the launcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherAppItem {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub exec: String,
    pub icon: Option<String>,
    pub icon_path: Option<String>,
    pub categories: Vec<String>,
    pub source: String,
    pub is_editor: bool,
}

impl LauncherAppItem {
    /// Creates a new launcher item with detected editor status.
    ///
    /// T31.10: clippy señala `too_many_arguments` (9). Agrupar estos
    /// parámetros en una estructura obligaría a tocar sus siete sitios de
    /// llamada en `system/barra` —un *workspace* aparte que este árbol no
    /// puede compilar ni verificar en este entorno (requiere GTK4, ausente
    /// aquí; ver la nota de T31.10 sobre `system/barra` en el propio
    /// ticket)— sin poder confirmar que el cambio compila. Los nueve campos
    /// son, además, una copia directa y sin relación lógica entre sí de los
    /// campos públicos de `LauncherAppItem`: agruparlos en un struct
    /// intermedio no añadiría significado, solo indirección. Se documenta
    /// la excepción en vez de forzar un refactor no verificable.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        generic_name: Option<String>,
        comment: Option<String>,
        exec: impl Into<String>,
        icon: Option<String>,
        icon_path: Option<String>,
        categories: Vec<String>,
        source: impl Into<String>,
    ) -> Self {
        let id_str = id.into();
        let name_str = name.into();
        let is_editor = Self::check_is_editor(&id_str, &name_str, &categories);

        Self {
            id: id_str,
            name: name_str,
            generic_name,
            comment,
            exec: exec.into(),
            icon,
            icon_path,
            categories,
            source: source.into(),
            is_editor,
        }
    }

    /// Converts a `DesktopAppSummary` (from antpkg) into a launcher item.
    pub fn from_desktop_summary(summary: &DesktopAppSummary) -> Self {
        Self::new(
            &summary.id,
            &summary.name,
            summary.generic_name.clone(),
            summary.comment.clone(),
            &summary.exec,
            summary.icon.clone(),
            summary.icon_path.clone(),
            summary.categories.clone(),
            "antpkg",
        )
    }

    /// Converts a `DesktopApp` (from Flatpak/System) into a launcher item.
    pub fn from_desktop_app(app: &DesktopApp) -> Self {
        Self::new(
            &app.id,
            &app.name,
            None,
            Some(app.description.clone()),
            &app.exec_cmd,
            app.icon.clone(),
            None,
            app.categories.clone(),
            app.source.as_str(),
        )
    }

    /// Detects if an application is an IDE or text editor requiring workspace injection.
    fn check_is_editor(id: &str, name: &str, categories: &[String]) -> bool {
        let id_lower = id.to_lowercase();
        let name_lower = name.to_lowercase();

        let known_editor_ids = [
            "code",
            "vscode",
            "visual-studio-code",
            "com.visualstudio.code",
            "zed",
            "zed-editor",
            "dev.zed.Zed",
            "cursor",
            "cursor-ai",
            "neovim",
            "nvim",
            "io.neovim.nvim",
            "sublime_text",
            "sublime-text",
            "com.sublimetext.three",
            "clion",
            "intellij",
            "pycharm",
            "webstorm",
            "rustrover",
            "alacritty",
            "kitty",
            "foot",
        ];

        if known_editor_ids
            .iter()
            .any(|k| id_lower.contains(k) || name_lower.contains(k))
        {
            return true;
        }

        categories.iter().any(|c| {
            let cl = c.to_lowercase();
            cl == "ide" || cl == "texteditor" || cl == "development"
        })
    }
}

/// Matches and ranks applications against the user input.
pub fn match_applications(query: &str, apps: &[LauncherAppItem]) -> Vec<LauncherAppItem> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }

    // Strip common launcher command prefixes
    let clean_q = if let Some(stripped) = q
        .strip_prefix("open ")
        .or_else(|| q.strip_prefix("abrir "))
        .or_else(|| q.strip_prefix("run "))
        .or_else(|| q.strip_prefix("lanzar "))
    {
        stripped.trim()
    } else {
        &q
    };

    if clean_q.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(i32, LauncherAppItem)> = apps
        .iter()
        .filter_map(|app| {
            let name_lower = app.name.to_lowercase();
            let id_lower = app.id.to_lowercase();
            let gen_lower = app.generic_name.as_deref().unwrap_or("").to_lowercase();
            let exec_lower = app.exec.to_lowercase();

            let mut score = 0;

            if name_lower == clean_q || id_lower == clean_q {
                score += 1000;
            } else if name_lower.starts_with(clean_q) || id_lower.starts_with(clean_q) {
                score += 500;
            } else if name_lower
                .split_whitespace()
                .any(|w| w.starts_with(clean_q))
            {
                score += 300;
            } else if gen_lower.starts_with(clean_q) {
                score += 250;
            } else if name_lower.contains(clean_q) || id_lower.contains(clean_q) {
                score += 100;
            } else if gen_lower.contains(clean_q) || exec_lower.contains(clean_q) {
                score += 50;
            } else if app
                .categories
                .iter()
                .any(|c| c.to_lowercase().contains(clean_q))
            {
                score += 30;
            } else if app
                .comment
                .as_deref()
                .map(|c| c.to_lowercase().contains(clean_q))
                .unwrap_or(false)
            {
                score += 10;
            }

            if score > 0 {
                Some((score, app.clone()))
            } else {
                None
            }
        })
        .collect();

    // Sort descending by score, then alphabetically
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    scored.into_iter().map(|(_, app)| app).collect()
}

/// Determines whether a user input represents an application search or an AI intent/command.
pub fn is_app_query(query: &str, apps: &[LauncherAppItem]) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return false;
    }

    // Explicit application launch verbs
    if q.starts_with("open ")
        || q.starts_with("abrir ")
        || q.starts_with("run ")
        || q.starts_with("lanzar ")
    {
        return true;
    }

    // Explicit AI Intent prefixes that should not activate the application launcher
    let intent_prefixes = [
        "haz ",
        "arregla ",
        "crea ",
        "analiza ",
        "commit",
        "libera ",
        "deshacer",
        "undo",
        "t1.",
        "t2.",
        "t3.",
        "t4.",
        "t5.",
        "t6.",
        "t7.",
        "t8.",
        "t9.",
        "t10.",
        "t11.",
        "t12.",
        "t13.",
        "t14.",
        "t15.",
        "t16.",
        "t17.",
        "t18.",
        "t19.",
        "t20.",
        "t21.",
        "t22.",
        "t23.",
        "t24.",
        "t25.",
        "panel",
        "board",
        "tablero",
        "status",
        "git ",
        "inicia ",
        "muestra ",
        "comprueba ",
        "revisa ",
        "ejecuta prueba",
    ];

    if intent_prefixes.iter().any(|p| q.starts_with(p)) {
        return false;
    }

    // Natural language instructions with 4 or more words are intents
    if q.split_whitespace().count() >= 4 {
        return false;
    }

    // Returns true if there is at least one matching application
    !match_applications(query, apps).is_empty()
}

/// Injects the active workspace path into launch arguments if the application is an editor or IDE.
pub fn inject_workspace_args(
    app: &LauncherAppItem,
    workspace: &str,
    existing_args: &[String],
) -> Vec<String> {
    let mut args = existing_args.to_vec();
    if app.is_editor && (args.is_empty() || !args.iter().any(|a| a == workspace)) {
        args.push(workspace.to_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_apps() -> Vec<LauncherAppItem> {
        vec![
            LauncherAppItem::new(
                "firefox",
                "Firefox",
                Some("Web Browser".to_string()),
                Some("Fast, Private & Safe Web Browser".to_string()),
                "firefox %u",
                Some("firefox".to_string()),
                Some("/share/icons/firefox.svg".to_string()),
                vec!["Network".to_string(), "WebBrowser".to_string()],
                "antpkg",
            ),
            LauncherAppItem::new(
                "chromium",
                "Chromium",
                Some("Web Browser".to_string()),
                Some("Open-source web browser".to_string()),
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
                Some("Extensible code editor".to_string()),
                "code --ozone-platform=wayland %F",
                Some("vscode".to_string()),
                None,
                vec!["Development".to_string(), "IDE".to_string()],
                "antpkg",
            ),
            LauncherAppItem::new(
                "zed",
                "Zed",
                Some("High-performance Code Editor".to_string()),
                Some("Multiplayer code editor written in Rust".to_string()),
                "zed",
                Some("zed".to_string()),
                None,
                vec![
                    "Development".to_string(),
                    "IDE".to_string(),
                    "TextEditor".to_string(),
                ],
                "antpkg",
            ),
            LauncherAppItem::new(
                "alacritty",
                "Alacritty",
                Some("Terminal".to_string()),
                Some("GPU-accelerated terminal emulator".to_string()),
                "alacritty",
                Some("alacritty".to_string()),
                None,
                vec!["System".to_string(), "TerminalEmulator".to_string()],
                "antpkg",
            ),
            LauncherAppItem::new(
                "postman",
                "Postman",
                Some("API Platform".to_string()),
                Some("API Development & Testing".to_string()),
                "postman",
                Some("postman".to_string()),
                None,
                vec!["Development".to_string()],
                "antpkg",
            ),
        ]
    }

    #[test]
    fn test_editor_detection() {
        let apps = sample_apps();
        let vscode = apps.iter().find(|a| a.id == "vscode").unwrap();
        let zed = apps.iter().find(|a| a.id == "zed").unwrap();
        let firefox = apps.iter().find(|a| a.id == "firefox").unwrap();

        assert!(vscode.is_editor);
        assert!(zed.is_editor);
        assert!(!firefox.is_editor);
    }

    #[test]
    fn test_matching_applications() {
        let apps = sample_apps();

        // Exact match
        let res_fire = match_applications("fire", &apps);
        assert!(!res_fire.is_empty());
        assert_eq!(res_fire[0].id, "firefox");

        // Prefix match
        let res_code = match_applications("code", &apps);
        assert!(!res_code.is_empty());
        assert_eq!(res_code[0].id, "vscode");

        // Generic category search
        let res_browser = match_applications("browser", &apps);
        assert!(res_browser.len() >= 2);
        assert!(res_browser.iter().any(|a| a.id == "firefox"));
        assert!(res_browser.iter().any(|a| a.id == "chromium"));

        // With command prefix
        let res_open = match_applications("open zed", &apps);
        assert_eq!(res_open.len(), 1);
        assert_eq!(res_open[0].id, "zed");
    }

    #[test]
    fn test_intent_vs_app_discrimination() {
        let apps = sample_apps();

        // App queries
        assert!(is_app_query("fire", &apps));
        assert!(is_app_query("vscode", &apps));
        assert!(is_app_query("open firefox", &apps));
        assert!(is_app_query("zed", &apps));

        // AI Intent queries
        assert!(!is_app_query("haz commit de los cambios", &apps));
        assert!(!is_app_query("arregla el error en el módulo auth", &apps));
        assert!(!is_app_query("crea rama feature-login", &apps));
        assert!(!is_app_query("libera el puerto 3000", &apps));
        assert!(!is_app_query("T25.4 desarrolla lanzador", &apps));
        assert!(!is_app_query("deshacer ultimo cambio", &apps));
    }

    #[test]
    fn test_inject_workspace_args() {
        let apps = sample_apps();
        let vscode = apps.iter().find(|a| a.id == "vscode").unwrap();
        let firefox = apps.iter().find(|a| a.id == "firefox").unwrap();

        let ws = "/Users/developer/my-project";

        // Editor gets workspace injected if args empty
        let vscode_args = inject_workspace_args(vscode, ws, &[]);
        assert_eq!(vscode_args, vec![ws.to_string()]);

        // Browser does NOT get workspace injected
        let firefox_args = inject_workspace_args(firefox, ws, &[]);
        assert!(firefox_args.is_empty());

        // Editor does not duplicate if already present
        let existing = vec![ws.to_string()];
        let vscode_existing = inject_workspace_args(vscode, ws, &existing);
        assert_eq!(vscode_existing.len(), 1);
    }
}
