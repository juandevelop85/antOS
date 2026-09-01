//! Motor de especificaciones y parser nativo de tickets Markdown para antOS (T1.3).
//!
//! Este módulo indexa y parsea en tiempo real los tickets ubicados en
//! `docs/tickets/` o `specs/`, extrayendo su ID, fase, estado, descripción,
//! alcance técnico y criterios de aceptación para el CLI (`antos tickets`),
//! el demonio IPC y el centro de control de agentes.

use antos_protocolo::{TicketDetail, TicketStatus, TicketSummary};
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
        let (tickets, estados_map) = indexar_directorio_tickets(&dir)?;

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

    /// Alias compatible.
    pub fn listar_tickets(&self, workspace_path: &Path) -> Result<Vec<TicketSummary>> {
        self.list_tickets(workspace_path)
    }

    /// Obtiene el detalle completo de un ticket específico.
    pub fn get_ticket(&self, workspace_path: &Path, ticket_id: &str) -> Result<Option<TicketDetail>> {
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
                if let Some(t) = entry.tickets.iter().find(|t| t.id.to_uppercase() == id_normalizado) {
                    ruta_archivo = Some(PathBuf::from(&t.ruta_archivo));
                    estado_ticket = t.estado;
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
                entry.detalles.insert(id_normalizado, (file_mtime, detalle.clone()));
            }
        }

        Ok(Some(detalle))
    }

    /// Alias compatible.
    pub fn obtener_ticket(&self, workspace_path: &Path, ticket_id: &str) -> Result<Option<TicketDetail>> {
        self.get_ticket(workspace_path, ticket_id)
    }
}

/// Encuentra el directorio de tickets (`docs/tickets/`, `specs/`, o `.tickets/`).
pub fn find_tickets_dir(inicio: &Path) -> Option<PathBuf> {
    let mut actual = inicio.canonicalize().unwrap_or_else(|_| inicio.to_path_buf());

    loop {
        let candidatos = [
            actual.join("docs").join("tickets"),
            actual.join("specs"),
            actual.join(".tickets"),
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

/// Alias compatible.
pub fn encontrar_directorio_tickets(inicio: &Path) -> Option<PathBuf> {
    find_tickets_dir(inicio)
}

/// Indexa el directorio de tickets leyendo `README.md` (si existe) y los ficheros individuales.
fn indexar_directorio_tickets(dir: &Path) -> Result<(Vec<TicketSummary>, HashMap<String, TicketStatus>)> {
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
    let entries = fs::read_dir(dir).with_context(|| format!("leyendo directorio {}", dir.display()))?;

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
                estados_map.insert(summary.id.clone(), summary.estado);
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
    let id_extraido = extraer_id_de_nombre_o_contenido(file_name, contenido)?;

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
        fase,
        titulo,
        estado,
        ruta_archivo: ruta.display().to_string(),
    })
}

/// Parsea el detalle completo de un archivo Markdown de ticket.
pub fn parsear_archivo_ticket(ruta: &Path, estado_override: Option<TicketStatus>) -> Result<TicketDetail> {
    let contenido = fs::read_to_string(ruta)
        .with_context(|| format!("no se pudo leer el archivo de ticket {}", ruta.display()))?;

    let file_name = ruta.file_name().and_then(|n| n.to_str()).unwrap_or("ticket.md");
    let id = extraer_id_de_nombre_o_contenido(file_name, &contenido)
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
                if l.starts_with('*') || l.starts_with('-') || l.starts_with("1.") || l.starts_with("2.") || l.starts_with("3.") || l.starts_with("4.") {
                    let limpio = limpiar_item_markdown(l);
                    if !limpio.is_empty() {
                        alcance_tecnico.push(limpio);
                    }
                }
            }
            "criterios" => {
                if l.starts_with('*') || l.starts_with('-') || l.starts_with("1.") || l.starts_with("2.") || l.starts_with("3.") {
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
        fase,
        titulo,
        estado,
        ruta_archivo: ruta.display().to_string(),
        descripcion,
        alcance_tecnico,
        criterios_aceptacion,
    })
}

fn limpiar_item_markdown(linea: &str) -> String {
    let sin_prefijo = linea.trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '*' || c == '-' || c.is_whitespace()).trim();
    sin_prefijo.replace("**", "").trim().to_string()
}

fn extraer_id_de_nombre_o_contenido(file_name: &str, _contenido: &str) -> Option<String> {
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
    use super::*;

    #[test]
    fn test_encontrar_directorio_tickets_ant_os() {
        let cwd = std::env::current_dir().expect("cwd");
        let dir = encontrar_directorio_tickets(&cwd);
        assert!(dir.is_some(), "debe encontrar docs/tickets en el repo actual");
        let path = dir.unwrap();
        assert!(path.join("README.md").exists());
    }

    #[test]
    fn test_indexar_y_listar_tickets_ant_os() {
        let cwd = std::env::current_dir().expect("cwd");
        let engine = SpecEngine::global();
        let tickets = engine.listar_tickets(&cwd).expect("listar tickets");

        assert!(!tickets.is_empty(), "debe encontrar los tickets en docs/tickets");
        
        // Verificar que T0.1, T1.1 y T1.2 están completados y T1.3 está indexado
        let t01 = tickets.iter().find(|t| t.id == "T0.1").expect("T0.1 debe existir");
        assert_eq!(t01.estado, TicketStatus::Completado);

        let t11 = tickets.iter().find(|t| t.id == "T1.1").expect("T1.1 debe existir");
        assert_eq!(t11.estado, TicketStatus::Completado);

        let t12 = tickets.iter().find(|t| t.id == "T1.2").expect("T1.2 debe existir");
        assert_eq!(t12.estado, TicketStatus::Completado);

        let t13 = tickets.iter().find(|t| t.id == "T1.3").expect("T1.3 debe existir");
        assert!(t13.titulo.contains("Indexador y Parser") || t13.titulo.contains("Spec-Engine") || t13.titulo.contains("tickets"));
    }

    #[test]
    fn test_obtener_detalle_ticket_t13() {
        let cwd = std::env::current_dir().expect("cwd");
        let engine = SpecEngine::global();
        let detalle = engine.obtener_ticket(&cwd, "T1.3").expect("obtener ticket").expect("detalle T1.3");

        assert_eq!(detalle.id, "T1.3");
        assert_eq!(detalle.fase, "Fase 1");
        assert!(!detalle.descripcion.is_empty());
        assert!(!detalle.alcance_tecnico.is_empty());
        assert!(!detalle.criterios_aceptacion.is_empty());
    }
}
