//! Recinto de Linux: Landlock.
//!
//! Landlock es el LSM sin privilegios del kernel: un proceso declara qué
//! accesos quiere que se controlen, añade reglas para las rutas que sí puede
//! tocar, y se encierra a sí mismo. No hace falta root, ni espacios de
//! nombres, ni montar nada.
//!
//! Frente a Seatbelt tiene dos diferencias que importan:
//!
//! - **Confina lecturas.** Es lo que macOS no podía dar: aquí el espacio
//!   personal del usuario, sus llaves y sus credenciales dejan de ser
//!   legibles para una capacidad, aunque nadie se lo haya prohibido a mano.
//! - **Es más grueso con los ficheros nuevos.** Las reglas se enganchan a un
//!   descriptor, así que la ruta tiene que existir. Para crear un fichero
//!   nuevo hay que permitir escribir *dentro de su directorio*, que sí está
//!   declarado. Seatbelt podía nombrar el fichero inexistente; Landlock no.
//!
//! Los syscalls se invocan a mano en vez de con un crate: son tres, la ABI
//! está documentada y así se ve exactamente qué estructura cruza al kernel.

use super::{Policy, Sandbox};
use anyhow::{bail, Context, Result};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

const SYS_CREATE_RULESET: libc::c_long = 444;
const SYS_ADD_RULE: libc::c_long = 445;
const SYS_RESTRICT_SELF: libc::c_long = 446;

const CREATE_RULESET_VERSION: libc::c_ulong = 1;
const RULE_PATH_BENEATH: libc::c_ulong = 1;

// Accesos de fichero. Los que NO se declaran aquí quedan sin controlar.
const FS_EXECUTE: u64 = 1 << 0;
const FS_WRITE_FILE: u64 = 1 << 1;
const FS_READ_FILE: u64 = 1 << 2;
const FS_READ_DIR: u64 = 1 << 3;
const FS_REMOVE_DIR: u64 = 1 << 4;
const FS_REMOVE_FILE: u64 = 1 << 5;
const FS_MAKE_CHAR: u64 = 1 << 6;
const FS_MAKE_DIR: u64 = 1 << 7;
const FS_MAKE_REG: u64 = 1 << 8;
const FS_MAKE_SOCK: u64 = 1 << 9;
const FS_MAKE_FIFO: u64 = 1 << 10;
const FS_MAKE_BLOCK: u64 = 1 << 11;
const FS_MAKE_SYM: u64 = 1 << 12;
const FS_REFER: u64 = 1 << 13; // ABI 2
const FS_TRUNCATE: u64 = 1 << 14; // ABI 3

/// Derechos que un fichero corriente puede tener. Conceder a un fichero un
/// derecho de directorio (`READ_DIR`, `MAKE_*`, `REMOVE_*`) hace que el kernel
/// rechace la regla entera con EINVAL — y una regla rechazada tumba el
/// recinto completo, no solo esa ruta.
const FILE_ONLY: u64 = FS_EXECUTE | FS_WRITE_FILE | FS_READ_FILE | FS_TRUNCATE;

const NET_BIND_TCP: u64 = 1 << 0; // ABI 4
const NET_CONNECT_TCP: u64 = 1 << 1;

/// Rutas del sistema que el proceso necesita poder leer para existir:
/// bibliotecas, enlazador dinámico, configuración base. Todo lo que no esté
/// aquí ni declarado por una capacidad queda ilegible — incluido `/home`.
const SYSTEM_READ_ROOTS: &[&str] = &[
    "/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc", "/proc", "/dev", "/sys",
];

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
}

/// `packed` no es opcional: el kernel espera 12 bytes, no 16.
#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

pub struct Landlock;

impl Sandbox for Landlock {
    fn name(&self) -> &'static str {
        "landlock"
    }

    fn guarantees(&self) -> &'static str {
        "escrituras, lecturas y TCP confinados por el kernel"
    }

    fn confines_reads(&self) -> bool {
        true
    }

    /// Landlock engancha sus reglas a descriptores, así que las rutas tienen
    /// que existir. Los directorios declarados se crean aquí, en el broker
    /// —sin confinar— justo después de fotografiarlos: son efectos ya
    /// autorizados, y si el plan falla, deshacer se los lleva.
    fn prepare(&self, policy: &Policy) -> Result<()> {
        for dir in &policy.dirs {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("preparando el directorio declarado {}", dir.display()))?;
        }
        Ok(())
    }

    fn command(&self, exe: &Path, policy: &Policy, subcommand: &str) -> Result<Command> {
        let mut cmd = Command::new(exe);
        cmd.arg(subcommand)
            .env(super::POLICY_ENV, serde_json::to_string(policy)?);
        Ok(cmd)
    }
}

/// La versión de ABI que soporta este kernel. 0 o menos: sin Landlock.
fn abi_version() -> i64 {
    unsafe {
        libc::syscall(
            SYS_CREATE_RULESET,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            CREATE_RULESET_VERSION,
        )
    }
}

pub fn available() -> bool {
    abi_version() >= 1
}

/// Los accesos que pedimos controlar, recortados a lo que el kernel entiende.
/// Pedir un bit que su ABI no conoce hace fallar la creación del conjunto.
fn handled_fs(abi: i64) -> u64 {
    let mut fs = FS_WRITE_FILE
        | FS_READ_FILE
        | FS_READ_DIR
        | FS_REMOVE_DIR
        | FS_REMOVE_FILE
        | FS_MAKE_CHAR
        | FS_MAKE_DIR
        | FS_MAKE_REG
        | FS_MAKE_SOCK
        | FS_MAKE_FIFO
        | FS_MAKE_BLOCK
        | FS_MAKE_SYM;
    if abi >= 2 {
        fs |= FS_REFER;
    }
    if abi >= 3 {
        fs |= FS_TRUNCATE;
    }
    fs
}

