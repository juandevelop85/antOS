//! Generación y validación de entradas `.desktop` (Freedesktop Desktop
//! Entry Specification, T25.1).
use super::*;

impl PackageEngine {
    /// Generates a valid Freedesktop .desktop entry conforming to Desktop Entry Specification (T25.1).
    pub fn generate_desktop_entry(manifest: &PackageManifest) -> String {
        let entry = match &manifest.desktop_entry {
            Some(d) => d.clone(),
            None => DesktopEntryManifest {
                name: manifest.name.clone(),
                generic_name: None,
                comment: Some(manifest.description.clone()),
                exec: manifest
                    .binaries
                    .first()
                    .cloned()
                    .unwrap_or_else(|| manifest.name.clone()),
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
                errors.push(format!(
                    "Línea {}: entrada previa al encabezado '[Desktop Entry]'",
                    line_no + 1
                ));
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
                            errors.push(format!(
                                "Valor no soportado para 'Type': '{}' (debe ser Application)",
                                value
                            ));
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
                    "MimeType" if !value.ends_with(';') => {
                        warnings.push("Campo 'MimeType' debería terminar con punto y coma ';' según especificación XDG".to_string());
                    }
                    _ => {}
                }
            } else {
                warnings.push(format!(
                    "Línea {} no contiene un par clave=valor válido: '{}'",
                    line_no + 1,
                    trimmed
                ));
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
}
