//! Motor de especificaciones y parser nativo de tickets Markdown para antOS (T1.3).
//!
//! Este módulo indexa y parsea en tiempo real los tickets ubicados en
//! `docs/tickets/` o `specs/`, extrayendo su ID, fase, estado, descripción,
//! alcance técnico y criterios de aceptación para el CLI (`antos tickets`),
//! el demonio IPC y el centro de control de agentes.

use antos_protocol::{TicketDetail, TicketStatus, TicketSummary};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

/// Entrada de caché para el catálogo de tickets de un espacio de trabajo.
#[derive(Debug, Clone)]
struct SpecCacheEntry {
    dir_mtime: Option<SystemTime>,
    readme_mtime: Option<SystemTime>,
    tickets: Vec<TicketSummary>,
    detalles: HashMap<String, (Option<SystemTime>, TicketDetail)>,
}

/// Motor de especificaciones con caché en memoria e invalidación por mtime.
pub struct SpecEngine {
    cache: Mutex<HashMap<PathBuf, SpecCacheEntry>>,
}

static INSTANCIA: OnceLock<SpecEngine> = OnceLock::new();

impl SpecEngine {
    pub fn global() -> &'static SpecEngine {
        INSTANCIA.get_or_init(|| SpecEngine {
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Lista todos los tickets disponibles en el espacio de trabajo.
    pub fn list_tickets(&self, workspace_path: &Path) -> Result<Vec<TicketSummary>> {
        let dir_tickets = find_tickets_dir(workspace_path);
        let Some(dir) = dir_tickets else {
            return Ok(Vec::new());
        };

        let dir_mtime = fs::metadata(&dir).and_then(|m| m.modified()).ok();
        let readme_path = dir.join("README.md");
        let readme_mtime = fs::metadata(&readme_path).and_then(|m| m.modified()).ok();

        // Comprobar caché
        if let Ok(guard) = self.cache.lock() {
            if let Some(entry) = guard.get(&dir) {
                if entry.dir_mtime == dir_mtime && entry.readme_mtime == readme_mtime {
                    return Ok(entry.tickets.clone());
                }
            }
        }

        // Reindexar tickets
        let (tickets, estados_map) = index_tickets_directory(&dir)?;

        // Actualizar caché
        if let Ok(mut guard) = self.cache.lock() {
            let entry = guard.entry(dir.clone()).or_insert_with(|| SpecCacheEntry {
                dir_mtime,
                readme_mtime,
                tickets: Vec::new(),
                detalles: HashMap::new(),
            });
            entry.dir_mtime = dir_mtime;
            entry.readme_mtime = readme_mtime;
            entry.tickets = tickets.clone();

            // Limpiar detalles obsoletos que ya no existan
            entry.detalles.retain(|k, _| estados_map.contains_key(k));
        }

        Ok(tickets)
    }

    #[deprecated]
    pub fn listar_tickets(&self, workspace_path: &Path) -> Result<Vec<TicketSummary>> {
        self.list_tickets(workspace_path)
    }

    /// Obtiene el detalle completo de un ticket específico.
    pub fn get_ticket(
        &self,
        workspace_path: &Path,
        ticket_id: &str,
    ) -> Result<Option<TicketDetail>> {
        let dir_tickets = find_tickets_dir(workspace_path);
        let Some(dir) = dir_tickets else {
            return Ok(None);
        };

        // Asegurar que la lista de tickets esté indexada
        let _ = self.list_tickets(workspace_path)?;

        let id_normalizado = ticket_id.trim().to_uppercase();

        // Buscar archivo correspondiente al ticket
        let mut ruta_archivo = None;
        let mut estado_ticket = TicketStatus::Pendiente;

        if let Ok(guard) = self.cache.lock() {
            if let Some(entry) = guard.get(&dir) {
                if let Some(t) = entry
                    .tickets
                    .iter()
                    .find(|t| t.id.to_uppercase() == id_normalizado)
                {
                    ruta_archivo = Some(PathBuf::from(&t.file_path));
                    estado_ticket = t.status;
                }
            }
        }

        let Some(ruta) = ruta_archivo else {
            return Ok(None);
        };

        let ruta_completa = if ruta.is_absolute() {
            ruta
        } else {
            workspace_path.join(ruta)
        };

        let file_mtime = fs::metadata(&ruta_completa).and_then(|m| m.modified()).ok();

        // Comprobar caché de detalle
        if let Ok(guard) = self.cache.lock() {
            if let Some(entry) = guard.get(&dir) {
                if let Some((cached_mtime, detalle)) = entry.detalles.get(&id_normalizado) {
                    if *cached_mtime == file_mtime {
                        return Ok(Some(detalle.clone()));
                    }
                }
            }
        }

        // Parsear detalle del archivo
        let detalle = parsear_archivo_ticket(&ruta_completa, Some(estado_ticket))?;

        // Guardar en caché
        if let Ok(mut guard) = self.cache.lock() {
            if let Some(entry) = guard.get_mut(&dir) {
                entry
                    .detalles
                    .insert(id_normalizado, (file_mtime, detalle.clone()));
            }
        }

        Ok(Some(detalle))
    }

    #[deprecated]
    pub fn obtener_ticket(
        &self,
        workspace_path: &Path,
        ticket_id: &str,
    ) -> Result<Option<TicketDetail>> {
        self.get_ticket(workspace_path, ticket_id)
    }

    /// Crea un nuevo ticket técnico en el espacio de trabajo activo.
    pub fn create_ticket(
        &self,
        workspace_path: &Path,
        id: &str,
        title: &str,
        description: Option<&str>,
        phase: Option<&str>,
    ) -> Result<PathBuf> {
        let dir = find_or_create_tickets_dir(workspace_path)?;
        let id_clean = id.trim().to_uppercase();
        let slug = slugify(title);
        let filename = format!("{id_clean}-{slug}.md");
        let filepath = dir.join(&filename);

        let phase_str = phase.unwrap_or("Fase Activa");
        let desc_str = description.unwrap_or("Descripción pendiente de especificación detallada.");

        let content = format!(
            "# {id_clean} · {title}\n\n\
            > **Estado:** ⏳ Pendiente  \n\
            > **Fase:** {phase_str}  \n\
            > **Fecha:** {date}\n\n\
            ## Descripción\n\
            {desc_str}\n\n\
            ## Alcance Técnico\n\
            1. **Diseño y Arquitectura:**\n\
               * Definir interfaces y modelos de datos tipados.\n\
            2. **Implementación:**\n\
               * Desarrollar lógica principal y módulos asociados.\n\
            3. **Pruebas y Verificación:**\n\
               * Crear pruebas unitarias y de integración para validar el funcionamiento.\n\n\
            ## Criterios de Aceptación\n\
            * El comando o funcionalidad responde adecuadamente a las intenciones del usuario.\n\
            * `cargo test --workspace` pasa al 100% sin advertencias ni regresiones.\n\
            * La bitácora del sistema y el catálogo de capacidades quedan actualizados.\n",
            date = chrono::Local::now().format("%B %Y")
        );

        fs::write(&filepath, content)?;

        // Actualizar README.md si existe
        let readme_path = dir.join("README.md");
        if readme_path.exists() {
            let mut readme_content = fs::read_to_string(&readme_path).unwrap_or_default();
            if readme_content.contains("| :--- |") || readme_content.contains("| Estado |") {
                let new_row = format!(
                    "| **{phase_str}** | [{id_clean}]({filename}) | {title} | ⏳ Pendiente |\n"
                );
                readme_content.push_str(&new_row);
                let _ = fs::write(&readme_path, readme_content);
            }
        }

        // Invalidar caché
        if let Ok(mut guard) = self.cache.lock() {
            guard.remove(&dir);
        }

        Ok(filepath)
    }

    /// Actualiza el estado de un ticket en su fichero y en el README.md.
    pub fn update_ticket_status(
        &self,
        workspace_path: &Path,
        ticket_id: &str,
        new_status: TicketStatus,
    ) -> Result<()> {
        let dir = find_tickets_dir(workspace_path)
            .ok_or_else(|| anyhow::anyhow!("no se encontró el directorio de tickets"))?;
        let id_upper = ticket_id.trim().to_uppercase();

        let status_label = match new_status {
            TicketStatus::Completado => "✅ Completado",
            TicketStatus::EnProgreso => "🔄 En progreso",
            TicketStatus::EnRevision => "🔍 En revisión",
            TicketStatus::Pendiente => "⏳ Pendiente",
        };

        // 1. Actualizar el fichero individual del ticket
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with(&id_upper) && name.ends_with(".md") {
                        if let Ok(content) = fs::read_to_string(&p) {
                            let mut new_lines = Vec::new();
                            for line in content.lines() {
                                if line.starts_with("> **Estado:**") {
                                    new_lines.push(format!("> **Estado:** {status_label}  "));
                                } else {
                                    new_lines.push(line.to_string());
                                }
                            }
                            let _ = fs::write(&p, new_lines.join("\n"));
                        }
                    }
                }
            }
        }

