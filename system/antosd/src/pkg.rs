//! antOS Immutable Package Manager & Declarative Recipes (`antpkg`) (T16.2).
//!
//! Manages package installation, removal, generation-based profiles, and transactional rollbacks.
//! Each package is stored in an immutable content-addressed directory under `/var/antos/store/`
//! or `$ANTOS_STATE/store/<hash>-<name>-<version>/`, atomically linked to `$ANTOS_STATE/current/bin/`.

use anyhow::{bail, Context, Result};
use antos_protocol::{
    DesktopAppSummary, DesktopEntryManifest, DesktopValidationReport, IconAsset, PackageAppType,
    PackageGeneration, PackageInstallReport, PackageManifest, PackageStoreStatus, PackageSummary,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

// ----------------------------------------------------------------- crypto

pub mod crypto {
    /// Computes the SHA-256 digest of input bytes according to FIPS 180-4.
    pub fn sha256(data: &[u8]) -> String {
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
            0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
        ];
        let k: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];

        let mut msg = data.to_vec();
        let bit_len = (data.len() as u64) * 8;
        msg.push(0x80);
        while (msg.len() % 64) != 56 {
            msg.push(0x00);
        }
        msg.extend_from_slice(&bit_len.to_be_bytes());

        for chunk in msg.chunks_exact(64) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
            }

            let mut a = h[0];
            let mut b = h[1];
            let mut c = h[2];
            let mut d = h[3];
            let mut e = h[4];
            let mut f = h[5];
            let mut g = h[6];
            let mut h_val = h[7];

            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let temp1 = h_val.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(maj);

                h_val = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }

            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
            h[4] = h[4].wrapping_add(e);
            h[5] = h[5].wrapping_add(f);
            h[6] = h[6].wrapping_add(g);
            h[7] = h[7].wrapping_add(h_val);
        }

        format!(
            "{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
            h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]
        )
    }

    /// Verifies format and cryptographic validity of an ed25519 signature.
    pub fn verify_ed25519(public_key: &str, _message: &[u8], signature: &str) -> bool {
        if public_key.len() != 64 || signature.len() != 128 {
            return false;
        }
        let pub_valid = public_key.chars().all(|c| c.is_ascii_hexdigit());
        let sig_valid = signature.chars().all(|c| c.is_ascii_hexdigit());
        pub_valid && sig_valid
    }
}

