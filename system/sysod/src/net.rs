//! Diagnóstico y liberación de puertos de red locales (T2.3).
//!
//! Permite a antOS inspeccionar qué procesos tienen ocupados los puertos TCP
//! de desarrollo (ej. 3000, 5173, 8080, 5432) y liberar puertos en colisión
//! mediante terminación segura de procesos huérfanos o desatendidos.

use antos_protocolo::PortDiagnosticInfo;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::process::Command;

/// Diagnostica puertos TCP locales en estado LISTEN.
pub fn diagnosticar_puertos(filtro_puerto: Option<u16>) -> Result<Vec<PortDiagnosticInfo>> {
    let mut args = vec!["-iTCP", "-sTCP:LISTEN", "-P", "-n"];
    let puerto_str;
    if let Some(p) = filtro_puerto {
        puerto_str = format!("-iTCP:{p}");
        args[0] = &puerto_str;
    }

    let out = Command::new("lsof")
        .args(&args)
        .output()
        .context("ejecutando lsof para inspeccionar puertos TCP")?;

    let mut resultados = Vec::new();
    let mut vistos = BTreeSet::new();

    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for linea in stdout.lines().skip(1) {
            let partes: Vec<&str> = linea.split_whitespace().collect();
            if partes.len() >= 9 {
                let process_name = partes[0].to_string();
                let pid: u32 = match partes[1].parse() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let name_col = partes[8]; // ej. *:3000 o 127.0.0.1:8080 o [::1]:5173
                if let Some(puerto) = extraer_puerto(name_col) {
                    if let Some(filtro) = filtro_puerto {
                        if puerto != filtro {
                            continue;
                        }
                    }

                    if vistos.insert((puerto, pid)) {
                        let command = obtener_comando_proceso(pid).unwrap_or_else(|| process_name.clone());
                        let working_dir = obtener_directorio_proceso(pid);

                        resultados.push(PortDiagnosticInfo {
                            port: puerto,
                            pid,
                            process_name,
                            command,
                            working_dir,
                        });
                    }
                }
            }
        }
    }

    resultados.sort_by_key(|p| (p.port, p.pid));
    Ok(resultados)
}

/// Libera un puerto de red terminando los procesos que lo tienen ocupado.
pub fn liberar_puerto(puerto: u16, force: bool) -> Result<Vec<PortDiagnosticInfo>> {
    let procesos = diagnosticar_puertos(Some(puerto))?;
    if procesos.is_empty() {
        return Ok(Vec::new());
    }

    for p in &procesos {
        let signal = if force { "-9" } else { "-15" };
        let out = Command::new("kill")
            .arg(signal)
            .arg(p.pid.to_string())
            .output()
            .with_context(|| format!("terminando proceso PID {} en puerto {}", p.pid, puerto))?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("no se pudo terminar el proceso {} en puerto {}: {}", p.pid, puerto, err);
        }
    }

    Ok(procesos)
}

/// Extrae el puerto numérico de un descriptor de socket tipo `*:3000` o `127.0.0.1:8080`.
fn extraer_puerto(name: &str) -> Option<u16> {
    if let Some((_, puerto_str)) = name.rsplit_once(':') {
        puerto_str.parse::<u16>().ok()
    } else {
        None
    }
}

/// Obtiene la línea de comando completa de un proceso a través de `ps`.
fn obtener_comando_proceso(pid: u32) -> Option<String> {
    let out = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;

    if out.status.success() {
        let cmd = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !cmd.is_empty() {
            return Some(cmd);
        }
    }
    None
}

/// Obtiene el directorio de trabajo de un proceso si está accesible.
fn obtener_directorio_proceso(pid: u32) -> Option<String> {
    let out = Command::new("lsof")
        .args(["-p", &pid.to_string(), "-a", "-d", "cwd", "-Fn"])
        .output()
        .ok()?;

    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for linea in stdout.lines() {
            if let Some(ruta) = linea.strip_prefix('n') {
                let r = ruta.trim();
                if !r.is_empty() {
                    return Some(r.to_string());
                }
            }
        }
    }
    None
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraer_puerto() {
        assert_eq!(extraer_puerto("*:3000"), Some(3000));
        assert_eq!(extraer_puerto("127.0.0.1:8080"), Some(8080));
        assert_eq!(extraer_puerto("[::1]:5173"), Some(5173));
        assert_eq!(extraer_puerto("0.0.0.0:5432"), Some(5432));
        assert_eq!(extraer_puerto("invalido"), None);
    }

    #[test]
    fn test_diagnosticar_puertos_no_falla() {
        // La llamada a diagnosticar_puertos no debe hacer panic
        let resultado = diagnosticar_puertos(None);
        assert!(resultado.is_ok(), "diagnosticar puertos debe retornar Ok");
    }

    #[test]
    fn test_diagnosticar_puerto_especifico_inexistente() {
        let resultado = diagnosticar_puertos(Some(59999)).expect("diagnostico");
        // Puerto 59999 improbable que esté en uso en tests
        // El test verifica que retorne Ok sin pánico
        assert!(resultado.iter().all(|p| p.port == 59999));
    }
}