        // 2. Actualizar README.md si existe
        let readme_path = dir.join("README.md");
        if readme_path.exists() {
            if let Ok(content) = fs::read_to_string(&readme_path) {
                let mut new_lines = Vec::new();
                for line in content.lines() {
                    if line.contains(&format!("[{id_upper}]"))
                        || line.contains(&format!(" {id_upper} "))
                    {
                        let parts: Vec<&str> = line.split('|').collect();
                        if parts.len() >= 5 {
                            let mut updated_parts = parts.clone();
                            let formatted = format!(" {status_label} ");
                            updated_parts[4] = &formatted;
                            new_lines.push(updated_parts.join("|"));
                            continue;
                        }
                    }
                    new_lines.push(line.to_string());
                }
                let _ = fs::write(&readme_path, new_lines.join("\n"));
            }
        }

        // Invalidar caché
        if let Ok(mut guard) = self.cache.lock() {
            guard.remove(&dir);
        }

        Ok(())
    }
}

pub fn slugify(texto: &str) -> String {
    texto
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Computes the default ceiling directory for ticket discovery starting from `workspace_path`.
///
/// Projects living inside `workspace/` (or subdirectories of the antOS repository under `workspace/`)
/// must NOT ascend into the antOS repository root, preventing developer projects from inheriting
/// the OS system tickets.
pub fn default_ceiling_for(workspace_path: &Path) -> Option<PathBuf> {
    let antos_root = crate::git::detect_antos_root()?;
    let ws_canon = workspace_path
        .canonicalize()
        .unwrap_or_else(|_| workspace_path.to_path_buf());
    let root_canon = antos_root
        .canonicalize()
        .unwrap_or_else(|_| antos_root.clone());

    let ws_dir = root_canon.join("workspace");
    if ws_canon.starts_with(&ws_dir) && ws_canon != root_canon {
        Some(root_canon)
    } else {
        None
    }
}

/// Ceiling-aware variant of [`find_tickets_dir`].
///
/// `ceiling` is the exclusive upper bound: if traversal reaches this directory
/// without having found a tickets directory, the function returns `None`.
pub fn find_tickets_dir_with_ceiling(inicio: &Path, ceiling: Option<&Path>) -> Option<PathBuf> {
    let mut actual = inicio
        .canonicalize()
        .unwrap_or_else(|_| inicio.to_path_buf());
    let ceiling_canon = ceiling
        .and_then(|c| c.canonicalize().ok())
        .or_else(|| ceiling.map(|c| c.to_path_buf()));

    loop {
        // Stop if we have reached (or passed) the ceiling directory.
        if let Some(ref ceil) = ceiling_canon {
            if actual == *ceil {
                break;
            }
        }

        let candidatos = [
            actual.join("docs").join("tickets"),
            actual.join("specs"),
            actual.join(".tickets"),
            actual.join(".antos").join("tickets"),
        ];

        for c in candidatos {
            if c.is_dir() {
                return Some(c);
            }
        }

        if !actual.pop() {
            break;
        }
    }

    None
}

/// Finds an existing tickets directory within the ceiling, or creates `<inicio>/docs/tickets/`
/// with an initial project-scoped `README.md`.
pub fn find_or_create_tickets_dir_with_ceiling(
    inicio: &Path,
    ceiling: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(d) = find_tickets_dir_with_ceiling(inicio, ceiling) {
        return Ok(d);
    }
    let default_dir = inicio.join("docs").join("tickets");
    fs::create_dir_all(&default_dir)?;
    let readme = default_dir.join("README.md");
    if !readme.exists() {
        let proj_name = inicio
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Proyecto".to_string());
        let content = format!(
            "# {proj_name} · Catálogo y Hoja de Ruta de Tickets\n\n\
            Este directorio contiene las especificaciones y backlog técnico para el proyecto **{proj_name}**.\n\n\
            | Fase | ID | Título | Estado |\n\
            | :--- | :--- | :--- | :--- |\n"
        );
        let _ = fs::write(&readme, content);
    }
    Ok(default_dir)
}

/// Encuentra o crea el directorio de tickets (`docs/tickets/`).
pub fn find_or_create_tickets_dir(inicio: &Path) -> Result<PathBuf> {
    let ceiling = default_ceiling_for(inicio);
    find_or_create_tickets_dir_with_ceiling(inicio, ceiling.as_deref())
}

/// Encuentra el directorio de tickets (`docs/tickets/`, `specs/`, o `.tickets/`)
/// aplicando el techo de contención por defecto.
pub fn find_tickets_dir(inicio: &Path) -> Option<PathBuf> {
    let ceiling = default_ceiling_for(inicio);
    find_tickets_dir_with_ceiling(inicio, ceiling.as_deref())
}

/// Encuentra el directorio de tickets ascendiendo sin ningún techo de contención.
pub fn find_tickets_dir_unbounded(inicio: &Path) -> Option<PathBuf> {
    find_tickets_dir_with_ceiling(inicio, None)
}

#[deprecated]
pub fn encontrar_directorio_tickets(inicio: &Path) -> Option<PathBuf> {
    find_tickets_dir(inicio)
}

/// Indexa el directorio de tickets leyendo `README.md` (si existe) y los ficheros individuales.
fn index_tickets_directory(
    dir: &Path,
) -> Result<(Vec<TicketSummary>, HashMap<String, TicketStatus>)> {
    let mut estados_map = HashMap::new();

    // 1. Parsear README.md si existe para extraer estados consolidados de la tabla
    let readme_path = dir.join("README.md");
    if readme_path.is_file() {
        if let Ok(contenido) = fs::read_to_string(&readme_path) {
            parsear_estados_readme(&contenido, &mut estados_map);
        }
    }

    // 2. Leer archivos en el directorio
    let mut summaries = Vec::new();
    let entries =
        fs::read_dir(dir).with_context(|| format!("leyendo directorio {}", dir.display()))?;

    let mut archivos_tickets: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(nombre) = path.file_name().and_then(|n| n.to_str()) {
                if nombre.starts_with('T') && nombre.ends_with(".md") && nombre != "README.md" {
                    archivos_tickets.push(path);
                }
            }
        }
    }

    // Ordenar tickets alfabéticamente/por ID
    archivos_tickets.sort();

    for path in archivos_tickets {
        if let Ok(contenido) = fs::read_to_string(&path) {
            if let Some(summary) = parsear_summary_ticket(&path, &contenido, &estados_map) {
                estados_map.insert(summary.id.clone(), summary.status);
                summaries.push(summary);
            }
        }
    }

    Ok((summaries, estados_map))
}

