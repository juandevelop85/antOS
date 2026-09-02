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
        "escrituras, red y lectura de secretos (.env/ssh) blindadas por el kernel"
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
/// Además, se deniega la lectura de archivos sensibles (.env*, id_rsa*, credenciales)
/// a menos que una concesión activa esté registrada en `policy.allowed_secrets` (T5.2).
pub fn sbpl(policy: &Policy) -> String {
    let mut p = String::from("(version 1)\n(allow default)\n(deny file-write*)\n");

    // Blindaje de secretos y claves (T5.2 Zero Environmental Authority)
    p.push_str(";; Shield sensitive credentials and secrets by default (T5.2)\n");
    p.push_str("(deny file-read*\n");
    p.push_str("  (regex #\"/\\.env($|\\..*)\")\n");
    p.push_str("  (regex #\"/\\.ssh/id_.*\")\n");
    p.push_str("  (regex #\"/\\.aws/credentials\")\n");
    p.push_str("  (regex #\"/\\.config/gcloud/\"))\n");

    // Reabrir secretos concedidos explícitamente
    if !policy.allowed_secrets.is_empty() {
        p.push_str("(allow file-read*");
        for s in &policy.allowed_secrets {
            p.push_str(&format!("\n  (subpath {})", quote(s)));
        }
        p.push_str(")\n");
    }

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
            allowed_secrets: vec![],
            quota: None,
        };
        let profile = sbpl(&policy);

        assert!(profile.contains("(deny file-write*)"), "debe denegar escrituras por defecto");
        assert!(profile.contains("(subpath \"/ws/demo\")"), "debe reabrir la ruta declarada");
        assert!(profile.contains("(deny network*)"), "sin red declarada, se deniega la red");
        assert!(profile.contains("Shield sensitive credentials"), "debe blindar secretos");
    }

    #[test]
    fn el_perfil_reabre_secretos_cuando_estan_concedidos() {
        let policy = Policy {
            writes: vec![],
            reads: vec![],
            dirs: vec![],
            network: false,
            allowed_secrets: vec![PathBuf::from("/ws/.env")],
            quota: None,
        };
        let profile = sbpl(&policy);

        assert!(profile.contains("(allow file-read*\n  (subpath \"/ws/.env\"))"));
    }

    #[test]
    fn la_red_se_permite_solo_si_esta_declarada() {
        let policy = Policy { writes: vec![], reads: vec![], dirs: vec![], network: true, allowed_secrets: vec![], quota: None };
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
            allowed_secrets: vec![],
            quota: None,
        };
        assert!(sbpl(&policy).contains("\"/ws/ma\\\"lo\""));
    }
}