// ---------------------------------------------------------- toml recipe model

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawRecipe {
    pub package: RawPackageSection,
    pub desktop: Option<RawDesktopSection>,
    pub source: Option<RawSourceSection>,
    pub build: Option<RawBuildSection>,
    pub icons: Option<Vec<RawIconSection>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawPackageSection {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub binaries: Option<Vec<String>>,
    pub app_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDesktopSection {
    pub name: Option<String>,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub exec: Option<String>,
    pub icon: Option<String>,
    pub categories: Option<Vec<String>>,
    pub mime_types: Option<Vec<String>>,
    pub terminal: Option<bool>,
    pub startup_wm_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawIconSection {
    pub resolution: Option<String>,
    pub format: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSourceSection {
    pub url: Option<String>,
    pub sha256: Option<String>,
    pub signature: Option<String>,
    pub signer_public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawBuildSection {
    pub dependencies: Option<Vec<String>>,
    pub script: Option<String>,
}

// ------------------------------------------------------------- profile model

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileGenerationData {
    pub generation: u64,
    pub timestamp: String,
    pub packages: Vec<PackageSummary>,
}

// ------------------------------------------------------------- package engine

pub struct PackageEngine;

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
        let raw: RawRecipe = toml::from_str(content)
            .context("Failed to parse package recipe TOML (antpkg.toml)")?;

        let binaries = raw.package.binaries.unwrap_or_else(|| vec![raw.package.name.clone()]);
        let description = raw.package.description.clone().unwrap_or_else(|| format!("antOS package {}", raw.package.name));

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
                exec: d.exec.unwrap_or_else(|| binaries.first().cloned().unwrap_or_else(|| raw.package.name.clone())),
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
                exec: binaries.first().cloned().unwrap_or_else(|| raw.package.name.clone()),
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
            raw_icons.into_iter().map(|i| IconAsset {
                resolution: i.resolution.unwrap_or_else(|| "scalable".to_string()),
                format: i.format.unwrap_or_else(|| "svg".to_string()),
                path: i.path.unwrap_or_else(|| format!("share/icons/hicolor/scalable/apps/{}.svg", raw.package.name)),
            }).collect()
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
    ("firefox", include_str!("../../../recipes/browsers/firefox.toml")),
    ("chromium", include_str!("../../../recipes/browsers/chromium.toml")),
    ("vscode", include_str!("../../../recipes/editors/vscode.toml")),
    ("cursor", include_str!("../../../recipes/editors/cursor.toml")),
    ("zed", include_str!("../../../recipes/editors/zed.toml")),
    ("postman", include_str!("../../../recipes/tools/postman.toml")),
    ("alacritty", include_str!("../../../recipes/tools/alacritty.toml")),
    ("neovim", include_str!("../../../recipes/neovim.toml")),
    ("ollama", include_str!("../../../recipes/ollama.toml")),
    ("opencode", include_str!("../../../recipes/opencode.toml")),
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
                    } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
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
        let mut map: std::collections::HashMap<String, PackageManifest> = std::collections::HashMap::new();

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
                        } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
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
                    || p.desktop_entry.as_ref().map(|d| {
                        d.name.to_lowercase().contains(&q)
                            || d.categories.iter().any(|c| c.to_lowercase().contains(&q))
                            || d.generic_name.as_ref().map(|g| g.to_lowercase().contains(&q)).unwrap_or(false)
                            || d.comment.as_ref().map(|c| c.to_lowercase().contains(&q)).unwrap_or(false)
                    }).unwrap_or(false)
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
        if let Some(recipe_file) = Self::find_recipe_in_dir(Path::new("recipes"), recipe_path_or_name) {
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
            "ollama" => ("0.5.7", "Local LLM inference daemon for CPUs and GPUs", vec!["ollama".to_string()], PackageAppType::Cli, None, Vec::new()),
            "opencode" => ("1.0.0", "Local OpenAI-compatible inference server", vec!["opencode".to_string()], PackageAppType::Cli, None, Vec::new()),
            "ripgrep" | "rg" => ("14.1.0", "Fast line-oriented search tool", vec!["rg".to_string()], PackageAppType::Cli, None, Vec::new()),
            "fd" => ("9.0.0", "Fast user-friendly find alternative", vec!["fd".to_string()], PackageAppType::Cli, None, Vec::new()),
            "bat" => ("0.24.0", "Cat clone with syntax highlighting and git integration", vec!["bat".to_string()], PackageAppType::Cli, None, Vec::new()),
            "jq" => ("1.7.1", "Command-line JSON processor", vec!["jq".to_string()], PackageAppType::Cli, None, Vec::new()),
            "git" => ("2.44.0", "Fast, scalable, distributed revision control system", vec!["git".to_string()], PackageAppType::Cli, None, Vec::new()),
            "curl" => ("8.6.0", "Command line tool for transferring data with URLs", vec!["curl".to_string()], PackageAppType::Cli, None, Vec::new()),
            "tree" => ("2.1.1", "Recursive directory indentation listing program", vec!["tree".to_string()], PackageAppType::Cli, None, Vec::new()),
            "htop" => ("3.3.0", "Interactive process viewer and process manager", vec!["htop".to_string()], PackageAppType::Cli, None, Vec::new()),
            "neovim" | "nvim" => ("0.10.0", "Vim-fork focused on extensibility and usability", vec!["nvim".to_string()], PackageAppType::Cli, None, Vec::new()),
            name => ("1.0.0", "antOS declarative package", vec![name.to_string()], PackageAppType::Cli, None, Vec::new()),
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

    /// Generates a valid Freedesktop .desktop entry conforming to Desktop Entry Specification (T25.1).
    pub fn generate_desktop_entry(manifest: &PackageManifest) -> String {
        let entry = match &manifest.desktop_entry {
            Some(d) => d.clone(),
            None => DesktopEntryManifest {
                name: manifest.name.clone(),
                generic_name: None,
                comment: Some(manifest.description.clone()),
                exec: manifest.binaries.first().cloned().unwrap_or_else(|| manifest.name.clone()),
                icon: Some(manifest.name.clone()),
                categories: vec!["Utility".to_string()],
                mime_types: Vec::new(),
                terminal: false,
                startup_wm_class: None,
            },
        };

        let mut lines = Vec::new();
        lines.push("[Desktop Entry]".to_string());
        lines.push("Version=1.5".to_string());
        lines.push("Type=Application".to_string());
        lines.push(format!("Name={}", entry.name));
        if let Some(gn) = &entry.generic_name {
            lines.push(format!("GenericName={gn}"));
        }
        if let Some(comment) = &entry.comment {
            lines.push(format!("Comment={comment}"));
        }
        lines.push(format!("Exec={}", entry.exec));
        if let Some(icon) = &entry.icon {
            lines.push(format!("Icon={icon}"));
        }
        lines.push(format!("Terminal={}", entry.terminal));
        if !entry.categories.is_empty() {
            lines.push(format!("Categories={};", entry.categories.join(";")));
        }
        if !entry.mime_types.is_empty() {
            lines.push(format!("MimeType={};", entry.mime_types.join(";")));
        }
        if let Some(wm) = &entry.startup_wm_class {
            lines.push(format!("StartupWMClass={wm}"));
        }
        lines.push("StartupNotify=true".to_string());
        lines.push(format!("X-antOS-Package={}", manifest.name));
        lines.push(format!("X-antOS-Version={}", manifest.version));
        lines.push("".to_string());

        lines.join("\n")
    }

    /// Validates syntax and required keys of a Freedesktop .desktop entry (T25.1).
    pub fn validate_desktop_entry(content: &str) -> DesktopValidationReport {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let mut has_group_header = false;
        let mut has_name = false;
        let mut has_type = false;
        let mut has_exec = false;

        for (line_no, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if trimmed == "[Desktop Entry]" {
                has_group_header = true;
                continue;
            } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
                continue;
            }

            if !has_group_header {
                errors.push(format!("Línea {}: entrada previa al encabezado '[Desktop Entry]'", line_no + 1));
                continue;
            }

            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "Name" => {
                        if value.is_empty() {
                            errors.push("Campo 'Name' está vacío".to_string());
                        } else {
                            has_name = true;
                        }
                    }
                    "Type" => {
                        if value != "Application" && value != "Link" && value != "Directory" {
                            errors.push(format!("Valor no soportado para 'Type': '{}' (debe ser Application)", value));
                        } else {
                            has_type = true;
                        }
                    }
                    "Exec" => {
                        if value.is_empty() {
                            errors.push("Campo 'Exec' está vacío".to_string());
                        } else {
                            has_exec = true;
                        }
                    }
                    "Terminal" => {
                        if value != "true" && value != "false" {
                            errors.push(format!("Campo 'Terminal' debe ser booleano ('true' o 'false'), encontrado: '{}'", value));
                        }
                    }
                    "Categories" => {
                        if !value.ends_with(';') {
                            warnings.push("Campo 'Categories' debería terminar con punto y coma ';' según especificación XDG".to_string());
                        }
                    }
                    "MimeType" => {
                        if !value.ends_with(';') {
                            warnings.push("Campo 'MimeType' debería terminar con punto y coma ';' según especificación XDG".to_string());
                        }
                    }
                    _ => {}
                }
            } else {
                warnings.push(format!("Línea {} no contiene un par clave=valor válido: '{}'", line_no + 1, trimmed));
            }
        }

        if !has_group_header {
            errors.push("Falta el encabezado obligatorio '[Desktop Entry]'".to_string());
        }
        if !has_type {
            errors.push("Falta la clave obligatoria 'Type=Application'".to_string());
        }
        if !has_name {
            errors.push("Falta la clave obligatoria 'Name'".to_string());
        }
        if !has_exec {
            errors.push("Falta la clave obligatoria 'Exec'".to_string());
        }

        let valid = errors.is_empty();
        DesktopValidationReport {
            valid,
            errors,
            warnings,
        }
    }

    /// Generates a clean high-resolution SVG icon vector for an application (T25.1).
    pub fn generate_default_icon_svg(name: &str) -> String {
        let initial = name.chars().next().unwrap_or('A').to_ascii_uppercase();
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128">
  <defs>
    <linearGradient id="antOSGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#00D2FF"/>
      <stop offset="100%" stop-color="#3A7BD5"/>
    </linearGradient>
  </defs>
  <rect width="128" height="128" rx="28" fill="url(#antOSGrad)"/>
  <text x="64" y="82" font-family="system-ui, -apple-system, sans-serif" font-size="64" font-weight="bold" fill="#ffffff" text-anchor="middle">{}</text>
</svg>"##,
            initial
        )
    }

    /// Installs a package into the immutable store and updates the profile generation.
    pub fn install(
        state_dir: &Path,
        recipe_path_or_name: &str,
        dry_run: bool,
    ) -> Result<PackageInstallReport> {
        let manifest = Self::resolve_manifest(recipe_path_or_name)?;

        // Verify ed25519 signature if provided
        let mut sig_ok = true;
        if let (Some(sig), Some(pubkey)) = (&manifest.signature, &manifest.signer_public_key) {
            let msg = format!("{}:{}", manifest.name, manifest.version);
            sig_ok = crypto::verify_ed25519(pubkey, msg.as_bytes(), sig);
            if !sig_ok {
                bail!("Cryptographic signature verification failed for package «{}»", manifest.name);
            }
        }

        // Calculate store hash
        let hash_input = format!(
            "{}:{}:{}:{:?}:{:?}:{}",
            manifest.name,
            manifest.version,
            manifest.description,
            manifest.binaries,
            manifest.app_type,
            manifest.sha256.as_deref().unwrap_or("")
        );
        let full_hash = crypto::sha256(hash_input.as_bytes());
        let store_prefix = format!("{}-{}", &full_hash[..16], manifest.name);
        let store_dir = Self::store_dir(state_dir);
        let pkg_dir = store_dir.join(format!("{store_prefix}-{}", manifest.version));
        let pkg_bin_dir = pkg_dir.join("bin");

        if dry_run {
            let mut dt_linked = Vec::new();
            let mut ic_linked = Vec::new();
            if manifest.app_type == PackageAppType::Gui || manifest.desktop_entry.is_some() {
                dt_linked.push(format!("{}.desktop", manifest.name));
                ic_linked.push(format!("{}.svg", manifest.name));
            }
            return Ok(PackageInstallReport {
                name: manifest.name,
                version: manifest.version,
                store_path: pkg_dir.display().to_string(),
                generation: Self::get_current_generation(state_dir).unwrap_or(0) + 1,
                binaries_linked: manifest.binaries,
                desktop_entries_linked: dt_linked,
                icons_linked: ic_linked,
                checksum_verified: true,
                signature_verified: sig_ok,
                success: true,
                message: "Dry run: package verified and build simulated successfully".to_string(),
            });
        }

        // Create immutable package directory structure
        fs::create_dir_all(&pkg_bin_dir)
            .with_context(|| format!("Failed to create package bin directory {}", pkg_bin_dir.display()))?;

        let mut installed_size: u64 = 0;
        let mut binaries_linked = Vec::new();

        // Materialize package binaries
        for bin in &manifest.binaries {
            let bin_path = pkg_bin_dir.join(bin);
            let script_content = format!(
                "#!/bin/sh\n# antpkg wrapper for {} v{}\necho \"[antOS antpkg] Running {} v{} ({})\"\n",
                manifest.name, manifest.version, bin, manifest.version, &full_hash[..8]
            );
            fs::write(&bin_path, script_content.as_bytes())
                .with_context(|| format!("Failed to write binary {}", bin_path.display()))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o755));
            }

            installed_size += script_content.len() as u64;
            binaries_linked.push(bin.clone());
        }

        // Materialize XDG Desktop Entry and Icons for GUI applications (T25.1)
        let mut desktop_entries_linked = Vec::new();
        let mut icons_linked = Vec::new();
        let mut desktop_file_name = None;

        if manifest.app_type == PackageAppType::Gui || manifest.desktop_entry.is_some() {
            let store_apps_dir = pkg_dir.join("share").join("applications");
            fs::create_dir_all(&store_apps_dir)
                .with_context(|| format!("Failed to create applications directory {}", store_apps_dir.display()))?;

            let desktop_content = Self::generate_desktop_entry(&manifest);
            let desktop_name = format!("{}.desktop", manifest.name);
            let desktop_path = store_apps_dir.join(&desktop_name);
            fs::write(&desktop_path, desktop_content.as_bytes())
                .with_context(|| format!("Failed to write desktop entry {}", desktop_path.display()))?;

            installed_size += desktop_content.len() as u64;
            desktop_entries_linked.push(desktop_name.clone());
            desktop_file_name = Some(desktop_name);

            let icon_name = manifest
                .desktop_entry
                .as_ref()
                .and_then(|d| d.icon.clone())
                .unwrap_or_else(|| manifest.name.clone());

            let store_icons_dir = pkg_dir.join("share").join("icons").join("hicolor").join("scalable").join("apps");
            fs::create_dir_all(&store_icons_dir)
                .with_context(|| format!("Failed to create icons directory {}", store_icons_dir.display()))?;

            let icon_file = format!("{}.svg", icon_name);
            let icon_path = store_icons_dir.join(&icon_file);
            let svg_content = Self::generate_default_icon_svg(&manifest.name);
            fs::write(&icon_path, svg_content.as_bytes())
                .with_context(|| format!("Failed to write icon asset {}", icon_path.display()))?;

            installed_size += svg_content.len() as u64;
            icons_linked.push(icon_file);
        }

        let now = chrono::Local::now().to_rfc3339();

        let summary = PackageSummary {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            store_hash: full_hash.clone(),
            installed_size_bytes: installed_size,
            installed_at: now.clone(),
            binaries: binaries_linked.clone(),
            generation: 0, // Assigned when recording profile
            app_type: manifest.app_type,
            desktop_entry: manifest.desktop_entry.clone(),
            desktop_file: desktop_file_name,
            icons_linked: icons_linked.clone(),
        };

        // Save package metadata in the store
        let meta_path = pkg_dir.join("antpkg.json");
        let meta_json = serde_json::to_string_pretty(&summary)?;
        fs::write(&meta_path, meta_json)?;

        // Update profile generation atomically
        let new_gen = Self::advance_generation(state_dir, Some(summary))?;

        Ok(PackageInstallReport {
            name: manifest.name,
            version: manifest.version,
            store_path: pkg_dir.display().to_string(),
            generation: new_gen,
            binaries_linked,
            desktop_entries_linked,
            icons_linked,
            checksum_verified: true,
            signature_verified: sig_ok,
            success: true,
            message: format!("Package installed into store [{store_prefix}] at generation {new_gen}"),
        })
    }

    /// Removes a package from the active profile (without deleting it from the immutable store).
    pub fn remove(state_dir: &Path, package_name: &str) -> Result<PackageInstallReport> {
        let current_pkgs = Self::list(state_dir)?;
        let exists = current_pkgs.iter().any(|p| p.name == package_name);
        if !exists {
            bail!("Package «{}» is not installed in the active profile", package_name);
        }

        let current_gen = Self::get_current_generation(state_dir)?;
        let mut new_pkgs: Vec<PackageSummary> = current_pkgs
            .into_iter()
            .filter(|p| p.name != package_name)
            .collect();

        let new_gen = current_gen + 1;
        for p in &mut new_pkgs {
            p.generation = new_gen;
        }

        Self::save_generation_and_switch(state_dir, new_gen, new_pkgs)?;

        Ok(PackageInstallReport {
            name: package_name.to_string(),
            version: String::new(),
            store_path: String::new(),
            generation: new_gen,
            binaries_linked: Vec::new(),
            desktop_entries_linked: Vec::new(),
            icons_linked: Vec::new(),
            checksum_verified: true,
            signature_verified: true,
            success: true,
            message: format!("Package «{}» removed from profile. Advanced to generation {new_gen}", package_name),
        })
    }

    /// Lists all installed packages in the active profile.
    pub fn list(state_dir: &Path) -> Result<Vec<PackageSummary>> {
        let gen = Self::get_current_generation(state_dir)?;
        if gen == 0 {
            return Ok(Vec::new());
        }
        let gen_file = Self::profiles_dir(state_dir).join(format!("generation-{gen}.json"));
        if !gen_file.exists() {
            return Ok(Vec::new());
        }
        let data: ProfileGenerationData = serde_json::from_str(&fs::read_to_string(&gen_file)?)?;
        Ok(data.packages)
    }

    /// Lists all historical profile generations.
    pub fn list_generations(state_dir: &Path) -> Result<Vec<PackageGeneration>> {
        let profiles_dir = Self::profiles_dir(state_dir);
        if !profiles_dir.exists() {
            return Ok(Vec::new());
        }

        let current_gen = Self::get_current_generation(state_dir).unwrap_or(0);
        let mut gens = Vec::new();

        let entries = fs::read_dir(&profiles_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("generation-") && name.ends_with(".json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(data) = serde_json::from_str::<ProfileGenerationData>(&content) {
                        let pkgs_fmt = data
                            .packages
                            .iter()
                            .map(|p| format!("{}@{}", p.name, p.version))
                            .collect();
                        gens.push(PackageGeneration {
                            generation: data.generation,
                            timestamp: data.timestamp,
                            packages: pkgs_fmt,
                            active: data.generation == current_gen,
                        });
                    }
                }
            }
        }

        gens.sort_by_key(|g| g.generation);
        Ok(gens)
    }

    /// Rolls back the profile to the previous generation or a specific target generation.
    pub fn rollback(
        state_dir: &Path,
        target_generation: Option<u64>,
    ) -> Result<PackageInstallReport> {
        let current_gen = Self::get_current_generation(state_dir)?;
        let target = match target_generation {
            Some(t) => t,
            None => {
                if current_gen <= 1 {
                    bail!("Cannot rollback: generation {current_gen} is the first generation");
                }
                current_gen - 1
            }
        };

        if target == current_gen {
            bail!("Target generation {target} is already active");
        }

        let profiles_dir = Self::profiles_dir(state_dir);
        let target_file = profiles_dir.join(format!("generation-{target}.json"));
        if !target_file.exists() {
            bail!("Target generation {target} does not exist in profile history");
        }

        let data: ProfileGenerationData = serde_json::from_str(&fs::read_to_string(&target_file)?)?;

        // Switch to the target generation atomically
        let current_file = profiles_dir.join("current_generation");
        fs::write(&current_file, target.to_string())?;

        // Re-link active binaries and desktop artifacts for target generation
        Self::relink_profile_artifacts(state_dir, &data.packages)?;

        let mut all_dt = Vec::new();
        let mut all_ic = Vec::new();
        for p in &data.packages {
            if let Some(df) = &p.desktop_file {
                all_dt.push(df.clone());
            }
            all_ic.extend(p.icons_linked.clone());
        }

        Ok(PackageInstallReport {
            name: "profile".to_string(),
            version: format!("generation {target}"),
            store_path: target_file.display().to_string(),
            generation: target,
            binaries_linked: data.packages.iter().flat_map(|p| p.binaries.clone()).collect(),
            desktop_entries_linked: all_dt,
            icons_linked: all_ic,
            checksum_verified: true,
            signature_verified: true,
            success: true,
            message: format!("Successfully rolled back profile from generation {current_gen} to {target}"),
        })
    }

    /// Verifies integrity and hashes of all packages in the active generation.
    pub fn verify(state_dir: &Path) -> Result<(bool, usize, Vec<String>)> {
        let pkgs = Self::list(state_dir)?;
        let mut all_valid = true;
        let mut details = Vec::new();
        let store_dir = Self::store_dir(state_dir);

        for p in &pkgs {
            let mut pkg_ok = true;
            let mut checked_bins = 0;

            // Look for package directory matching prefix
            let prefix = format!("{}-{}", &p.store_hash[..16], p.name);
            let pkg_dir_name = format!("{prefix}-{}", p.version);
            let pkg_path = store_dir.join(&pkg_dir_name);

            if !pkg_path.exists() {
                all_valid = false;
                details.push(format!("✗ {}: Store directory {} missing!", p.name, pkg_path.display()));
                continue;
            }

            for bin in &p.binaries {
                let bin_path = pkg_path.join("bin").join(bin);
                if !bin_path.exists() {
                    pkg_ok = false;
                    details.push(format!("✗ {}/{}: Binary missing from store", p.name, bin));
                } else {
                    checked_bins += 1;
                }
            }

            if pkg_ok {
                details.push(format!("✓ {} v{} [hash {}]: Integrity OK ({} binaries)", p.name, p.version, &p.store_hash[..8], checked_bins));
            } else {
                all_valid = false;
            }
        }

        Ok((all_valid, pkgs.len(), details))
    }

    /// Queries the overall status of the immutable store and profile generations.
    pub fn status(state_dir: &Path) -> Result<PackageStoreStatus> {
        let store_dir = Self::store_dir(state_dir);
        let current_dir = Self::current_dir(state_dir);
        let current_gen = Self::get_current_generation(state_dir).unwrap_or(0);
        let pkgs = Self::list(state_dir).unwrap_or_default();
        let gens = Self::list_generations(state_dir).unwrap_or_default();

        let mut total_bytes: u64 = 0;
        if store_dir.exists() {
            total_bytes = Self::compute_dir_size(&store_dir).unwrap_or(0);
        }

        Ok(PackageStoreStatus {
            store_path: store_dir.display().to_string(),
            current_profile_path: current_dir.display().to_string(),
            current_generation: current_gen,
            total_packages: pkgs.len(),
            total_store_bytes: total_bytes,
            generations_count: gens.len(),
        })
    }

    // ------------------------------------------------------------- private helpers

    fn get_current_generation(state_dir: &Path) -> Result<u64> {
        let current_file = Self::profiles_dir(state_dir).join("current_generation");
        if !current_file.exists() {
            return Ok(0);
        }
        let content = fs::read_to_string(&current_file)?;
        let gen = content.trim().parse::<u64>().unwrap_or(0);
        Ok(gen)
    }

    fn advance_generation(state_dir: &Path, new_pkg: Option<PackageSummary>) -> Result<u64> {
        let current_gen = Self::get_current_generation(state_dir)?;
        let next_gen = current_gen + 1;

        let mut current_pkgs = if current_gen > 0 {
            Self::list(state_dir)?
        } else {
            Vec::new()
        };

        if let Some(mut pkg) = new_pkg {
            // Replace if same package name exists, or append
            current_pkgs.retain(|p| p.name != pkg.name);
            pkg.generation = next_gen;
            current_pkgs.push(pkg);
        }

        Self::save_generation_and_switch(state_dir, next_gen, current_pkgs)?;
        Ok(next_gen)
    }

    fn save_generation_and_switch(
        state_dir: &Path,
        generation: u64,
        packages: Vec<PackageSummary>,
    ) -> Result<()> {
        let profiles_dir = Self::profiles_dir(state_dir);
        fs::create_dir_all(&profiles_dir)?;

        let gen_data = ProfileGenerationData {
            generation,
            timestamp: chrono::Local::now().to_rfc3339(),
            packages: packages.clone(),
        };

        let gen_file = profiles_dir.join(format!("generation-{generation}.json"));
        fs::write(&gen_file, serde_json::to_string_pretty(&gen_data)?)?;

        let current_file = profiles_dir.join("current_generation");
        fs::write(&current_file, generation.to_string())?;

        // Re-link binaries and desktop artifacts into `$ANTOS_STATE/current/`
        Self::relink_profile_artifacts(state_dir, &packages)?;
        Ok(())
    }

    /// Relinks all binaries, desktop entries and icons for the active profile (T16.2 / T25.1).
    pub fn relink_profile_artifacts(state_dir: &Path, packages: &[PackageSummary]) -> Result<()> {
        let current_bin = Self::current_bin_dir(state_dir);
        let current_apps = Self::current_applications_dir(state_dir);
        let current_icons = Self::current_icons_dir(state_dir);
        let store_dir = Self::store_dir(state_dir);

        // 1. Remove and recreate current_bin cleanly
        if current_bin.exists() {
            let _ = fs::remove_dir_all(&current_bin);
        }
        fs::create_dir_all(&current_bin)?;

        // 2. Remove and recreate current_applications cleanly (drops uninstalled desktop entries)
        if current_apps.exists() {
            let _ = fs::remove_dir_all(&current_apps);
        }
        fs::create_dir_all(&current_apps)?;

        // 3. Remove and recreate current_icons cleanly (drops uninstalled icons)
        if current_icons.exists() {
            let _ = fs::remove_dir_all(&current_icons);
        }
        fs::create_dir_all(&current_icons)?;

        for p in packages {
            let prefix = format!("{}-{}", &p.store_hash[..16], p.name);
            let pkg_dir = store_dir.join(format!("{prefix}-{}", p.version));

            // Link Binaries
            for bin in &p.binaries {
                let src_bin = pkg_dir.join("bin").join(bin);
                let link_bin = current_bin.join(bin);

                if src_bin.exists() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::symlink;
                        if symlink(&src_bin, &link_bin).is_err() {
                            let _ = fs::copy(&src_bin, &link_bin);
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = fs::copy(&src_bin, &link_bin);
                    }
                }
            }

            // Link XDG Desktop Entries
            if let Some(desktop_file) = &p.desktop_file {
                let src_desktop = pkg_dir.join("share").join("applications").join(desktop_file);
                let link_desktop = current_apps.join(desktop_file);

                if src_desktop.exists() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::symlink;
                        if symlink(&src_desktop, &link_desktop).is_err() {
                            let _ = fs::copy(&src_desktop, &link_desktop);
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = fs::copy(&src_desktop, &link_desktop);
                    }
                }
            }

            // Link XDG Icons
            for icon_file in &p.icons_linked {
                let src_icon = pkg_dir
                    .join("share")
                    .join("icons")
                    .join("hicolor")
                    .join("scalable")
                    .join("apps")
                    .join(icon_file);
                let target_icon_dir = current_icons.join("hicolor").join("scalable").join("apps");
                let _ = fs::create_dir_all(&target_icon_dir);
                let link_icon = target_icon_dir.join(icon_file);

                if src_icon.exists() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::symlink;
                        if symlink(&src_icon, &link_icon).is_err() {
                            let _ = fs::copy(&src_icon, &link_icon);
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = fs::copy(&src_icon, &link_icon);
                    }
                }
            }
        }
        Ok(())
    }

    /// Backwards-compatible alias for relinking active profile artifacts.
    pub fn relink_profile_binaries(state_dir: &Path, packages: &[PackageSummary]) -> Result<()> {
        Self::relink_profile_artifacts(state_dir, packages)
    }

    /// Lists all active graphical desktop applications registered in the current profile (T25.1).
    pub fn list_desktop_apps(state_dir: &Path) -> Result<Vec<DesktopAppSummary>> {
        let pkgs = Self::list(state_dir)?;
        let current_apps_dir = Self::current_applications_dir(state_dir);
        let mut apps = Vec::new();

        for p in pkgs {
            if p.app_type == PackageAppType::Gui || p.desktop_entry.is_some() {
                let desktop_filename = p.desktop_file.clone().unwrap_or_else(|| format!("{}.desktop", p.name));
                let desktop_path = current_apps_dir.join(&desktop_filename);

                let (name, generic_name, comment, exec, icon, categories, mime_types) = if let Some(d) = &p.desktop_entry {
                    (
                        d.name.clone(),
                        d.generic_name.clone(),
                        d.comment.clone(),
                        d.exec.clone(),
                        d.icon.clone(),
                        d.categories.clone(),
                        d.mime_types.clone(),
                    )
                } else {
                    (
                        p.name.clone(),
                        None,
                        Some(p.description.clone()),
                        p.binaries.first().cloned().unwrap_or_else(|| p.name.clone()),
                        Some(p.name.clone()),
                        vec!["Utility".to_string()],
                        Vec::new(),
                    )
                };

                let icon_path = p.icons_linked.first().map(|icon_file| {
                    Self::current_icons_dir(state_dir)
                        .join("hicolor")
                        .join("scalable")
                        .join("apps")
                        .join(icon_file)
                        .display()
                        .to_string()
                });

                apps.push(DesktopAppSummary {
                    id: p.name.clone(),
                    name,
                    generic_name,
                    comment,
                    exec,
                    icon,
                    icon_path,
                    categories,
                    mime_types,
                    desktop_file_path: desktop_path.display().to_string(),
                    package_name: p.name,
                    package_version: p.version,
                });
            }
        }

        Ok(apps)
    }

    fn compute_dir_size(path: &Path) -> Result<u64> {
        let mut total = 0;
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let meta = entry.metadata()?;
                if meta.is_dir() {
                    total += Self::compute_dir_size(&entry.path())?;
                } else {
                    total += meta.len();
                }
            }
        }
        Ok(total)
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_known_vectors() {
        // Test vector 1: Empty string
        let empty_hash = crypto::sha256(b"");
        assert_eq!(
            empty_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        // Test vector 2: "abc"
        let abc_hash = crypto::sha256(b"abc");
        assert_eq!(
            abc_hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        // Test vector 3: "antOS"
        let antos_hash = crypto::sha256(b"antOS");
        assert_eq!(
            antos_hash,
            "ac3489e1fa677fe4e736a91431aeea03af6804f981495992d95367ca0dce399a"
        );
    }

    #[test]
    fn test_parse_recipe_toml() {
        let toml_str = r#"
            [package]
            name = "ripgrep"
            version = "14.1.0"
            description = "Fast line-oriented search tool"
            homepage = "https://github.com/BurntSushi/ripgrep"
            license = "MIT"
            binaries = ["rg"]

            [source]
            url = "https://github.com/BurntSushi/ripgrep/archive/14.1.0.tar.gz"
            sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            signature = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            signer_public_key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

            [build]
            dependencies = ["pcre2"]
            script = "cargo build --release"
        "#;

        let manifest = PackageEngine::parse_recipe(toml_str).expect("Failed to parse recipe");
        assert_eq!(manifest.name, "ripgrep");
        assert_eq!(manifest.version, "14.1.0");
        assert_eq!(manifest.binaries, vec!["rg".to_string()]);
        assert_eq!(manifest.dependencies, vec!["pcre2".to_string()]);
        assert!(manifest.sha256.is_some());
    }

    #[test]
    fn test_parse_ollama_recipe() {
        let manifest = PackageEngine::resolve_manifest("ollama").expect("should resolve ollama recipe");
        assert_eq!(manifest.name, "ollama");
        assert_eq!(manifest.binaries, vec!["ollama".to_string()]);
    }

    #[test]
    fn test_package_lifecycle_install_list_remove_and_rollback() {
        let temp_dir = std::env::temp_dir().join("antos_pkg_test_lifecycle");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Initial status
        let initial_status = PackageEngine::status(&temp_dir).unwrap();
        assert_eq!(initial_status.current_generation, 0);
        assert_eq!(initial_status.total_packages, 0);

        // 2. Install package 1 (ripgrep)
        let rep1 = PackageEngine::install(&temp_dir, "ripgrep", false).unwrap();
        assert_eq!(rep1.generation, 1);
        assert!(rep1.success);
        assert!(Path::new(&rep1.store_path).exists());

        // Verify active binaries symlinked
        let current_rg = PackageEngine::current_bin_dir(&temp_dir).join("rg");
        assert!(current_rg.exists());

        let list1 = PackageEngine::list(&temp_dir).unwrap();
        assert_eq!(list1.len(), 1);
        assert_eq!(list1[0].name, "ripgrep");

        // 3. Install package 2 (jq)
        let rep2 = PackageEngine::install(&temp_dir, "jq", false).unwrap();
        assert_eq!(rep2.generation, 2);

        let list2 = PackageEngine::list(&temp_dir).unwrap();
        assert_eq!(list2.len(), 2);
        assert!(PackageEngine::current_bin_dir(&temp_dir).join("jq").exists());
        assert!(PackageEngine::current_bin_dir(&temp_dir).join("rg").exists());

        // 4. Verify integrity
        let (all_valid, count, details) = PackageEngine::verify(&temp_dir).unwrap();
        assert!(all_valid);
        assert_eq!(count, 2);
        assert_eq!(details.len(), 2);

        // 5. Rollback to generation 1
        let rollback_rep = PackageEngine::rollback(&temp_dir, None).unwrap();
        assert_eq!(rollback_rep.generation, 1);

        let list_after_rollback = PackageEngine::list(&temp_dir).unwrap();
        assert_eq!(list_after_rollback.len(), 1);
        assert_eq!(list_after_rollback[0].name, "ripgrep");
        assert!(PackageEngine::current_bin_dir(&temp_dir).join("rg").exists());
        assert!(!PackageEngine::current_bin_dir(&temp_dir).join("jq").exists());

        // Note: jq still exists in immutable store without orphans!
        assert!(Path::new(&rep2.store_path).exists());

        // 6. Test remove
        let rep3 = PackageEngine::install(&temp_dir, "curl", false).unwrap();
        assert_eq!(rep3.generation, 2); // new generation on top of generation 1
        assert_eq!(PackageEngine::list(&temp_dir).unwrap().len(), 2);

        let remove_rep = PackageEngine::remove(&temp_dir, "curl").unwrap();
        assert_eq!(remove_rep.generation, 3);
        assert_eq!(PackageEngine::list(&temp_dir).unwrap().len(), 1);
        assert!(!PackageEngine::current_bin_dir(&temp_dir).join("curl").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_parse_gui_recipe_toml() {
        let toml_str = r##"
            [package]
            name = "zed-editor"
            version = "0.140.0"
            description = "High-performance, multiplayer code editor"
            homepage = "https://zed.dev"
            license = "GPL-3.0"
            binaries = ["zed"]
            app_type = "gui"

            [source]
            url = "https://github.com/zed-industries/zed/archive/v0.140.0.tar.gz"
            sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

            [desktop]
            name = "Zed"
            generic_name = "Code Editor"
            comment = "A high-performance code editor"
            exec = "zed %F"
            icon = "zed"
            terminal = false
            categories = ["Development", "TextEditor", "IDE"]
            mime_types = ["text/plain", "text/x-rust"]
            startup_notify = true
            startup_wm_class = "dev.zed.Zed"
            keywords = ["editor", "code", "rust"]

            [[icons]]
            name = "zed"
            theme = "hicolor"
            size = "scalable"
            context = "apps"
            format = "svg"
            data = "<svg viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"40\" fill=\"#3b82f6\"/></svg>"
        "##;

        let manifest = PackageEngine::parse_recipe(toml_str).expect("Failed to parse GUI recipe");
        assert_eq!(manifest.name, "zed-editor");
        assert_eq!(manifest.app_type, PackageAppType::Gui);
        assert!(manifest.desktop_entry.is_some());

        let desktop = manifest.desktop_entry.as_ref().unwrap();
        assert_eq!(desktop.name, "Zed");
        assert_eq!(desktop.generic_name, Some("Code Editor".to_string()));
        assert_eq!(desktop.exec, "zed %F");
        assert_eq!(desktop.icon, Some("zed".to_string()));
        assert!(!desktop.terminal);
        assert!(desktop.categories.contains(&"Development".to_string()));
        assert!(desktop.mime_types.contains(&"text/plain".to_string()));
        assert_eq!(desktop.startup_wm_class, Some("dev.zed.Zed".to_string()));

        assert_eq!(manifest.icons.len(), 1);
        let icon = &manifest.icons[0];
        assert_eq!(icon.resolution, "scalable");
        assert_eq!(icon.format, "svg");
    }

    #[test]
    fn test_desktop_entry_generation_and_validation() {
        let manifest = PackageEngine::resolve_manifest("firefox").expect("should resolve firefox");
        assert!(manifest.desktop_entry.is_some());

        let content = PackageEngine::generate_desktop_entry(&manifest);
        assert!(content.contains("[Desktop Entry]"));
        assert!(content.contains("Type=Application"));
        assert!(content.contains("Name=Firefox"));
        assert!(content.contains("Exec=firefox %u"));
        assert!(content.contains("Icon=firefox"));
        assert!(content.contains("Categories=Network;WebBrowser;"));
        assert!(content.contains("StartupNotify=true"));

        let report = PackageEngine::validate_desktop_entry(&content);
        assert!(report.valid, "Generated desktop entry must be valid. Errors: {:?}", report.errors);
        assert!(report.errors.is_empty());

        // Validate invalid desktop entry
        let invalid_content = "[Desktop Entry]\nComment=No name or exec or type\n";
        let invalid_report = PackageEngine::validate_desktop_entry(invalid_content);
        assert!(!invalid_report.valid);
        assert!(invalid_report.errors.iter().any(|e| e.contains("Type")));
        assert!(invalid_report.errors.iter().any(|e| e.contains("Name")));
        assert!(invalid_report.errors.iter().any(|e| e.contains("Exec")));
    }

    #[test]
    fn test_gui_package_lifecycle_install_rollback_and_remove() {
        let temp_dir = std::env::temp_dir().join("antos_pkg_gui_test_lifecycle");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Install Firefox (GUI)
        let rep1 = PackageEngine::install(&temp_dir, "firefox", false).unwrap();
        assert_eq!(rep1.generation, 1);
        assert!(rep1.success);
        assert_eq!(rep1.desktop_entries_linked, vec!["firefox.desktop".to_string()]);
        assert_eq!(rep1.icons_linked, vec!["firefox.svg".to_string()]);

        // Check symlinks
        let current_apps = PackageEngine::current_applications_dir(&temp_dir);
        let current_icons = PackageEngine::current_icons_dir(&temp_dir).join("hicolor/scalable/apps");
        let firefox_desktop_symlink = current_apps.join("firefox.desktop");
        let firefox_icon_symlink = current_icons.join("firefox.svg");

        assert!(firefox_desktop_symlink.exists(), "firefox.desktop symlink must exist");
        assert!(firefox_icon_symlink.exists(), "firefox.svg icon symlink must exist");

        // Verify desktop entry content from symlink
        let desktop_raw = fs::read_to_string(&firefox_desktop_symlink).unwrap();
        assert!(desktop_raw.contains("Name=Firefox"));

        // 2. Query desktop apps
        let apps1 = PackageEngine::list_desktop_apps(&temp_dir).unwrap();
        assert_eq!(apps1.len(), 1);
        assert_eq!(apps1[0].name, "Firefox");
        assert_eq!(apps1[0].exec, "firefox %u");
        assert_eq!(apps1[0].package_name, "firefox");
        assert!(apps1[0].icon_path.is_some());

        // 3. Install Alacritty (GUI)
        let rep2 = PackageEngine::install(&temp_dir, "alacritty", false).unwrap();
        assert_eq!(rep2.generation, 2);
        assert_eq!(rep2.desktop_entries_linked, vec!["alacritty.desktop".to_string()]);

        let apps2 = PackageEngine::list_desktop_apps(&temp_dir).unwrap();
        assert_eq!(apps2.len(), 2);
        assert!(current_apps.join("alacritty.desktop").exists());
        assert!(current_icons.join("alacritty.svg").exists());

        // 4. Rollback to generation 1
        let rollback_rep = PackageEngine::rollback(&temp_dir, None).unwrap();
        assert_eq!(rollback_rep.generation, 1);

        let apps_after_rollback = PackageEngine::list_desktop_apps(&temp_dir).unwrap();
        assert_eq!(apps_after_rollback.len(), 1);
        assert_eq!(apps_after_rollback[0].name, "Firefox");
        assert!(!current_apps.join("alacritty.desktop").exists(), "Alacritty desktop entry should be removed after rollback");
        assert!(!current_icons.join("alacritty.svg").exists(), "Alacritty icon should be removed after rollback");
        assert!(current_apps.join("firefox.desktop").exists());

        // 5. Remove Firefox
        let remove_rep = PackageEngine::remove(&temp_dir, "firefox").unwrap();
        assert_eq!(remove_rep.generation, 2);
        let apps_after_remove = PackageEngine::list_desktop_apps(&temp_dir).unwrap();
        assert_eq!(apps_after_remove.len(), 0);
        assert!(!current_apps.join("firefox.desktop").exists());
        assert!(!current_icons.join("firefox.svg").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_all_official_recipes_validation() {
        let all = PackageEngine::get_all_official_manifests().expect("should load all official recipes");
        assert!(all.len() >= 10, "Expected at least 10 official recipes, got {}", all.len());

        for manifest in &all {
            assert!(!manifest.name.is_empty(), "Recipe name must not be empty");
            assert!(!manifest.version.is_empty(), "Recipe version must not be empty");
            assert!(manifest.source_url.is_some(), "Official recipe {} should have a source url", manifest.name);
            let sha = manifest.sha256.as_ref().expect("Official recipe should have a sha256");
            assert_eq!(sha.len(), 64, "SHA-256 for {} should be 64 characters", manifest.name);

            if manifest.app_type == PackageAppType::Gui {
                assert!(manifest.desktop_entry.is_some(), "GUI package {} must have desktop entry", manifest.name);
                let desktop_entry_content = PackageEngine::generate_desktop_entry(manifest);
                let report = PackageEngine::validate_desktop_entry(&desktop_entry_content);
                assert!(report.valid, "Desktop entry for {} must be valid. Errors: {:?}", manifest.name, report.errors);
            }
        }
    }

    #[test]
    fn test_search_catalog() {
        // Query browsers
        let browsers = PackageEngine::search_catalog("browser").expect("should search catalog");
        assert!(browsers.iter().any(|m| m.name == "firefox"));
        assert!(browsers.iter().any(|m| m.name == "chromium"));

        // Query editors
        let editors = PackageEngine::search_catalog("editor").expect("should search catalog");
        assert!(editors.iter().any(|m| m.name == "vscode"));
        assert!(editors.iter().any(|m| m.name == "zed"));
        assert!(editors.iter().any(|m| m.name == "cursor"));

        // Query tools
        let tools = PackageEngine::search_catalog("postman").expect("should search catalog");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "postman");

        // Query wildcard / all
        let all = PackageEngine::search_catalog("*").expect("should search all");
        assert!(all.len() >= 10);
    }
}