/// Extrae estados de tickets desde la tabla de `docs/tickets/README.md`.
fn parsear_estados_readme(contenido: &str, destino: &mut HashMap<String, TicketStatus>) {
    for linea in contenido.lines() {
        let l = linea.trim();
        if !l.starts_with('|') || l.contains("---") || l.contains("Fase") && l.contains("Título") {
            continue;
        }

        let celdas: Vec<&str> = l.split('|').map(|s| s.trim()).collect();
        // Celdas esperadas: ["", "Fase X", "[T1.1](...)", "Título...", "Estado", ""]
        if celdas.len() >= 5 {
            let id_raw = celdas[2];
            let estado_raw = celdas[4];

            // Extraer ID de formato `[T1.1](...)` o `T1.1`
            let id = if let Some(start) = id_raw.find('[') {
                if let Some(end) = id_raw.find(']') {
                    &id_raw[start + 1..end]
                } else {
                    id_raw
                }
            } else {
                id_raw
            };

            let estado = parsear_estado_str(estado_raw);
            if !id.is_empty() {
                destino.insert(id.trim().to_uppercase(), estado);
            }
        }
    }
}

fn parsear_estado_str(texto: &str) -> TicketStatus {
    let t = texto.to_lowercase();
    if t.contains("completado") || t.contains("done") || t.contains("✅") {
        TicketStatus::Completado
    } else if t.contains("progreso") || t.contains("in progress") || t.contains("🔄") {
        TicketStatus::EnProgreso
    } else if t.contains("revisión") || t.contains("review") || t.contains("🔍") {
        TicketStatus::EnRevision
    } else {
        TicketStatus::Pendiente
    }
}

