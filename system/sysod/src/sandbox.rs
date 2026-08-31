//! El recinto de ejecución.
//!
//! M1 comprobaba las rutas; M2 las hace cumplir. La diferencia importa: en M1
//! nada impedía *físicamente* a una capacidad salirse, solo la mirábamos. Aquí
//! quien deniega es el kernel, y una capacidad que mienta sobre sus efectos
//! declarados falla con EPERM en vez de salirse con la suya.
//!
//! La ejecución vive en un proceso aparte. Esa separación no es decorativa:
//! el confinamiento se aplica al proceso entero, así que si el broker
//! ejecutara los cambios él mismo, el recinto lo encerraría a él también — y
//! no podría escribir su propia bitácora ni sus instantáneas.

use crate::blast::Blast;
use crate::exec::Change;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Lo que el recinto permite. Se deriva del radio de impacto: exactamente lo
/// que las capacidades declararon que iban a tocar, ni un byte más.
pub struct Policy {
    pub writes: Vec<PathBuf>,
    pub network: bool,
}

impl Policy {
    pub fn from_blast(blast: &Blast) -> Self {
        Policy {
            writes: blast.paths_to_snapshot(),
            network: !blast.network.is_empty(),
        }
    }
}

pub trait Sandbox {
    fn name(&self) -> &'static str;
    /// Qué garantiza de verdad este recinto, para poder decirlo sin adornos.
    fn guarantees(&self) -> &'static str;
    fn command(&self, exe: &Path, policy: &Policy, subcommand: &str) -> Result<Command>;
}

pub fn for_host() -> Box<dyn Sandbox> {
    if cfg!(target_os = "macos") && Path::new(SANDBOX_EXEC).exists() {
        Box::new(Seatbelt)
    } else {
        Box::new(SinRecinto)
    }
}

// ------------------------------------------------------------------ macOS

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

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
/// macOS pelea con el enlazador dinámico y mata el proceso al arrancar. En el
/// destino real —Linux— confinar lecturas es trivial: en un espacio de nombres
/// de montaje simplemente no montas lo que no debe verse. Con la red denegada,
/// una lectura no declarada no puede salir a ningún sitio.
fn sbpl(policy: &Policy) -> String {
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

// -------------------------------------------------------------- sin recinto

/// Cuando la plataforma no ofrece confinamiento. No falla en silencio: quien
/// lo use tiene que enterarse de que las garantías no existen.
pub struct SinRecinto;

impl Sandbox for SinRecinto {
    fn name(&self) -> &'static str {
        "ninguno"
    }

    fn guarantees(&self) -> &'static str {
        "NINGUNA — la ejecución no está confinada en esta plataforma"
    }

    fn command(&self, exe: &Path, _policy: &Policy, subcommand: &str) -> Result<Command> {
        let mut cmd = Command::new(exe);
        cmd.arg(subcommand);
        Ok(cmd)
    }
}

// ------------------------------------------------- protocolo broker↔ejecutor

#[derive(Debug, Serialize, Deserialize)]
pub struct Orden {
    pub changes: Vec<Change>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Respuesta {
    pub ok: bool,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Lanza el ejecutor confinado y le pasa los cambios por la entrada estándar.
pub fn run(sandbox: &dyn Sandbox, changes: &[Change], policy: &Policy) -> Result<Vec<String>> {
    let exe = std::env::current_exe().context("no sé cuál es mi propio binario")?;
    let mut cmd = sandbox.command(&exe, policy, EXEC_SUBCOMMAND)?;
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().context("no pude lanzar el ejecutor confinado")?;
    let orden = serde_json::to_vec(&Orden { changes: changes.to_vec() })?;
    child
        .stdin
        .take()
        .expect("stdin canalizado")
        .write_all(&orden)?;

    let out = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);

    let resp: Respuesta = serde_json::from_str(stdout.trim()).with_context(|| {
        format!(
            "el ejecutor no devolvió una respuesta legible (código {:?})\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        )
    })?;

    if !resp.ok {
        bail!("{}", resp.error.unwrap_or_else(|| "error sin detalle".into()));
    }
    Ok(resp.outputs)
}

/// Comprueba si el recinto deja salir a la red.
pub fn probe_network(sandbox: &dyn Sandbox, policy: &Policy) -> Result<bool> {
    let exe = std::env::current_exe()?;
    let out = sandbox
        .command(&exe, policy, NET_SUBCOMMAND)?
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).trim() == "conectado")
}

// -------------------------------------------------------------- el ejecutor

pub const EXEC_SUBCOMMAND: &str = "__ejecutar";
pub const NET_SUBCOMMAND: &str = "__probar-red";

/// El otro lado: ya confinado, lee la orden y la aplica.
///
/// No conoce el catálogo, ni la política, ni la bitácora. Solo recibe una
/// lista de cambios concretos. Cuanto menos sepa, menos puede equivocarse.
pub fn execute_from_stdin() -> Result<()> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;
    let orden: Orden = serde_json::from_str(&raw)?;

    let resp = match crate::exec::apply(&orden.changes) {
        Ok(outputs) => Respuesta { ok: true, outputs, error: None },
        Err(e) => Respuesta {
            ok: false,
            outputs: Vec::new(),
            error: Some(format!("{e:#}")),
        },
    };
    println!("{}", serde_json::to_string(&resp)?);
    Ok(())
}

pub fn probe_network_from_inside() -> Result<()> {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    let reachable = "one.one.one.one:443"
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .map(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok())
        .unwrap_or(false);

    println!("{}", if reachable { "conectado" } else { "bloqueado" });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_perfil_solo_reabre_las_rutas_declaradas() {
        let policy = Policy {
            writes: vec![PathBuf::from("/ws/demo")],
            network: false,
        };
        let profile = sbpl(&policy);

        assert!(profile.contains("(deny file-write*)"), "debe denegar escrituras por defecto");
        assert!(profile.contains("(subpath \"/ws/demo\")"), "debe reabrir la ruta declarada");
        assert!(profile.contains("(deny network*)"), "sin red declarada, se deniega la red");
    }

    #[test]
    fn la_red_se_permite_solo_si_esta_declarada() {
        let policy = Policy { writes: vec![], network: true };
        assert!(!sbpl(&policy).contains("(deny network*)"));
    }

    #[test]
    fn las_comillas_en_una_ruta_no_pueden_romper_el_perfil() {
        // Una ruta con comillas cerraría el literal SBPL y el resto del
        // perfil se interpretaría como política. Hay que escaparlo.
        let policy = Policy {
            writes: vec![PathBuf::from("/ws/ma\"lo")],
            network: false,
        };
        assert!(sbpl(&policy).contains("\"/ws/ma\\\"lo\""));
    }
}
