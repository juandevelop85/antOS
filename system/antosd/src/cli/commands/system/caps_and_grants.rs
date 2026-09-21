//! `antos caps`, `antos doctor`, `antos runtime-info`, `antos log`, `antos grant` y `antos revoke`.
#![allow(unused_imports, dead_code)]

extern crate antos_protocol;

use crate::capability::{Catalog, Tier};
use crate::cli::args::Opts;
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{Outcome, Record};
use crate::planner::{
    claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner,
    openai_compat::OpenAiCompatPlanner, Planner,
};
use crate::terminal::{ellipsis, paint, tier_color, BLUE, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub fn cmd_caps(catalog: &Catalog, ctx: &Ctx) -> Result<()> {
    let grants = Grants::load(&ctx.grants_path())?;
    println!();
    println!("{}", paint("capacidades", BOLD));
    for cap in catalog.caps.values() {
        let mut nivel = cap.policy.tier.label().to_string();
        if cap.policy.tier == Tier::Grant {
            nivel.push_str(if grants.is_granted(&cap.name) {
                " · concedida"
            } else {
                " · denegada"
            });
        }
        println!();
        println!(
            "  {}  {}",
            paint(&cap.name, BOLD),
            paint(&nivel, tier_color(cap.policy.tier))
        );
        println!("    {}", paint(&cap.summary, DIM));
        let params = cap
            .params
            .iter()
            .map(|(n, s)| {
                if s.optional || s.default.is_some() {
                    format!("[{n}]")
                } else {
                    n.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        if !params.is_empty() {
            println!("    {}", paint(&format!("parámetros: {params}"), DIM));
        }
    }
    println!();
    Ok(())
}

/// Comprueba que el recinto es real, atacándolo.
///
/// No basta con generar una política y confiar: `doctor` intenta de verdad
/// escribir fuera de lo declarado y salir a la red, y solo da por buena la
/// garantía si el kernel lo impide.
pub fn cmd_doctor(ctx: &Ctx, args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "--desktop" || a == "desktop") {
        return cmd_doctor_desktop(ctx);
    }
    let rt = crate::runtime::detect();
    let jail = rt.sandbox_provider();
    let info = rt.runtime_info(ctx);

    println!();
    println!("{}", paint("antOS runtime diagnostics", BOLD));
    println!("  platform  {}", info.platform_name);
    println!("  kernel    {}", info.kernel_version);
    println!("  arch      {}", info.arch);
    println!();

    // ─── capabilities matrix ───────────────────────────────────────────
    println!("{}", paint("capabilities matrix", BOLD));
    print_capability("Landlock LSM", info.capabilities.landlock_lsm);
    print_capability("Seatbelt", info.capabilities.seatbelt);
    print_capability("Cgroups v2", info.capabilities.cgroups_v2);
    print_capability("eBPF Supervision", info.capabilities.ebpf_supervision);
    print_capability("KVM / Hypervisor", info.capabilities.kvm_hypervisor);
    print_capability("Wayland Desktop", info.capabilities.wayland_desktop);
    println!();

    // ─── sandbox attack tests ──────────────────────────────────────────
    println!("{}", paint("sandbox verification", BOLD));
    println!("  engine     {}", jail.name());
    println!("  guarantees {}", paint(jail.guarantees(), DIM));
    println!();

    let mut fallos = 0;

    // Policy: only workspace is declared, no network.
    let solo_workspace = crate::sandbox::Policy {
        writes: vec![ctx.workspace.clone()],
        reads: vec![],
        dirs: vec![ctx.workspace.clone()],
        network: false,
        allowed_secrets: vec![],
        quota: None,
    };

    // 1) write outside declared paths
    let fuga = ctx.state.join("doctor-fuga.txt");
    let _ = std::fs::remove_file(&fuga);
    let intento = crate::sandbox::run(
        &*jail,
        &[crate::exec::Change::Write {
            path: fuga.clone(),
            content: "esto no debería existir".into(),
        }],
        &solo_workspace,
    );
    let quedo_escrito = fuga.exists();
    let _ = std::fs::remove_file(&fuga);

    if intento.is_err() && !quedo_escrito {
        marca(
            true,
            "escritura fuera de lo declarado: la deniega el kernel",
        );
    } else {
        fallos += 1;
        marca(false, "escritura fuera de lo declarado: SE COMPLETÓ");
    }

    // 2) write inside declared paths must work
    let dentro = ctx.workspace.join(".doctor-prueba");
    let _ = std::fs::remove_file(&dentro);
    let permitido = crate::sandbox::run(
        &*jail,
        &[crate::exec::Change::Write {
            path: dentro.clone(),
            content: "ok".into(),
        }],
        &solo_workspace,
    );
    let creado = dentro.exists();
    let _ = std::fs::remove_file(&dentro);
    if permitido.is_ok() && creado {
        marca(true, "escritura dentro de lo declarado: permitida");
    } else {
        fallos += 1;
        marca(
            false,
            "escritura dentro de lo declarado: BLOQUEADA (el recinto es demasiado estrecho)",
        );
    }

    // 3) read outside declared paths
    let secreto = ctx.state.join("doctor-secreto.txt");
    std::fs::write(&secreto, "credencial de mentira")?;
    let lectura = crate::sandbox::run(
        &*jail,
        &[crate::exec::Change::Read {
            path: secreto.clone(),
        }],
        &solo_workspace,
    );
    let _ = std::fs::remove_file(&secreto);

    match (jail.confines_reads(), lectura.is_err()) {
        (true, true) => marca(true, "lectura fuera de lo declarado: la deniega el kernel"),
        (true, false) => {
            fallos += 1;
            marca(false, "lectura fuera de lo declarado: SE COMPLETÓ");
        }
        (false, _) => println!(
            "  {} {}",
            paint("·", YELLOW),
            paint(
                "lectura fuera de lo declarado: este motor no confina lecturas",
                DIM
            )
        ),
    }

    // 4) network test
    let con_red = crate::sandbox::Policy {
        writes: vec![],
        reads: vec![],
        dirs: vec![],
        network: true,
        allowed_secrets: vec![],
        quota: None,
    };
    let alcanzable_declarando = crate::sandbox::probe_network(&*jail, &con_red).unwrap_or(false);
    let alcanzable_sin_declarar =
        crate::sandbox::probe_network(&*jail, &solo_workspace).unwrap_or(false);

    match (alcanzable_declarando, alcanzable_sin_declarar) {
        (true, false) => marca(true, "red: alcanzable al declararla, bloqueada si no"),
        (true, true) => {
            fallos += 1;
            marca(false, "red: alcanzable SIN declararla");
        }
        (false, _) => println!(
            "  {} {}",
            paint("?", YELLOW),
            paint(
                "red: no concluyente — esta máquina no llega a internet",
                DIM
            )
        ),
    }

    // ─── subsystem probes ──────────────────────────────────────────────
    println!();
    println!("{}", paint("subsystem probes", BOLD));

    // Quota monitor
    let qm = rt.quota_monitor();
    let _qm_ok = qm.fidelity().is_enforcing();
    println!(
        "  {} quota monitor: {} ({})",
        fidelity_icon(qm.fidelity()),
        qm.name(),
        paint(qm.fidelity().label(), fidelity_color(qm.fidelity()))
    );

    // Telemetry provider
    let tp = rt.telemetry_provider();
    println!(
        "  {} telemetry: {} ({})",
        fidelity_icon(tp.fidelity()),
        tp.name(),
        paint(tp.fidelity().label(), fidelity_color(tp.fidelity()))
    );

    // Ollama / LLM
    let llm_icon = if info.ollama_available {
        paint("✓", GREEN)
    } else {
        paint("✗", YELLOW)
    };
    println!(
        "  {} ollama: {}",
        llm_icon,
        if info.ollama_available {
            "reachable"
        } else {
            "not detected"
        }
    );
    if let Some(ref ep) = info.local_llm_endpoint {
        println!("    endpoint: {}", paint(ep, DIM));
    }

    println!();
    if fallos == 0 {
        println!("{}", paint("✓ all diagnostics passed", GREEN));
        Ok(())
    } else {
        bail!("{fallos} diagnostic check(s) failed")
    }
}

/// `antos runtime info` — prints the full runtime diagnostic report.
/// Una comprobación de `antos doctor --desktop`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopCheck {
    pub name: String,
    /// `None` = no aplicable en esta máquina (se muestra, no cuenta).
    pub ok: Option<bool>,
    pub detail: String,
    /// Si falla, ¿hace fallar al doctor? (Ollama y flathub solo avisan.)
    pub critical: bool,
}

/// `uid` del dueño de `path`, si existe.
fn owner_uid(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| m.uid())
}

/// `uid` efectivo del proceso.
fn current_uid() -> u32 {
    // SAFETY: `geteuid` no tiene precondiciones ni efectos.
    unsafe { libc::geteuid() }
}

/// ¿Hay un proceso cuyo ejecutable se llame `name`? Solo Linux (`/proc`);
/// en otros sistemas devuelve `None`.
fn process_running(name: &str) -> Option<bool> {
    let entries = std::fs::read_dir("/proc").ok()?;
    if !cfg!(target_os = "linux") {
        return None;
    }
    for entry in entries.flatten() {
        let Ok(raw) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let first = raw.split(|b| *b == 0).next().unwrap_or(&[]);
        let first = String::from_utf8_lossy(first);
        let base = first.rsplit('/').next().unwrap_or("");
        // En NixOS los binarios van envueltos: `.antos-barra-wrapped`.
        if base == name || base == format!(".{name}-wrapped") {
            return Some(true);
        }
    }
    Some(false)
}

/// Las comprobaciones del escritorio (T36.5): el recinto es del usuario
/// actual (antOS Linux es monousuario), hay sesión Wayland, el demonio
/// responde por el socket, la barra vive, y lo que `antos setup` deja
/// (flathub, identidad git) está. Ollama solo avisa.
pub fn desktop_checks(ctx: &Ctx) -> Vec<DesktopCheck> {
    let mut checks = Vec::new();

    // 1. Dueño del recinto.
    let me = current_uid();
    match owner_uid(&ctx.state) {
        Some(uid) if uid == me => checks.push(DesktopCheck {
            name: "recinto".into(),
            ok: Some(true),
            detail: format!("{} es de este usuario (uid {me})", ctx.state.display()),
            critical: true,
        }),
        Some(uid) => checks.push(DesktopCheck {
            name: "recinto".into(),
            ok: Some(false),
            detail: format!(
                "{} pertenece al uid {uid}, no a este usuario ({me}): antOS Linux es monousuario en esta versión; el escritorio y el demonio son del usuario dueño del recinto",
                ctx.state.display()
            ),
            critical: true,
        }),
        None => checks.push(DesktopCheck {
            name: "recinto".into(),
            ok: Some(false),
            detail: format!("{} no existe", ctx.state.display()),
            critical: true,
        }),
    }

    // 2. Sesión Wayland.
    let wayland = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|v| !v.is_empty());
    checks.push(DesktopCheck {
        name: "sesión Wayland".into(),
        ok: Some(wayland.is_some()),
        detail: wayland
            .map(|d| format!("WAYLAND_DISPLAY={d}"))
            .unwrap_or_else(|| "sin WAYLAND_DISPLAY: no estás en la sesión gráfica".into()),
        critical: true,
    });

    // 3. El demonio por el socket.
    let socket = crate::ipc::socket_path(ctx);
    checks.push(match crate::ipc::ping(ctx) {
        Ok(event) => DesktopCheck {
            name: "demonio".into(),
            ok: Some(true),
            detail: format!("{} responde (QueryGitStatus → {event})", socket.display()),
            critical: true,
        },
        Err(e) => DesktopCheck {
            name: "demonio".into(),
            ok: Some(false),
            detail: format!("{e:#}"),
            critical: true,
        },
    });

    // 4. La barra.
    checks.push(match process_running("antos-barra") {
        Some(true) => DesktopCheck {
            name: "antos-barra".into(),
            ok: Some(true),
            detail: "en ejecución".into(),
            critical: true,
        },
        Some(false) => DesktopCheck {
            name: "antos-barra".into(),
            ok: Some(false),
            detail: "no hay ningún proceso antos-barra".into(),
            critical: true,
        },
        None => DesktopCheck {
            name: "antos-barra".into(),
            ok: None,
            detail: "no comprobable fuera de Linux".into(),
            critical: false,
        },
    });

    // 5. Ollama (solo avisa: puede estar apagado a propósito).
    let ollama = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], 11434)),
        std::time::Duration::from_millis(300),
    )
    .is_ok();
    checks.push(DesktopCheck {
        name: "ollama".into(),
        ok: Some(ollama),
        detail: if ollama {
            "127.0.0.1:11434 responde".into()
        } else {
            "127.0.0.1:11434 no responde (services.antos.llm o `antos service up ollama`)".into()
        },
        critical: false,
    });

    // 6. Flathub.
    let flatpak = std::process::Command::new("flatpak")
        .args(["remotes", "--user", "--columns=name"])
        .output();
    checks.push(match flatpak {
        Ok(out) => {
            let present = String::from_utf8_lossy(&out.stdout)
                .lines()
                .any(|l| l.trim() == "flathub");
            DesktopCheck {
                name: "flathub".into(),
                ok: Some(present),
                detail: if present {
                    "remoto flathub del usuario presente".into()
                } else {
                    "sin remoto flathub (`antos setup` o el servicio antos-flathub)".into()
                },
                critical: false,
            }
        }
        Err(_) => DesktopCheck {
            name: "flathub".into(),
            ok: None,
            detail: "flatpak no está en PATH".into(),
            critical: false,
        },
    });

    // 7. Configuración de Labwc editada por el usuario con versión nueva
    //    pendiente (la sesión no la pisa; se avisa, no se aplica).
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if let Some(h) = home.as_deref() {
        for name in ["rc.xml", "autostart"] {
            let (Ok(current), Ok(orig), Ok(system)) = (
                std::fs::read(h.join(".config/labwc").join(name)),
                std::fs::read(h.join(".config/labwc").join(format!(".antos-orig-{name}"))),
                std::fs::read(format!("/etc/antos/desktop/{name}")),
            ) else {
                continue;
            };
            if current != orig && system != orig {
                checks.push(DesktopCheck {
                    name: format!("labwc/{name}"),
                    ok: Some(false),
                    detail: format!(
                        "editado por ti y hay una versión nueva en /etc/antos/desktop/{name}; se respeta la tuya (compara y fusiona a mano)"
                    ),
                    critical: false,
                });
            }
        }
    }

    // 8. Identidad git.
    let git = home
        .as_deref()
        .map(super::git_identity_present)
        .unwrap_or(false);
    checks.push(DesktopCheck {
        name: "identidad git".into(),
        ok: Some(git),
        detail: if git {
            "user.name y user.email configurados".into()
        } else {
            "sin user.name/user.email (`antos setup`)".into()
        },
        critical: false,
    });

    checks
}