/// Parsea el resumen de un archivo de ticket individual.
fn parsear_summary_ticket(
    ruta: &Path,
    contenido: &str,
    estados_map: &HashMap<String, TicketStatus>,
) -> Option<TicketSummary> {
    let file_name = ruta.file_name()?.to_str()?;
    let id_extraido = extract_id_from_name_or_content(file_name, contenido)?;

    let mut titulo = file_name.trim_end_matches(".md").to_string();
    let fase = deducir_fase(&id_extraido);

    for linea in contenido.lines() {
        let l = linea.trim();
        if l.starts_with('#') {
            // Ejemplo: `# T1.3 · Indexador y Parser Nativo...`
            let encabezado = l.trim_start_matches('#').trim();
            if let Some((_id_part, tit_part)) = encabezado.split_once('·') {
                titulo = tit_part.trim().to_string();
            } else if let Some((_id_part, tit_part)) = encabezado.split_once('-') {
                titulo = tit_part.trim().to_string();
            } else {
                titulo = encabezado.to_string();
            }
            break;
        }
    }

    let estado = estados_map
        .get(&id_extraido.to_uppercase())
        .copied()
        .unwrap_or(TicketStatus::Pendiente);

    Some(TicketSummary {
        id: id_extraido,
        phase: fase,
        title: titulo,
        status: estado,
        file_path: ruta.display().to_string(),
    })
}

