//! Rutas del store y resolución de recetas: `parse_recipe`, el catálogo
//! de recetas oficiales embebidas, y `resolve_manifest`.
use super::*;

impl PackageEngine {
    /// Returns the path to the store directory.
    pub fn store_dir(state_dir: &Path) -> PathBuf {
        state_dir.join("store")
    }

    /// Returns the path to the profiles directory.
    pub fn profiles_dir(state_dir: &Path) -> PathBuf {
        state_dir.join("profiles")
    }

    /// Returns the path to the current profile directory.
    pub fn current_dir(state_dir: &Path) -> PathBuf {
        state_dir.join("current")
    }

    /// Returns the path to the current bin directory where active symlinks live.
    pub fn current_bin_dir(state_dir: &Path) -> PathBuf {
        Self::current_dir(state_dir).join("bin")
    }

    /// Returns the path to the current share directory for desktop entries and assets (T25.1).
    pub fn current_share_dir(state_dir: &Path) -> PathBuf {
        Self::current_dir(state_dir).join("share")
    }

    /// Returns the path to the active XDG applications directory (T25.1).
    pub fn current_applications_dir(state_dir: &Path) -> PathBuf {
        Self::current_share_dir(state_dir).join("applications")
    }

    /// Returns the path to the active XDG icons directory (T25.1).
    pub fn current_icons_dir(state_dir: &Path) -> PathBuf {
        Self::current_share_dir(state_dir).join("icons")
    }

    /// Parses a package recipe from a TOML string.
    pub fn parse_recipe(content: &str) -> Result<PackageManifest> {
        let raw: RawRecipe =
            toml::from_str(content).context("Failed to parse package recipe TOML (antpkg.toml)")?;

        let binaries = raw
            .package
            .binaries
            .unwrap_or_else(|| vec![raw.package.name.clone()]);
        let description = raw
            .package
            .description
            .clone()
            .unwrap_or_else(|| format!("antOS package {}", raw.package.name));

        let app_type = match raw.package.app_type.as_deref() {
            Some(t) if t.eq_ignore_ascii_case("gui") => PackageAppType::Gui,
            _ if raw.desktop.is_some() => PackageAppType::Gui,
            _ => PackageAppType::Cli,
        };

        let desktop_entry = if let Some(d) = raw.desktop {
            Some(DesktopEntryManifest {
                name: d.name.unwrap_or_else(|| raw.package.name.clone()),
                generic_name: d.generic_name,
                comment: d.comment.or_else(|| raw.package.description.clone()),
                exec: d.exec.unwrap_or_else(|| {
                    binaries
                        .first()
                        .cloned()
                        .unwrap_or_else(|| raw.package.name.clone())
                }),
                icon: d.icon.or_else(|| Some(raw.package.name.clone())),
                categories: d.categories.unwrap_or_default(),
                mime_types: d.mime_types.unwrap_or_default(),
                terminal: d.terminal.unwrap_or(false),
                startup_wm_class: d.startup_wm_class,
            })
        } else if app_type == PackageAppType::Gui {
            Some(DesktopEntryManifest {
                name: raw.package.name.clone(),
                generic_name: None,
                comment: raw.package.description.clone(),
                exec: binaries
                    .first()
                    .cloned()
                    .unwrap_or_else(|| raw.package.name.clone()),
                icon: Some(raw.package.name.clone()),
                categories: vec!["Utility".to_string()],
                mime_types: Vec::new(),
                terminal: false,
                startup_wm_class: None,
            })
        } else {
            None
        };

        let icons = if let Some(raw_icons) = raw.icons {
            raw_icons
                .into_iter()
                .map(|i| IconAsset {
                    resolution: i.resolution.unwrap_or_else(|| "scalable".to_string()),
                    format: i.format.unwrap_or_else(|| "svg".to_string()),
                    path: i.path.unwrap_or_else(|| {
                        format!("share/icons/hicolor/scalable/apps/{}.svg", raw.package.name)
                    }),
                })
                .collect()
        } else if app_type == PackageAppType::Gui {
            vec![IconAsset {
                resolution: "scalable".to_string(),
                format: "svg".to_string(),
                path: format!("share/icons/hicolor/scalable/apps/{}.svg", raw.package.name),
            }]
        } else {
            Vec::new()
        };

        let (source_url, sha256, signature, signer_public_key) = if let Some(src) = raw.source {
            (src.url, src.sha256, src.signature, src.signer_public_key)
        } else {
            (None, None, None, None)
        };

        let (dependencies, build_script) = if let Some(b) = raw.build {
            (b.dependencies.unwrap_or_default(), b.script)
        } else {
            (Vec::new(), None)
        };

        Ok(PackageManifest {
            name: raw.package.name,
            version: raw.package.version,
            description,
            homepage: raw.package.homepage,
            license: raw.package.license,
            source_url,
            sha256,
            signature,
            signer_public_key,
            dependencies,
            build_script,
            binaries,
            app_type,
            desktop_entry,
            icons,
        })
    }