/// `antos doctor --desktop` (T36.5): imprime [`desktop_checks`] y falla si
/// alguna comprobación crítica no pasa.
pub fn cmd_doctor_desktop(ctx: &Ctx) -> Result<()> {
    println!();
    println!("{}", paint("antOS · doctor --desktop", BOLD));
    let checks = desktop_checks(ctx);
    let mut failures = 0;
    for c in &checks {
        let (symbol, color) = match c.ok {
            Some(true) => ("✓", GREEN),
            Some(false) if c.critical => {
                failures += 1;
                ("✗", RED)
            }
            Some(false) => ("!", YELLOW),
            None => ("?", DIM),
        };
        println!(
            "  {} {:<16} {}",
            paint(symbol, color),
            c.name,
            paint(&c.detail, DIM)
        );
    }
    println!();
    if failures == 0 {
        println!("{}", paint("✓ el escritorio antOS está vivo", GREEN));
        Ok(())
    } else {
        bail!("{failures} comprobación(es) crítica(s) del escritorio fallaron")
    }
}

/// `antos ping`: ¿hay un demonio vivo que hable el protocolo de la barra?
/// Sale con 0 y el evento recibido, o con error si no hay socket, no
/// responde o responde algo que la barra no entendería (T36.2).
pub fn cmd_ping(ctx: &Ctx) -> Result<()> {
    let socket = crate::ipc::socket_path(ctx);
    let event = crate::ipc::ping(ctx)?;
    println!(
        "{} demonio vivo en {} · QueryGitStatus → {}",
        paint("✓", GREEN),
        socket.display(),
        paint(&event, BOLD)
    );
    Ok(())
}