/// Parsea el detalle completo de un archivo Markdown de ticket.
pub fn parsear_archivo_ticket(
    ruta: &Path,
    estado_override: Option<TicketStatus>,
) -> Result<TicketDetail> {
    let contenido = fs::read_to_string(ruta)
        .with_context(|| format!("no se pudo leer el archivo de ticket {}", ruta.display()))?;

    let file_name = ruta
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("ticket.md");
    let id = extract_id_from_name_or_content(file_name, &contenido)
        .unwrap_or_else(|| "T0.0".to_string());

    let fase = deducir_fase(&id);
    let mut titulo = file_name.trim_end_matches(".md").to_string();
    let mut descripcion = String::new();
    let mut alcance_tecnico = Vec::new();
    let mut criterios_aceptacion = Vec::new();

    let mut seccion_actual = "";

    for linea in contenido.lines() {
        let l = linea.trim();

        if l.starts_with("# ") {
            let encabezado = l[2..].trim();
            if let Some((_, tit)) = encabezado.split_once('·') {
                titulo = tit.trim().to_string();
            } else if let Some((_, tit)) = encabezado.split_once('-') {
                titulo = tit.trim().to_string();
            } else {
                titulo = encabezado.to_string();
            }
            continue;
        }

        if l.starts_with("## ") {
            let sec_nombre = l[3..].to_lowercase();
            if sec_nombre.contains("descrip") {
                seccion_actual = "descripcion";
            } else if sec_nombre.contains("alcance") || sec_nombre.contains("técnico") {
                seccion_actual = "alcance";
            } else if sec_nombre.contains("criterio") || sec_nombre.contains("aceptación") {
                seccion_actual = "criterios";
            } else {
                seccion_actual = "";
            }
            continue;
        }

        match seccion_actual {
            "descripcion" => {
                if !l.is_empty() {
                    if !descripcion.is_empty() {
                        descripcion.push(' ');
                    }
                    descripcion.push_str(l);
                }
            }
            "alcance" => {
                if l.starts_with('*')
                    || l.starts_with('-')
                    || l.starts_with("1.")
                    || l.starts_with("2.")
                    || l.starts_with("3.")
                    || l.starts_with("4.")
                {
                    let limpio = limpiar_item_markdown(l);
                    if !limpio.is_empty() {
                        alcance_tecnico.push(limpio);
                    }
                }
            }
            "criterios" => {
                if l.starts_with('*')
                    || l.starts_with('-')
                    || l.starts_with("1.")
                    || l.starts_with("2.")
                    || l.starts_with("3.")
                {
                    let limpio = limpiar_item_markdown(l);
                    if !limpio.is_empty() {
                        criterios_aceptacion.push(limpio);
                    }
                }
            }
            _ => {}
        }
    }

    let estado = estado_override.unwrap_or(TicketStatus::Pendiente);

    Ok(TicketDetail {
        id,
        phase: fase,
        title: titulo,
        status: estado,
        file_path: ruta.display().to_string(),
        description: descripcion,
        technical_scope: alcance_tecnico,
        acceptance_criteria: criterios_aceptacion,
    })
}

