//! Renderiza el diff que se le enseña al usuario antes de ejecutar.

use crate::ctx::Ctx;
use crate::exec::Change;

pub enum Line {
    Info(String),
    Add(String),
    Del(String),
}

const MAX_DIFF_LINES: usize = 16;

pub fn render(ctx: &Ctx, changes: &[Change]) -> Vec<Line> {
    let mut out = Vec::new();
    // El diff de un paso se compara con lo que dejó el paso anterior, no con
    // el disco de partida. Si no, tres escrituras al mismo fichero se
    // mostrarían las tres como si partieran de cero.
    let mut pendiente = crate::exec::Pendiente::default();

    for change in changes {
        match change {
            Change::Read { path } => {
                out.push(Line::Info(format!("lee       {}", ctx.display(path))));
            }
            Change::Mkdir { path } => {
                out.push(Line::Info(format!("crea dir  {}", ctx.display(path))));
            }
            Change::Delete { path } => {
                let detail = if path.is_dir() {
                    format!(" ({} elementos)", count_entries(path))
                } else {
                    String::new()
                };
                out.push(Line::Info(format!("borra     {}{detail}", ctx.display(path))));
                out.push(Line::Del(format!("  {}", ctx.display(path))));
            }
            Change::Write { path, content } => {
                let ya_previsto = pendiente.leer(path);
                let existia = ya_previsto.is_some() || path.exists();
                let old = ya_previsto
                    .unwrap_or_else(|| std::fs::read_to_string(path).unwrap_or_default());
                let verb = if existia { "modifica" } else { "crea    " };
                out.push(Line::Info(format!("{verb}  {}", ctx.display(path))));
                for (marker, text) in diff(&old, content) {
                    match marker {
                        '-' => out.push(Line::Del(format!("  {text}"))),
                        _ => out.push(Line::Add(format!("  {text}"))),
                    }
                }
            }
        }
        pendiente.aplicar(change);
    }
    out
}

fn count_entries(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
}

/// Diff por líneas recortando prefijo y sufijo comunes.
///
/// No es un algoritmo de diff completo, pero para el caso real — reescribir
/// un fichero pequeño — muestra exactamente lo que cambia sin dependencias.
fn diff(old: &str, new: &str) -> Vec<(char, String)> {
    let o: Vec<&str> = old.lines().collect();
    let n: Vec<&str> = new.lines().collect();

    let mut start = 0;
    while start < o.len() && start < n.len() && o[start] == n[start] {
        start += 1;
    }
    let max_end = (o.len() - start).min(n.len() - start);
    let mut end = 0;
    while end < max_end && o[o.len() - 1 - end] == n[n.len() - 1 - end] {
        end += 1;
    }

    let removed = &o[start..o.len() - end];
    let added = &n[start..n.len() - end];

    let mut lines: Vec<(char, String)> = Vec::new();
    for l in removed {
        lines.push(('-', l.to_string()));
    }
    for l in added {
        lines.push(('+', l.to_string()));
    }

    if lines.len() > MAX_DIFF_LINES {
        let hidden = lines.len() - MAX_DIFF_LINES;
        lines.truncate(MAX_DIFF_LINES);
        lines.push((' ', format!("… y {hidden} línea(s) más")));
    }
    lines
}