pub fn cmd_runtime_info(ctx: &Ctx, args: &[String]) -> Result<()> {
    let rt = crate::runtime::detect();
    let info = rt.runtime_info(ctx);

    let json_mode = args.iter().any(|a| a == "--json");

    if json_mode {
        println!("{}", serde_json::to_string_pretty(&info)?);
        return Ok(());
    }

    println!();
    println!("{}", paint("antOS Runtime Info", BOLD));
    println!();
    println!("  Platform        {}", paint(&info.platform_name, CYAN));
    println!("  Kernel          {}", info.kernel_version);
    println!("  Architecture    {}", info.arch);
    println!();

    println!("{}", paint("Capabilities Matrix", BOLD));
    println!();
    println!(
        "  {:<22} {:<14} {}",
        paint("Subsystem", BOLD),
        paint("Fidelity", BOLD),
        paint("Enforcing", BOLD),
    );
    println!("  {}", "─".repeat(52));

    let rows: Vec<(&str, antos_protocol::CapabilityFidelity)> = vec![
        ("Landlock LSM", info.capabilities.landlock_lsm),
        ("Seatbelt", info.capabilities.seatbelt),
        ("Cgroups v2", info.capabilities.cgroups_v2),
        ("eBPF Supervision", info.capabilities.ebpf_supervision),
        ("KVM / Hypervisor", info.capabilities.kvm_hypervisor),
        ("Wayland Desktop", info.capabilities.wayland_desktop),
    ];

    for (name, fidelity) in &rows {
        let enforcing = if fidelity.is_enforcing() {
            paint("yes", GREEN)
        } else {
            paint("no", DIM)
        };
        println!(
            "  {:<22} {:<14} {}",
            name,
            paint(fidelity.label(), fidelity_color(*fidelity)),
            enforcing,
        );
    }

    println!();
    println!("{}", paint("Local LLM", BOLD));
    println!(
        "  Ollama            {}",
        if info.ollama_available {
            paint("✓ reachable", GREEN)
        } else {
            paint("✗ not detected", YELLOW)
        }
    );
    if let Some(ref ep) = info.local_llm_endpoint {
        println!("  Endpoint          {}", ep);
    }

    println!();
    Ok(())
}