fn limpiar_item_markdown(linea: &str) -> String {
    let sin_prefijo = linea
        .trim_start_matches(|c: char| {
            c.is_numeric() || c == '.' || c == '*' || c == '-' || c.is_whitespace()
        })
        .trim();
    sin_prefijo.replace("**", "").trim().to_string()
}

fn extract_id_from_name_or_content(file_name: &str, _contenido: &str) -> Option<String> {
    // Buscar patrón tipo T0.1, T1.3, T2.2 al inicio del nombre del archivo
    if file_name.starts_with('T') {
        let partes: Vec<&str> = file_name.split('-').collect();
        if !partes.is_empty() && partes[0].contains('.') {
            return Some(partes[0].to_string());
        }
    }
    None
}

fn deducir_fase(id: &str) -> String {
    // Si ID es T1.3 -> "Fase 1", T0.1 -> "Fase 0"
    if let Some(resto) = id.strip_prefix('T') {
        if let Some((fase_num, _)) = resto.split_once('.') {
            return format!("Fase {}", fase_num);
        }
    }
    "General".to_string()
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_find_tickets_dir_ant_os() {
        let cwd = std::env::current_dir().expect("cwd");
        let dir = find_tickets_dir(&cwd);
        assert!(
            dir.is_some(),
            "debe encontrar docs/tickets en el repo actual"
        );
        let path = dir.unwrap();
        assert!(path.join("README.md").exists());
    }

    #[test]
    fn test_index_and_list_tickets_ant_os() {
        let cwd = std::env::current_dir().expect("cwd");
        let engine = SpecEngine::global();
        let tickets = engine.list_tickets(&cwd).expect("listar tickets");

        assert!(
            !tickets.is_empty(),
            "debe encontrar los tickets en docs/tickets"
        );

        // Verificar que T0.1, T1.1 y T1.2 están completados y T1.3 está indexado
        let t01 = tickets
            .iter()
            .find(|t| t.id == "T0.1")
            .expect("T0.1 debe existir");
        assert_eq!(t01.status, TicketStatus::Completed);

        let t11 = tickets
            .iter()
            .find(|t| t.id == "T1.1")
            .expect("T1.1 debe existir");
        assert_eq!(t11.status, TicketStatus::Completed);

        let t12 = tickets
            .iter()
            .find(|t| t.id == "T1.2")
            .expect("T1.2 debe existir");
        assert_eq!(t12.status, TicketStatus::Completed);

        let t13 = tickets
            .iter()
            .find(|t| t.id == "T1.3")
            .expect("T1.3 debe existir");
        assert!(
            t13.title.contains("Indexador y Parser")
                || t13.title.contains("Spec-Engine")
                || t13.title.contains("tickets")
        );
    }

    #[test]
    fn test_get_ticket_detail_t13() {
        let cwd = std::env::current_dir().expect("cwd");
        let engine = SpecEngine::global();
        let detalle = engine
            .get_ticket(&cwd, "T1.3")
            .expect("obtener ticket")
            .expect("detalle T1.3");

        assert_eq!(detalle.id, "T1.3");
        assert_eq!(detalle.phase, "Fase 1");
        assert!(!detalle.description.is_empty());
        assert!(!detalle.technical_scope.is_empty());
        assert!(!detalle.acceptance_criteria.is_empty());
    }

    #[test]
    fn test_create_and_update_dynamic_ticket() {
        let ws = std::env::temp_dir().join(format!("antos-test-spec-{}", std::process::id()));
        let _ = fs::remove_dir_all(&ws);
        fs::create_dir_all(&ws).expect("create test ws");

        let engine = SpecEngine::global();

        // 1. Crear ticket en nuevo workspace
        let path = engine
            .create_ticket(
                &ws,
                "T99.1",
                "Módulo de Prueba Dinámica",
                Some("Prueba de creación dinámica"),
                Some("Fase 99"),
            )
            .expect("crear ticket");

        assert!(path.exists());
        let content = fs::read_to_string(&path).expect("read ticket");
        assert!(content.contains("# T99.1 · Módulo de Prueba Dinámica"));
        assert!(content.contains("> **Estado:** ⏳ Pendiente"));

        // 2. Listar y verificar
        let tickets = engine.list_tickets(&ws).expect("listar");
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].id, "T99.1");
        assert_eq!(tickets[0].status, TicketStatus::Pending);

        // 3. Actualizar estado
        engine
            .update_ticket_status(&ws, "T99.1", TicketStatus::Completed)
            .expect("update");
        let detalle = engine
            .get_ticket(&ws, "T99.1")
            .expect("get")
            .expect("exists");
        assert_eq!(detalle.status, TicketStatus::Completed);

        let _ = fs::remove_dir_all(&ws);
    }

    // ────────────────────────────────────────────────────── T17.4 tests ──

    /// T17.4 — A project under workspace/ without its own tickets directory must NOT
    /// inherit the antOS operating system tickets (docs/tickets/).
    #[test]
    fn test_workspace_project_does_not_inherit_system_tickets() {
        let antos_root = crate::git::detect_antos_root().expect("antos root");
        let ws_project = antos_root
            .join("workspace")
            .join(format!("test_no_tickets_{}", std::process::id()));
        let _ = fs::create_dir_all(&ws_project);

        let engine = SpecEngine::global();
        let tickets = engine.list_tickets(&ws_project).expect("list tickets");
        let _ = fs::remove_dir_all(&ws_project);

        assert!(
            tickets.is_empty(),
            "A project under workspace/ without its own tickets must NOT inherit system tickets; got {} tickets",
            tickets.len()
        );
    }

    /// T17.4 — SpecEngine can create, list, and update tickets independently within
    /// a developer project under workspace/.
    #[test]
    fn test_project_tickets_lifecycle_isolated() {
        let antos_root = crate::git::detect_antos_root().expect("antos root");
        let ws_project = antos_root
            .join("workspace")
            .join(format!("test_proj_spec_{}", std::process::id()));
        let _ = fs::remove_dir_all(&ws_project);
        fs::create_dir_all(&ws_project).expect("create proj");

        let engine = SpecEngine::global();

        // 1. Initially 0 tickets
        let tickets_init = engine.list_tickets(&ws_project).expect("list");
        assert!(tickets_init.is_empty(), "must be empty initially");

        // 2. Create ticket in project
        let ticket_file = engine
            .create_ticket(
                &ws_project,
                "T1.1",
                "Modulo de Autenticacion JWT",
                Some("Implementar JWT en el microservicio"),
                Some("Fase 1"),
            )
            .expect("create ticket");

        assert!(ticket_file.exists());
        assert!(ws_project.join("docs/tickets/README.md").exists());

        // 3. List tickets of project
        let tickets = engine.list_tickets(&ws_project).expect("list");
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].id, "T1.1");
        assert_eq!(tickets[0].title, "Modulo de Autenticacion JWT");
        assert_eq!(tickets[0].status, TicketStatus::Pending);

        // 4. Update status in project
        engine
            .update_ticket_status(&ws_project, "T1.1", TicketStatus::Completed)
            .expect("update status");

        let detail = engine
            .get_ticket(&ws_project, "T1.1")
            .expect("get ticket")
            .expect("detail");
        assert_eq!(detail.status, TicketStatus::Completed);

        let _ = fs::remove_dir_all(&ws_project);
    }
}
