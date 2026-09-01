//! Diagnóstico y liberación de puertos de red locales (T2.3).
//!
//! Permite a antOS inspeccionar qué procesos tienen ocupados los puertos TCP
//! de desarrollo (ej. 3000, 5173, 8080, 5432) y liberar puertos en colisión
//! mediante terminación segura de procesos huérfanos o desatendidos.

use antos_protocolo::PortDiagnosticInfo;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::process::Command;

/// Diagnoses local TCP listening ports.
pub fn diagnose_ports(port_filter: Option<u16>) -> Result<Vec<PortDiagnosticInfo>> {
    let mut args = vec!["-iTCP", "-sTCP:LISTEN", "-P", "-n"];
    let port_str;
    if let Some(p) = port_filter {
        port_str = format!("-iTCP:{p}");
        args[0] = &port_str;
    }

    let out = Command::new("lsof")
        .args(&args)
        .output()
        .context("executing lsof to inspect TCP ports")?;

    let mut results = Vec::new();
    let mut seen = BTreeSet::new();

    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 9 {
                let process_name = parts[0].to_string();
                let pid: u32 = match parts[1].parse() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let name_col = parts[8]; // e.g. *:3000 or 127.0.0.1:8080 or [::1]:5173
                if let Some(port) = extract_port(name_col) {
                    if let Some(filter) = port_filter {
                        if port != filter {
                            continue;
                        }
                    }

                    if seen.insert((port, pid)) {
                        let command = get_process_command(pid).unwrap_or_else(|| process_name.clone());
                        let working_dir = get_process_cwd(pid);

                        results.push(PortDiagnosticInfo {
                            port,
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

    results.sort_by_key(|p| (p.port, p.pid));
    Ok(results)
}

/// Alias compatible.
pub fn diagnosticar_puertos(filtro_puerto: Option<u16>) -> Result<Vec<PortDiagnosticInfo>> {
    diagnose_ports(filtro_puerto)
}

/// Terminates processes holding a network port.
pub fn kill_port(port: u16, force: bool) -> Result<Vec<PortDiagnosticInfo>> {
    let processes = diagnose_ports(Some(port))?;
    if processes.is_empty() {
        return Ok(Vec::new());
    }

    for p in &processes {
        let signal = if force { "-9" } else { "-15" };
        let out = Command::new("kill")
            .arg(signal)
            .arg(p.pid.to_string())
            .output()
            .with_context(|| format!("terminating process PID {} on port {}", p.pid, port))?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("could not kill process {} on port {}: {}", p.pid, port, err);
        }
    }

    Ok(processes)
}

/// Alias compatible.
pub fn liberar_puerto(puerto: u16, force: bool) -> Result<Vec<PortDiagnosticInfo>> {
    kill_port(puerto, force)
}

/// Extracts port from socket descriptor e.g. `*:3000` or `127.0.0.1:8080`.
pub fn extract_port(name: &str) -> Option<u16> {
    if let Some((_, port_str)) = name.rsplit_once(':') {
        port_str.parse::<u16>().ok()
    } else {
        None
    }
}

/// Alias compatible.
pub fn extraer_puerto(name: &str) -> Option<u16> {
    extract_port(name)
}

/// Gets full process command line via `ps`.
fn get_process_command(pid: u32) -> Option<String> {
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

/// Gets process current working directory via `lsof`.
fn get_process_cwd(pid: u32) -> Option<String> {
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