fn marca(ok: bool, texto: &str) {
    let (simbolo, color) = if ok { ("✓", GREEN) } else { ("✗", RED) };
    println!("  {} {texto}", paint(simbolo, color));
}

fn print_capability(name: &str, fidelity: antos_protocol::CapabilityFidelity) {
    println!(
        "  {} {:<22} {}",
        fidelity_icon(fidelity),
        name,
        paint(fidelity.label(), fidelity_color(fidelity)),
    );
}

fn fidelity_icon(f: antos_protocol::CapabilityFidelity) -> String {
    match f {
        antos_protocol::CapabilityFidelity::Native => paint("✓", GREEN),
        antos_protocol::CapabilityFidelity::Emulated => paint("⚠", YELLOW),
        antos_protocol::CapabilityFidelity::Simulated => paint("~", YELLOW),
        antos_protocol::CapabilityFidelity::Unsupported => paint("✗", DIM),
    }
}

fn fidelity_color(f: antos_protocol::CapabilityFidelity) -> &'static str {
    match f {
        antos_protocol::CapabilityFidelity::Native => GREEN,
        antos_protocol::CapabilityFidelity::Emulated => YELLOW,
        antos_protocol::CapabilityFidelity::Simulated => YELLOW,
        antos_protocol::CapabilityFidelity::Unsupported => DIM,
    }
}