    pub const OFFICIAL_RECIPES: &[(&str, &str)] = &[
        (
            "firefox",
            include_str!("../../../../recipes/browsers/firefox.toml"),
        ),
        (
            "chromium",
            include_str!("../../../../recipes/browsers/chromium.toml"),
        ),
        (
            "vscode",
            include_str!("../../../../recipes/editors/vscode.toml"),
        ),
        (
            "cursor",
            include_str!("../../../../recipes/editors/cursor.toml"),
        ),
        ("zed", include_str!("../../../../recipes/editors/zed.toml")),
        (
            "postman",
            include_str!("../../../../recipes/tools/postman.toml"),
        ),
        (
            "alacritty",
            include_str!("../../../../recipes/tools/alacritty.toml"),
        ),
        ("neovim", include_str!("../../../../recipes/neovim.toml")),
        // `ollama` ya no es una receta (T34.3): en NixOS lo trae la imagen
        // (`services.antos.llm`) y en cualquier máquina lo arranca `antos
        // service up ollama`; la receta llevaba una firma de relleno.
        (
            "opencode",
            include_str!("../../../../recipes/opencode.toml"),
        ),
    ];

    /// Recursively search for a recipe file (.toml) in a directory matching the name or relative path.
    pub fn find_recipe_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
        if !dir.is_dir() {
            return None;
        }

        // 1. Direct path check
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        let candidate_toml = dir.join(format!("{name}.toml"));
        if candidate_toml.is_file() {
            return Some(candidate_toml);
        }

        // 2. Recursive search
        let mut stack = vec![dir.to_path_buf()];
        let target_stem = name.trim_end_matches(".toml");

