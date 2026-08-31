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
//!
//! Cada plataforma se encierra a su manera: en macOS el padre envuelve al
//! hijo con `sandbox-exec`; en Linux el hijo se encierra a sí mismo con
//! Landlock leyendo la política del entorno.

#[cfg(target_os = "linux")]
pub mod landlock;
#[cfg(target_os = "macos")]
pub mod seatbelt;

use crate::blast::Blast;
use crate::exec::Change;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Por dónde viaja la política hasta un ejecutor que se encierra solo.
pub const POLICY_ENV: &str = "SYSO_RECINTO";

pub const EXEC_SUBCOMMAND: &str = "__ejecutar";
pub const NET_SUBCOMMAND: &str = "__probar-red";

/// Lo que el recinto permite. Se deriva del radio de impacto: exactamente lo
/// que las capacidades declararon que iban a tocar, ni un byte más.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub writes: Vec<PathBuf>,
    #[serde(default)]
    pub reads: Vec<PathBuf>,
    /// Cuáles de las rutas escritas son directorios, según lo declarado.
    #[serde(default)]
    pub dirs: Vec<PathBuf>,
    pub network: bool,
}

impl Policy {
    pub fn from_blast(blast: &Blast) -> Self {
        Policy {
            writes: blast.paths_to_snapshot(),
            reads: blast.reads.iter().cloned().collect(),
            dirs: blast.dirs.iter().cloned().collect(),
            network: !blast.network.is_empty(),
        }
    }
}

pub trait Sandbox {
    fn name(&self) -> &'static str;
    /// Qué garantiza de verdad este recinto, para poder decirlo sin adornos.
    fn guarantees(&self) -> &'static str;
    /// Si confina lecturas. No todas las plataformas pueden: `doctor` lo
    /// necesita para saber si una lectura permitida es un fallo o el límite
    /// conocido de este motor.
    fn confines_reads(&self) -> bool {
        false
    }
    /// Corre en el broker, sin confinar, antes de lanzar el ejecutor.
    fn prepare(&self, _policy: &Policy) -> Result<()> {
        Ok(())
    }
    fn command(&self, exe: &Path, policy: &Policy, subcommand: &str) -> Result<Command>;
}

pub fn for_host() -> Box<dyn Sandbox> {
    #[cfg(target_os = "linux")]
    if landlock::available() {
        return Box::new(landlock::Landlock);
    }

    #[cfg(target_os = "macos")]
    if Path::new(seatbelt::SANDBOX_EXEC).exists() {
        return Box::new(seatbelt::Seatbelt);
    }

    Box::new(SinRecinto)
}

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
    sandbox.prepare(policy)?;

    let exe = std::env::current_exe().context("no sé cuál es mi propio binario")?;
    let mut cmd = sandbox.command(&exe, policy, EXEC_SUBCOMMAND)?;
    sin_secretos(&mut cmd);
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
    let mut cmd = sandbox.command(&exe, policy, NET_SUBCOMMAND)?;
    sin_secretos(&mut cmd);
    let out = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).trim() == "conectado")
}

/// Quita del entorno del hijo lo que no le hace falta para su trabajo.
///
/// El ejecutor confinado aplica cambios en ficheros: no tiene ningún motivo
/// para llevar encima una credencial. Y el recinto le prohíbe salir a la red,
/// pero una credencial filtrada no necesita red para hacer daño — basta con
/// que acabe escrita en algún sitio.
pub fn sin_secretos(cmd: &mut Command) {
    for variable in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY_FILE"] {
        cmd.env_remove(variable);
    }
}

// -------------------------------------------------------------- el ejecutor

/// Se encierra a sí mismo si la plataforma lo hace desde dentro.
///
/// En macOS no hace nada: `sandbox-exec` ya aplicó el perfil antes de que
/// este proceso existiera.
fn self_restrict() -> Result<()> {
    let Ok(raw) = std::env::var(POLICY_ENV) else {
        return Ok(());
    };
    let policy: Policy = serde_json::from_str(&raw).context("política de recinto ilegible")?;

    #[cfg(target_os = "linux")]
    landlock::restrict(&policy)?;

    #[cfg(not(target_os = "linux"))]
    let _ = policy;

    Ok(())
}

/// El otro lado: ya confinado, lee la orden y la aplica.
///
/// No conoce el catálogo, ni la política de permisos, ni la bitácora. Solo
/// recibe una lista de cambios concretos. Cuanto menos sepa, menos puede
/// equivocarse.
pub fn execute_from_stdin() -> Result<()> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;
    let orden: Orden = serde_json::from_str(&raw)?;

    // Encerrarse ANTES de tocar nada. Leer stdin es seguro: el descriptor ya
    // estaba abierto y su contenido lo escribió el broker, no una capacidad.
    self_restrict()?;

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

    self_restrict()?;

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

}