pub fn cmd_log(ctx: &Ctx) -> Result<()> {
    let records = crate::journal::read_all(&ctx.journal_path())?;
    if records.is_empty() {
        println!("la bitácora está vacía");
        return Ok(());
    }
    println!();
    println!("{}", paint("bitácora", BOLD));
    for r in records
        .iter()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let mark = match r.outcome {
            Outcome::Executed if r.reverted => paint("↩", DIM),
            Outcome::Executed => paint("✓", GREEN),
            Outcome::Reverted => paint("↩", DIM),
            Outcome::Denied => paint("✗", RED),
            Outcome::Failed => paint("!", RED),
            Outcome::Cancelled => paint("·", DIM),
        };
        let ticket_badge = if let Some(tid) = &r.ticket_id {
            format!("{} ", paint(&format!("[{tid}]"), BOLD))
        } else {
            String::new()
        };
        println!(
            "  {mark} {}  {} {}{}",
            paint(&r.id, DIM),
            paint(&format!("[{}]", r.tier.label()), tier_color(r.tier)),
            ticket_badge,
            ellipsis(&r.intent, 60)
        );
        if let Some(d) = &r.detail {
            println!("      {}", paint(d, DIM));
        }
    }
    println!();
    Ok(())
}

pub fn cmd_grant(ctx: &Ctx, catalog: &Catalog, args: &[String]) -> Result<()> {
    let Some(cap_name) = args.first() else {
        bail!("uso: antos grant <capacidad|secreto> [--minutos N] [--para \"motivo\"]");
    };

    // Si no está en el catálogo directamente, comprobar si es un permiso de secreto o ruta
    if let Ok(cap) = catalog.get(cap_name) {
        if cap.policy.tier != Tier::Grant {
            bail!(
                "{cap_name} es de nivel «{}»: no necesita concesión",
                cap.policy.tier.label()
            );
        }
    }

    let minutes = args
        .iter()
        .position(|a| a == "--minutos" || a == "-m")
        .and_then(|i| args.get(i + 1))
        .and_then(|m| m.parse::<i64>().ok())
        .unwrap_or(10);

    let reason = args
        .iter()
        .position(|a| a == "--para" || a == "--reason" || a == "-p")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let mut grants = Grants::load(&ctx.grants_path())?;
    grants.grant_with_reason(cap_name, minutes, reason.clone());
    grants.save(&ctx.grants_path())?;

    let motivo_str = reason.map(|r| format!(" para «{r}»")).unwrap_or_default();
    println!(
        "\n{} Concesión explícita otorgada a «{}» durante {} minutos{motivo_str}.\n",
        paint("🔑 CONCEDIDA", GREEN),
        paint(cap_name, BOLD),
        paint(&minutes.to_string(), YELLOW)
    );
    Ok(())
}

pub fn cmd_revoke(ctx: &Ctx, args: &[String]) -> Result<()> {
    let Some(cap_name) = args.first() else {
        bail!("uso: antos revoke <capacidad|secreto>");
    };
    let mut grants = Grants::load(&ctx.grants_path())?;
    grants.revoke(cap_name);
    grants.save(&ctx.grants_path())?;
    println!(
        "\n{} Concesión revocada: «{}».\n",
        paint("🔒 REVOCADA", DIM),
        paint(cap_name, BOLD)
    );
    Ok(())
}