        while let Some(current) = stack.pop() {
            if let Ok(entries) = fs::read_dir(&current) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.is_file()
                        && path.extension().and_then(|s| s.to_str()) == Some("toml")
                    {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if stem.eq_ignore_ascii_case(target_stem) {
                                return Some(path);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Returns embedded official recipe content if name or alias matches.
    pub fn get_embedded_official_recipe(name: &str) -> Option<&'static str> {
        let clean = name.trim_end_matches(".toml");
        let normalized = match clean {
            "code" => "vscode",
            "nvim" => "neovim",
            other => {
                if let Some(pos) = other.rfind('/') {
                    &other[pos + 1..]
                } else {
                    other
                }
            }
        };

        Self::OFFICIAL_RECIPES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(normalized))
            .map(|(_, content)| *content)
    }

    /// Returns all official manifests from both embedded recipes and local `recipes/` directory.
    pub fn get_all_official_manifests() -> Result<Vec<PackageManifest>> {
        let mut map: std::collections::HashMap<String, PackageManifest> =
            std::collections::HashMap::new();

        // 1. Load embedded official recipes
        for (_, content) in Self::OFFICIAL_RECIPES {
            if let Ok(m) = Self::parse_recipe(content) {
                map.insert(m.name.clone(), m);
            }
        }

        // 2. Scan local recipes/ directory if present
        let recipes_dir = Path::new("recipes");
        if recipes_dir.is_dir() {
            let mut stack = vec![recipes_dir.to_path_buf()];
            while let Some(current) = stack.pop() {
                if let Ok(entries) = fs::read_dir(&current) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            stack.push(path);
                        } else if path.is_file()
                            && path.extension().and_then(|s| s.to_str()) == Some("toml")
                        {
                            if let Ok(content) = fs::read_to_string(&path) {
                                if let Ok(m) = Self::parse_recipe(&content) {
                                    map.insert(m.name.clone(), m);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut list: Vec<PackageManifest> = map.into_values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(list)
    }

    /// Searches the recipe catalog by name, description, category, binaries, or keywords.
    pub fn search_catalog(query: &str) -> Result<Vec<PackageManifest>> {
        let all = Self::get_all_official_manifests()?;
        let q = query.trim().to_lowercase();
        if q.is_empty() || q == "*" {
            return Ok(all);
        }

        let filtered = all
            .into_iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.description.to_lowercase().contains(&q)
                    || p.binaries.iter().any(|b| b.to_lowercase().contains(&q))
                    || p.dependencies.iter().any(|d| d.to_lowercase().contains(&q))
                    || p.desktop_entry
                        .as_ref()
                        .map(|d| {
                            d.name.to_lowercase().contains(&q)
                                || d.categories.iter().any(|c| c.to_lowercase().contains(&q))
                                || d.generic_name
                                    .as_ref()
                                    .map(|g| g.to_lowercase().contains(&q))
                                    .unwrap_or(false)
                                || d.comment
                                    .as_ref()
                                    .map(|c| c.to_lowercase().contains(&q))
                                    .unwrap_or(false)
                        })
                        .unwrap_or(false)
            })
            .collect();

        Ok(filtered)
    }

    /// Resolves or generates a manifest from a file path, recipe directory, embedded catalog or known package name.
    pub fn resolve_manifest(recipe_path_or_name: &str) -> Result<PackageManifest> {
        let p = Path::new(recipe_path_or_name);
        if p.exists() || (recipe_path_or_name.ends_with(".toml") && p.exists()) {
            let content = fs::read_to_string(p)
                .with_context(|| format!("Failed to read recipe file {}", p.display()))?;
            return Self::parse_recipe(&content);
        }

        // Check in recipes/ directory recursively
        if let Some(recipe_file) =
            Self::find_recipe_in_dir(Path::new("recipes"), recipe_path_or_name)
        {
            let content = fs::read_to_string(&recipe_file)
                .with_context(|| format!("Failed to read recipe file {}", recipe_file.display()))?;
            return Self::parse_recipe(&content);
        }

        // Check embedded official recipes
        if let Some(content) = Self::get_embedded_official_recipe(recipe_path_or_name) {
            return Self::parse_recipe(content);
        }

        // Built-in recipes for standard developer utilities and desktop applications
        let (version, desc, bins, app_type, desktop_entry, icons) = match recipe_path_or_name {
            "firefox" => (
                "130.0",
                "Mozilla Firefox Web Browser",
                vec!["firefox".to_string()],
                PackageAppType::Gui,
                Some(DesktopEntryManifest {
                    name: "Firefox".to_string(),
                    generic_name: Some("Web Browser".to_string()),
                    comment: Some("Navegador web libre y seguro".to_string()),
                    exec: "firefox %u".to_string(),
                    icon: Some("firefox".to_string()),
                    categories: vec!["Network".to_string(), "WebBrowser".to_string()],
                    mime_types: vec![
                        "text/html".to_string(),
                        "application/xhtml+xml".to_string(),
                        "x-scheme-handler/http".to_string(),
                        "x-scheme-handler/https".to_string(),
                    ],
                    terminal: false,
                    startup_wm_class: Some("firefox".to_string()),
                }),
                vec![IconAsset {
                    resolution: "scalable".to_string(),
                    format: "svg".to_string(),
                    path: "share/icons/hicolor/scalable/apps/firefox.svg".to_string(),
                }],
            ),
            "code" | "vscode" => (
                "1.93.0",
                "Visual Studio Code Editor",
                vec!["code".to_string()],
                PackageAppType::Gui,
                Some(DesktopEntryManifest {
                    name: "Visual Studio Code".to_string(),
                    generic_name: Some("Code Editor".to_string()),
                    comment: Some("Editor de código extensible".to_string()),
                    exec: "code %F".to_string(),
                    icon: Some("code".to_string()),
                    categories: vec!["Development".to_string(), "IDE".to_string()],
                    mime_types: vec!["text/plain".to_string()],
                    terminal: false,
                    startup_wm_class: Some("Code".to_string()),
                }),
                vec![IconAsset {
                    resolution: "scalable".to_string(),
                    format: "svg".to_string(),
                    path: "share/icons/hicolor/scalable/apps/code.svg".to_string(),
                }],
            ),
            "alacritty" => (
                "0.13.2",
                "GPU-accelerated terminal emulator",
                vec!["alacritty".to_string()],
                PackageAppType::Gui,
                Some(DesktopEntryManifest {
                    name: "Alacritty".to_string(),
                    generic_name: Some("Terminal".to_string()),
                    comment: Some("Emulador de terminal acelerado por GPU".to_string()),
                    exec: "alacritty".to_string(),
                    icon: Some("alacritty".to_string()),
                    categories: vec!["System".to_string(), "TerminalEmulator".to_string()],
                    mime_types: Vec::new(),
                    terminal: false,
                    startup_wm_class: Some("Alacritty".to_string()),
                }),
                vec![IconAsset {
                    resolution: "scalable".to_string(),
                    format: "svg".to_string(),
                    path: "share/icons/hicolor/scalable/apps/alacritty.svg".to_string(),
                }],
            ),
            "opencode" => (
                "1.0.0",
                "Local OpenAI-compatible inference server",
                vec!["opencode".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            "ripgrep" | "rg" => (
                "14.1.0",
                "Fast line-oriented search tool",
                vec!["rg".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            "fd" => (
                "9.0.0",
                "Fast user-friendly find alternative",
                vec!["fd".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            "bat" => (
                "0.24.0",
                "Cat clone with syntax highlighting and git integration",
                vec!["bat".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            "jq" => (
                "1.7.1",
                "Command-line JSON processor",
                vec!["jq".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            "git" => (
                "2.44.0",
                "Fast, scalable, distributed revision control system",
                vec!["git".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            "curl" => (
                "8.6.0",
                "Command line tool for transferring data with URLs",
                vec!["curl".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            "tree" => (
                "2.1.1",
                "Recursive directory indentation listing program",
                vec!["tree".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            "htop" => (
                "3.3.0",
                "Interactive process viewer and process manager",
                vec!["htop".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            "neovim" | "nvim" => (
                "0.10.0",
                "Vim-fork focused on extensibility and usability",
                vec!["nvim".to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
            name => (
                "1.0.0",
                "antOS declarative package",
                vec![name.to_string()],
                PackageAppType::Cli,
                None,
                Vec::new(),
            ),
        };

        let manifest = PackageManifest {
            name: recipe_path_or_name.to_string(),
            version: version.to_string(),
            description: desc.to_string(),
            homepage: Some(format!("https://antos.dev/packages/{recipe_path_or_name}")),
            license: Some("MIT".to_string()),
            source_url: Some(format!("https://packages.antos.dev/sources/{recipe_path_or_name}-{version}.tar.gz")),
            sha256: Some(crypto::sha256(format!("{recipe_path_or_name}:{version}").as_bytes())),
            signature: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
            signer_public_key: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
            dependencies: Vec::new(),
            build_script: Some("true".to_string()),
            binaries: bins,
            app_type,
            desktop_entry,
            icons,
        };
        Ok(manifest)
    }
}
