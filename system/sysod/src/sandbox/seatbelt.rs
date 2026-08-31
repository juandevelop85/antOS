//! Recinto de macOS: Seatbelt, vía `sandbox-exec`.
//!
//! El perfil lo aplica el padre al lanzar el proceso, así que el ejecutor no
//! tiene que hacer nada al arrancar. Es lo contrario de Landlock, donde el
//! proceso se encierra a sí mismo.

use super::{Policy, Sandbox};
use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

pub struct Seatbelt;

impl Sandbox for Seatbelt {
    fn name(&self) -> &'static str {
        "seatbelt"
    }

    fn guarantees(&self) -> &'static str {
        "escrituras y red confinadas por el kernel; las lecturas no"
    }

    fn command(&self, exe: &Path, policy: &Policy, subcommand: &str) -> Result<Command> {
        let mut cmd = Command::new(SANDBOX_EXEC);
        cmd.arg("-p").arg(sbpl(policy)).arg(exe).arg(subcommand);
        Ok(cmd)
    }
}

/// Traduce los efectos declarados a una política del kernel (SBPL).
///
/// Se deniegan escrituras por defecto y se reabren SOLO las rutas declaradas.
/// Las lecturas quedan permitidas a propósito: un perfil `(deny default)` en
/// macOS pelea con el enlazador dinámico y mata el proceso al arrancar. En
/// Linux, Landlock sí las confina.
pub fn sbpl(policy: &Policy) -> String {
    let mut p = String::from("(version 1)\n(allow default)\n(deny file-write*)\n");

    if !policy.writes.is_empty() {
        p.push_str("(allow file-write*");
        for w in &policy.writes {
            p.push_str(&format!("\n  (subpath {})", quote(w)));
        }
        p.push_str(")\n");
    }

    if !policy.network {
        p.push_str("(deny network*)\n");
    }
    p
}

/// Los literales de SBPL son cadenas: hay que escapar lo que las rompería.
fn quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn el_perfil_solo_reabre_las_rutas_declaradas() {
        let policy = Policy {
            writes: vec![PathBuf::from("/ws/demo")],
            reads: vec![],
            dirs: vec![PathBuf::from("/ws/demo")],
            network: false,
        };
        let profile = sbpl(&policy);

        assert!(profile.contains("(deny file-write*)"), "debe denegar escrituras por defecto");
        assert!(profile.contains("(subpath \"/ws/demo\")"), "debe reabrir la ruta declarada");
        assert!(profile.contains("(deny network*)"), "sin red declarada, se deniega la red");
    }

    #[test]
    fn la_red_se_permite_solo_si_esta_declarada() {
        let policy = Policy { writes: vec![], reads: vec![], dirs: vec![], network: true };
        assert!(!sbpl(&policy).contains("(deny network*)"));
    }

    #[test]
    fn las_comillas_en_una_ruta_no_pueden_romper_el_perfil() {
        // Una ruta con comillas cerraría el literal SBPL y el resto del
        // perfil se interpretaría como política. Hay que escaparlo.
        let policy = Policy {
            writes: vec![PathBuf::from("/ws/ma\"lo")],
            reads: vec![],
            dirs: vec![],
            network: false,
        };
        assert!(sbpl(&policy).contains("\"/ws/ma\\\"lo\""));
    }
}