/// Encierra al proceso actual. A partir de aquí no hay vuelta atrás: Landlock
/// no se puede relajar, solo apretar más.
pub fn restrict(policy: &Policy) -> Result<()> {
    let abi = abi_version();
    if abi < 1 {
        bail!("este kernel no trae Landlock");
    }

    let handled = handled_fs(abi);
    // Sin red declarada y con ABI suficiente, se controla TCP y no se añade
    // ninguna regla: queda todo denegado.
    let handled_net = if abi >= 4 && !policy.network {
        NET_BIND_TCP | NET_CONNECT_TCP
    } else {
        0
    };

    // Debe ir ANTES de encerrarse: sin esto, restrict_self exige CAP_SYS_ADMIN.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        bail!(
            "no pude fijar no_new_privs: {}",
            std::io::Error::last_os_error()
        );
    }

    let attr = RulesetAttr {
        handled_access_fs: handled,
        handled_access_net: handled_net,
    };
    let ruleset = unsafe {
        libc::syscall(
            SYS_CREATE_RULESET,
            &attr as *const RulesetAttr,
            std::mem::size_of::<RulesetAttr>(),
            0usize,
        )
    };
    if ruleset < 0 {
        bail!(
            "no pude crear el conjunto de reglas: {}",
            std::io::Error::last_os_error()
        );
    }
    let ruleset = ruleset as libc::c_int;

    // Lo declarado por las capacidades: lectura y escritura.
    for path in policy.writes.iter().chain(policy.reads.iter()) {
        add_rule(ruleset, path, handled)?;
    }
    // El sistema, solo lectura y ejecución.
    let read_only = (FS_READ_FILE | FS_READ_DIR | FS_EXECUTE) & handled;
    for root in SYSTEM_READ_ROOTS {
        add_rule(ruleset, Path::new(root), read_only)?;
    }
    // El propio binario, para poder ejecutarse desde donde esté.
    if let Ok(exe) = std::env::current_exe() {
        add_rule(ruleset, &exe, read_only)?;
    }

    let rc = unsafe { libc::syscall(SYS_RESTRICT_SELF, ruleset, 0usize) };
    unsafe { libc::close(ruleset) };
    if rc != 0 {
        bail!("no pude encerrarme: {}", std::io::Error::last_os_error());
    }
    Ok(())
}

/// Añade una regla para `path`. Que la ruta no exista no es un error: su
/// directorio declarado ya la cubre.
fn add_rule(ruleset: libc::c_int, path: &Path, allowed: u64) -> Result<()> {
    let Ok(cpath) = CString::new(path.as_os_str().as_bytes()) else {
        return Ok(());
    };
    let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if fd < 0 {
        return Ok(());
    }

    let rule = PathBeneathAttr {
        allowed_access: allowed_for(is_dir(fd), allowed),
        parent_fd: fd,
    };
    let rc = unsafe {
        libc::syscall(
            SYS_ADD_RULE,
            ruleset,
            RULE_PATH_BENEATH,
            &rule as *const PathBeneathAttr,
            0usize,
        )
    };
    unsafe { libc::close(fd) };

    if rc != 0 {
        bail!(
            "no pude añadir la regla para {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

/// Recorta los derechos a los que la ruta puede aceptar.
fn allowed_for(is_dir: bool, allowed: u64) -> u64 {
    if is_dir {
        allowed
    } else {
        allowed & FILE_ONLY
    }
}

fn is_dir(fd: libc::c_int) -> bool {
    // SAFETY: fstat solo escribe en el buffer que le damos; funciona sobre
    // descriptores O_PATH, que es lo único que abrimos aquí.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        libc::fstat(fd, &mut st) == 0 && (st.st_mode & libc::S_IFMT) == libc::S_IFDIR
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_requested_accesses_are_trimmed_to_kernel_abi() {
        // REFER llegó en la ABI 2 y TRUNCATE en la 3: pedirlos a un kernel
        // más viejo hace fallar la creación del conjunto entero.
        assert_eq!(handled_fs(1) & FS_REFER, 0);
        assert_eq!(handled_fs(1) & FS_TRUNCATE, 0);
        assert_ne!(handled_fs(2) & FS_REFER, 0);
        assert_eq!(handled_fs(2) & FS_TRUNCATE, 0);
        assert_ne!(handled_fs(3) & FS_TRUNCATE, 0);
    }

    #[test]
    fn siempre_se_controlan_lecturas_y_escrituras() {
        let fs = handled_fs(1);
        assert_ne!(fs & FS_WRITE_FILE, 0);
        assert_ne!(fs & FS_READ_FILE, 0);
        assert_ne!(fs & FS_MAKE_DIR, 0);
    }

    #[test]
    fn un_fichero_no_recibe_derechos_de_directorio() {
        let todo = handled_fs(3);
        let para_fichero = allowed_for(false, todo);

        assert_eq!(
            para_fichero & FS_READ_DIR,
            0,
            "un fichero no se puede listar"
        );
        assert_eq!(
            para_fichero & FS_MAKE_DIR,
            0,
            "no se crean directorios dentro de un fichero"
        );
        assert_ne!(para_fichero & FS_READ_FILE, 0, "pero sí se puede leer");
        assert_ne!(para_fichero & FS_WRITE_FILE, 0, "y escribir");

        assert_eq!(
            allowed_for(true, todo),
            todo,
            "un directorio los conserva todos"
        );
    }

    #[test]
    fn test_rule_struct_is_packed() {
        // Si el compilador la alinea a 16 bytes, el kernel lee basura.
        assert_eq!(std::mem::size_of::<PathBeneathAttr>(), 12);
    }
}
