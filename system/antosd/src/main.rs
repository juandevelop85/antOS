//! syso — el recorrido de una intención.
//!
//! captura → planificación → radio de impacto → política → instantánea y
//! ejecución → registro. El modelo solo participa en la planificación, y su
//! salida se valida entera antes de que nadie la vea.

extern crate antos_protocol as antos_protocolo;

pub mod barra;
pub mod autopilot;
mod blast;
pub mod boot;
mod capability;
pub mod collab;
mod ctx;
pub mod desktop;
pub mod diff_view;
pub mod distributed;
pub mod ebpf;
pub mod env;
mod exec;
pub mod flow;
pub mod git;
mod grants;
pub mod installer;
mod ipc;
mod journal;
pub mod lsp;
pub mod memory;
pub mod mesh;
pub mod net;
pub mod notification;
pub mod pkg;
mod plan;
mod planner;
mod preview;
pub mod profiler;
mod protocolo;
mod sandbox;
pub mod service;
mod sesion;
mod snapshot;
pub mod spec;
mod terminal;
pub mod vault;
pub mod vfs;
pub mod vfs_guard;
pub mod vision;
pub mod vm;
mod voz;
pub mod vte;
pub mod wasm;
pub mod web;

use anyhow::{bail, Result};
use capability::{Catalog, Tier};
use ctx::Ctx;
use grants::Grants;
use journal::{Outcome, Record};
use planner::{
    claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner,
    openai_compat::OpenAiCompatPlanner, Planner,
};
use terminal::{ellipsis, paint, tier_color, BLUE, BOLD, CYAN, DIM, GREEN, RED, YELLOW};

#[derive(Default)]
struct Opts {
    assume_yes: bool,
    dry_run: bool,
    planner: Option<String>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {e:#}", paint("error:", RED));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // El ejecutor confinado se atiende antes que nada. Corre DENTRO del
    // recinto, así que no puede crear directorios de estado ni leer el
    // catálogo: solo aplica la lista de cambios que le llega por stdin.
    match std::env::args().nth(1).unwrap_or_default().as_str() {
        sandbox::EXEC_SUBCOMMAND => return sandbox::execute_from_stdin(),
        sandbox::NET_SUBCOMMAND => return sandbox::probe_network_from_inside(),
        _ => {}
    }

    let mut opts = Opts::default();
    let mut rest: Vec<String> = Vec::new();
    let mut argv = std::env::args().skip(1);

    while let Some(a) = argv.next() {
        match a.as_str() {
            "--si" | "-s" | "--yes" | "-y" => opts.assume_yes = true,
            "--seco" | "-n" | "--dry-run" => opts.dry_run = true,
            "--planificador" | "-p" | "--planner" => opts.planner = argv.next(),
            "-h" | "--ayuda" | "--help" => {
                help();
                return Ok(());
            }
            _ => rest.push(a),
        }
    }

    if rest.is_empty() {
        help();
        return Ok(());
    }

    let ctx = Ctx::discover()?;
    let catalog = Catalog::load(&ctx.caps_dir)?;

    match rest[0].as_str() {
        "caps" => cmd_caps(&catalog, &ctx),
        "demonio" => ipc::servir(&ctx, &catalog),
        "doctor" => cmd_doctor(&ctx),
        "escucha" => cmd_escuchar(&ctx, &catalog, &rest[1..], &opts),
        "log" => cmd_log(&ctx),
        "undo" => cmd_undo(&ctx, &rest[1..]),
        "tickets" | "ticket" => cmd_tickets(&ctx, &rest[1..]),
        "ports" => cmd_ports(&rest[1..]),
        "services" | "service" => cmd_services(&ctx, &rest[1..]),
        "secrets" | "secret" => cmd_secrets(&ctx, &rest[1..]),
        "agent" | "agents" | "flow" => cmd_agent(&ctx, &rest[1..]),
        "panel" | "board" => cmd_panel(&ctx, &rest[1..]),
        "llm" | "models" | "model" => cmd_llm(&rest[1..]),
        "memory" | "memoria" | "search" => cmd_memory(&ctx, &rest[1..]),
        "env" | "perfil" => cmd_env(&ctx, &rest[1..]),
        "quota" | "cuota" | "cuotas" | "limits" => cmd_quota(&ctx, &rest[1..]),
        "diff" | "diffs" => cmd_diff(&ctx, &rest[1..]),
        "vte" | "term" | "terminal" => cmd_terminal(&rest[1..]),
        "notify" | "notif" | "notificaciones" => cmd_notify(&ctx, &rest[1..]),
        "mesh" | "p2p" => cmd_mesh(&ctx, &rest[1..]),
        "swarm" => cmd_swarm(&ctx, &rest[1..]),
        "vfs" | "antfs" => cmd_vfs(&ctx, &rest[1..]),
        "ebpf" | "bpf" => cmd_ebpf(&ctx, &rest[1..]),
        "profile" | "perf" | "profiler" => cmd_profile(&ctx, &rest[1..]),
        "lsp" => cmd_lsp(&ctx, &rest[1..]),
        "pair" | "collab" => cmd_pair(&ctx, &rest[1..]),
        "debug" | "dap" => cmd_debug(&ctx, &rest[1..]),
        "desktop" | "wm" => cmd_desktop(&ctx, &rest[1..]),
        "barra" | "bar" => cmd_barra(&ctx, &rest[1..]),
        "boot" | "qemu" => cmd_boot(&ctx, &rest[1..]),
        "plugin" | "plugins" | "wasm" => cmd_plugin(&ctx, &rest[1..]),
        "screenshot" | "captura" => cmd_screenshot(&ctx, &rest[1..]),
        "qa" => cmd_qa(&ctx, &rest[1..]),
        "disk" | "storage" | "part" => cmd_disk(&ctx, &rest[1..]),
        "install" | "installer" => cmd_install(&ctx, &rest[1..]),
        "bootloader" | "uefi" => cmd_bootloader(&ctx, &rest[1..]),
        "vm" | "microvm" => cmd_vm(&ctx, &rest[1..]),
        "pkg" | "antpkg" | "package" => cmd_pkg(&ctx, &rest[1..]),
        "autopilot" | "sentinel" | "centinela" => cmd_autopilot(&ctx, &rest[1..]),
        "web" | "webconsole" | "remote-console" => cmd_web(&ctx, &rest[1..]),
        "project" | "projects" | "proyectos" => cmd_project(&ctx, &rest[1..]),
        "use" => cmd_use(&ctx, &rest[1..]),
        "git" => cmd_git(&ctx, &rest[1..]),
        "release" | "dist" => cmd_boot(&ctx, &["release".into()]),
        "grant" => cmd_grant(&ctx, &catalog, &rest[1..]),
        "revoke" => cmd_revoke(&ctx, &rest[1..]),
        _ => cmd_intent(&ctx, &catalog, &rest.join(" "), &opts),
    }
}

// ---------------------------------------------------------------- intención

fn cmd_intent(ctx: &Ctx, catalog: &Catalog, intent: &str, opts: &Opts) -> Result<()> {
    // Si hay demonio, se le habla. Si no, se hace en proceso.
    //
    // El resultado es idéntico porque ambos caminos usan el MISMO recorrido y
    // el MISMO dibujado: lo único que cambia es dónde corre cada mitad.
    let forzar_local = std::env::var_os("ANTOS_SIN_DEMONIO")
        .or_else(|| std::env::var_os("SYSO_SIN_DEMONIO"))
        .is_some();
    if !forzar_local && ipc::hay_demonio(ctx) {
        return ipc::intencion_remota(
            &ipc::ruta_socket(ctx),
            intent,
            opts.planner.as_deref(),
            opts.dry_run,
            opts.assume_yes,
        );
    }

    let planificador = pick_planner_por_nombre(opts.planner.as_deref())?;
    let mut terminal = terminal::Terminal::new(opts.assume_yes);
    sesion::intencion(
        ctx,
        catalog,
        intent,
        &*planificador,
        opts.dry_run,
        &mut terminal,
    )
}

// --------------------------------------------------------------------- voz

/// Escuchar es capturar y transcribir. A partir de ahí, el recorrido es
/// exactamente el mismo que si lo hubieras tecleado — incluidos el diff y la
/// confirmación. La voz no salta ningún control: hablar es más cómodo, no
/// más privilegiado.
fn cmd_escuchar(ctx: &Ctx, catalog: &Catalog, args: &[String], opts: &Opts) -> Result<()> {
    if args.iter().any(|a| a == "--dispositivos") {
        println!();
        println!("{}", paint("dispositivos de audio", BOLD));
        for linea in voz::Voz::dispositivos()?.lines() {
            println!("  {linea}");
        }
        println!();
        println!(
            "{}",
            paint("elige uno con: antos escucha --dispositivo N", DIM)
        );
        return Ok(());
    }

    let voz = voz::Voz::discover()?;

    let segundos = args
        .iter()
        .position(|a| a == "--segundos")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(5);
    let desde = args
        .iter()
        .position(|a| a == "--desde")
        .and_then(|i| args.get(i + 1));
    let dispositivo = args
        .iter()
        .position(|a| a == "--dispositivo")
        .and_then(|i| args.get(i + 1))
        .map(|d| format!(":{d}"))
        .unwrap_or_else(|| ":default".to_string());

    println!();
    println!("{}", paint("antOS · escucha", BOLD));
    println!(
        "  {}",
        paint(&format!("modelo local: {}", voz.modelo().display()), DIM)
    );

    let captura = ctx.state.join("captura.wav");
    match desde {
        Some(fichero) => {
            voz.normalizar(std::path::Path::new(fichero), &captura)?;
            println!("  {}", paint(&format!("desde el fichero {fichero}"), DIM));
        }
        None => {
            println!(
                "  {}",
                paint(
                    &format!("grabando {segundos} s desde {dispositivo} · habla ahora"),
                    BOLD
                )
            );
            voz.grabar(segundos, &captura, &dispositivo)?;
        }
    }

    let intencion = voz.transcribir(&captura, &vocabulario(catalog))?;
    let _ = std::fs::remove_file(&captura);

    println!(
        "  {} {}",
        paint("he entendido:", DIM),
        paint(&format!("«{intencion}»"), BOLD)
    );

    // Y desde aquí, todo igual que si lo hubieras escrito.
    cmd_intent(ctx, catalog, &intencion, opts)
}

/// Construye la frase con la que se ceba el transcriptor.
///
/// Sale del catálogo, no de una lista escrita a mano: si mañana aparece una
/// capacidad que acepta un lenguaje nuevo, el transcriptor lo aprende solo.
fn vocabulario(catalog: &Catalog) -> String {
    let mut opciones: std::collections::BTreeSet<&str> = Default::default();
    for cap in catalog.caps.values() {
        for spec in cap.params.values() {
            opciones.extend(spec.of.iter().map(String::as_str));
        }
    }

    format!(
        "Órdenes para syso. Crear un proyecto {}. \
         Declarar una dependencia en un proyecto. \
         Leer, escribir o borrar un fichero del espacio de trabajo.",
        opciones.into_iter().collect::<Vec<_>>().join(", ")
    )
}

// ------------------------------------------------------------------ deshacer

fn cmd_undo(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut records = journal::read_all(&ctx.journal_path())?;

    let ticket_id_arg = args
        .iter()
        .position(|a| a == "--ticket" || a == "-t")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| {
            if !args.is_empty() && !args[0].starts_with('-') {
                Some(args[0].clone())
            } else {
                None
            }
        });

    if let Some(raw_tid) = ticket_id_arg {
        let tid = raw_tid.to_uppercase();
        println!(
            "\n{} {}",
            paint("antOS · Reversión granular de ticket", BOLD),
            paint(&tid, YELLOW)
        );

        let mut reversiones = 0;
        let mut idxs_a_revertir = Vec::new();

        for (idx, r) in records.iter().enumerate() {
            if r.outcome == Outcome::Ejecutado && !r.reverted && r.snapshot.is_some() {
                let coincide_ticket = r.ticket_id.as_deref() == Some(&tid)
                    || journal::extraer_ticket_id(&r.intent).as_deref() == Some(&tid);
                if coincide_ticket {
                    idxs_a_revertir.push(idx);
                }
            }
        }

        if idxs_a_revertir.is_empty() {
            let ticket_clean = tid.to_lowercase();
            let wt_path = ctx.state.join("worktrees").join(&ticket_clean);
            if wt_path.exists() {
                let _ = std::fs::remove_dir_all(&wt_path);
                println!(
                    "  {} Worktree efímero ({}) limpiado.",
                    paint("✓", GREEN),
                    wt_path.display()
                );
                reversiones += 1;
            }

            if reversiones == 0 {
                println!("  No hay transacciones ejecutadas ni cambios pendientes para el ticket «{tid}».\n");
                return Ok(());
            }
        }

        idxs_a_revertir.reverse();
        for idx in idxs_a_revertir {
            let snap_id = records[idx].snapshot.clone().unwrap();
            let snap = snapshot::load(&snap_id, &ctx.snapshots_dir())?;

            println!(
                "  {} Transacción {}: «{}»",
                paint("↩", DIM),
                paint(&records[idx].id, DIM),
                records[idx].intent
            );
            for line in snapshot::restore(&snap)? {
                println!("    {line}");
            }

            records[idx].reverted = true;
            let undone = Record {
                id: plan::new_id(),
                at: chrono::Local::now().to_rfc3339(),
                intent: format!("deshacer ticket {tid} ({})", records[idx].id),
                ticket_id: Some(tid.clone()),
                planner: "antos".into(),
                plan: records[idx].plan.clone(),
                tier: records[idx].tier,
                reasons: vec![format!("revierte instantánea {snap_id} del ticket {tid}")],
                outcome: Outcome::Revertido,
                detail: None,
                sandbox: "broker".into(),
                snapshot: Some(snap_id),
                reverted: false,
            };
            journal::append(&ctx.journal_path(), &undone)?;
            reversiones += 1;
        }

        let ticket_clean = tid.to_lowercase();
        let wt_path = ctx.state.join("worktrees").join(&ticket_clean);
        if wt_path.exists() {
            let _ = std::fs::remove_dir_all(&wt_path);
            println!(
                "  {} Worktree efímero ({}) eliminado.",
                paint("✓", GREEN),
                wt_path.display()
            );
        }

        journal::rewrite(&ctx.journal_path(), &records)?;
        println!(
            "\n  {} Reversión completada: {} transacción(es) revertida(s).\n",
            paint("✓", GREEN),
            reversiones
        );
        return Ok(());
    }

    let idx = records
        .iter()
        .rposition(|r| r.outcome == Outcome::Ejecutado && !r.reverted && r.snapshot.is_some());

    let Some(idx) = idx else {
        println!("no hay nada que deshacer");
        return Ok(());
    };

    let snap_id = records[idx].snapshot.clone().unwrap();
    let snap = snapshot::load(&snap_id, &ctx.snapshots_dir())?;

    println!();
    println!(
        "{} {}",
        paint("deshaciendo", BOLD),
        paint(&format!("«{}»", records[idx].intent), DIM)
    );
    for line in snapshot::restore(&snap)? {
        println!("  {line}");
    }

    records[idx].reverted = true;
    let undone = Record {
        id: plan::new_id(),
        at: chrono::Local::now().to_rfc3339(),
        intent: format!("deshacer {}", records[idx].id),
        ticket_id: records[idx].ticket_id.clone(),
        planner: "antos".into(),
        plan: records[idx].plan.clone(),
        tier: records[idx].tier,
        reasons: vec![format!("revierte la instantánea {snap_id}")],
        outcome: Outcome::Revertido,
        detail: None,
        sandbox: "broker".into(),
        snapshot: Some(snap_id),
        reverted: false,
    };
    journal::rewrite(&ctx.journal_path(), &records)?;
    journal::append(&ctx.journal_path(), &undone)?;

    println!();
    println!("{}", paint("✓ revertido", GREEN));
    Ok(())
}

// ------------------------------------------------------------ otros comandos

fn cmd_caps(catalog: &Catalog, ctx: &Ctx) -> Result<()> {
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
fn cmd_doctor(ctx: &Ctx) -> Result<()> {
    let jail = sandbox::for_host();
    println!();
    println!("{}", paint("recinto de ejecución", BOLD));
    println!("  motor     {}", jail.name());
    println!("  garantiza {}", paint(jail.guarantees(), DIM));
    println!();

    let mut fallos = 0;

    // Política de prueba: solo se declara el espacio de trabajo, sin red.
    let solo_workspace = sandbox::Policy {
        writes: vec![ctx.workspace.clone()],
        reads: vec![],
        dirs: vec![ctx.workspace.clone()],
        network: false,
        allowed_secrets: vec![],
        quota: None,
    };

    // 1) escribir fuera de lo declarado
    let fuga = ctx.state.join("doctor-fuga.txt");
    let _ = std::fs::remove_file(&fuga);
    let intento = sandbox::run(
        &*jail,
        &[exec::Change::Write {
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

    // 2) escribir dentro de lo declarado debe seguir funcionando: un recinto
    //    que lo bloquea todo no es seguro, es inútil.
    let dentro = ctx.workspace.join(".doctor-prueba");
    let _ = std::fs::remove_file(&dentro);
    let permitido = sandbox::run(
        &*jail,
        &[exec::Change::Write {
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

    // 3) leer fuera de lo declarado. Es la garantía que separa a Landlock de
    //    Seatbelt, así que se pregunta al motor qué promete antes de juzgar.
    let secreto = ctx.state.join("doctor-secreto.txt");
    std::fs::write(&secreto, "credencial de mentira")?;
    let lectura = sandbox::run(
        &*jail,
        &[exec::Change::Read {
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

    // 4) red. Se prueba en los dos sentidos para no confundir «bloqueada»
    //    con «esta máquina no tiene internet».
    let con_red = sandbox::Policy {
        writes: vec![],
        reads: vec![],
        dirs: vec![],
        network: true,
        allowed_secrets: vec![],
        quota: None,
    };
    let alcanzable_declarando = sandbox::probe_network(&*jail, &con_red).unwrap_or(false);
    let alcanzable_sin_declarar = sandbox::probe_network(&*jail, &solo_workspace).unwrap_or(false);

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

    println!();
    if fallos == 0 {
        println!("{}", paint("✓ el recinto se comporta como dice", GREEN));
        Ok(())
    } else {
        bail!("{fallos} comprobación(es) del recinto han fallado")
    }
}

fn marca(ok: bool, texto: &str) {
    let (simbolo, color) = if ok { ("✓", GREEN) } else { ("✗", RED) };
    println!("  {} {texto}", paint(simbolo, color));
}

fn cmd_log(ctx: &Ctx) -> Result<()> {
    let records = journal::read_all(&ctx.journal_path())?;
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
            Outcome::Ejecutado if r.reverted => paint("↩", DIM),
            Outcome::Ejecutado => paint("✓", GREEN),
            Outcome::Revertido => paint("↩", DIM),
            Outcome::Denegado => paint("✗", RED),
            Outcome::Fallido => paint("!", RED),
            Outcome::Cancelado => paint("·", DIM),
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

fn cmd_grant(ctx: &Ctx, catalog: &Catalog, args: &[String]) -> Result<()> {
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

fn cmd_revoke(ctx: &Ctx, args: &[String]) -> Result<()> {
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

fn cmd_secrets(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let grants = Grants::load(&ctx.grants_path())?;

    match sub {
        "set" => {
            let key = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos secret set <CLAVE> <VALOR>"))?;
            let val = args
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("uso: antos secret set <CLAVE> <VALOR>"))?;
            vault::set_secret(&ctx.state, key, val)?;
            println!(
                "\n{} Secreto «{}» almacenado de forma segura en la bóveda de antOS.\n",
                paint("✓", GREEN),
                paint(key, BOLD)
            );
        }
        "get" => {
            let key = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos secret get <CLAVE>"))?;
            match vault::get_secret(&ctx.state, key, &grants) {
                Ok(Some(v)) => {
                    println!("\n{} {key} = {}\n", paint("🔑", BOLD), paint(&v, GREEN));
                }
                Ok(None) => {
                    println!(
                        "\n{} El secreto «{key}» no existe en la bóveda.\n",
                        paint("○", DIM)
                    );
                }
                Err(e) => {
                    println!(
                        "\n{} {e}\n",
                        paint("🛡️ Cero Autoridad Ambiental (Bloqueado):", RED)
                    );
                }
            }
        }
        "list" | _ => {
            let list = vault::list_secrets(&ctx.state)?;
            let active_grants = grants.list_active();

            println!(
                "\n{}",
                paint(
                    "antOS · Bóveda de Secretos y Blindaje Zero Environmental Authority (T5.2)",
                    BOLD
                )
            );

            // Concesiones activas
            println!("  {}", paint("● CONCESIONES ACTIVAS", BOLD));
            if active_grants.is_empty() {
                println!(
                    "    {} No hay concesiones activas. Blindaje al 100%.",
                    paint("○", DIM)
                );
            } else {
                for g in active_grants {
                    let mins_left = ((g.expires_at - chrono::Local::now().timestamp()) / 60).max(1);
                    let reason_str = g
                        .reason
                        .as_deref()
                        .map(|r| format!(" (motivo: «{r}»)"))
                        .unwrap_or_default();
                    println!(
                        "    {} {:<20} expira en {:>2} min{reason_str}",
                        paint("●", GREEN),
                        paint(&g.cap, BOLD),
                        paint(&mins_left.to_string(), YELLOW)
                    );
                }
            }
            println!();

            // Secretos almacenados
            println!(
                "  {}",
                paint("● SECRETOS EN BÓVEDA ($STATE/vault.json)", BOLD)
            );
            if list.is_empty() {
                println!("    No hay secretos en la bóveda.");
                println!("    Guarda uno con: antos secret set <CLAVE> <VALOR>\n");
            } else {
                println!("    ┌──────────────────────────────┬──────────────┬────────────────────────────┐");
                println!(
                    "    │ {:<28} │ {:<12} │ {:<26} │",
                    paint("CLAVE", BOLD),
                    paint("LONGITUD", BOLD),
                    paint("ESTADO DE ACCESO", BOLD)
                );
                println!("    ├──────────────────────────────┼──────────────┼────────────────────────────┤");
                for s in list {
                    let has_grant = grants.is_granted("secret.read")
                        || grants.is_granted(&format!("secret.{}", s.key));
                    let acc_str = if has_grant {
                        paint("🔓 Concedido", GREEN)
                    } else {
                        paint("🔒 Protegido (Grant req)", YELLOW)
                    };
                    println!(
                        "    │ {:<28} │ {:<12} │ {:<37} │",
                        paint(&s.key, BOLD),
                        format!("{} bytes", s.length),
                        acc_str
                    );
                }
                println!("    └──────────────────────────────┴──────────────┴────────────────────────────┘\n");
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ apoyo

pub(crate) fn pick_planner_por_nombre(nombre: Option<&str>) -> Result<Box<dyn Planner>> {
    match nombre {
        Some("local") => Ok(Box::new(LocalPlanner)),
        Some("claude") => Ok(Box::new(ClaudePlanner::from_env()?)),
        Some("ollama" | "local-llm" | "local_llm") => Ok(Box::new(OllamaPlanner::from_env()?)),
        Some("groq") => Ok(Box::new(OpenAiCompatPlanner::from_preset("groq")?)),
        Some("openrouter" | "open-router") => Ok(Box::new(OpenAiCompatPlanner::from_preset("openrouter")?)),
        Some("gemini" | "google") => Ok(Box::new(OpenAiCompatPlanner::from_preset("gemini")?)),
        Some("opencode" | "localai" | "vllm") => Ok(Box::new(OpenAiCompatPlanner::from_preset("opencode")?)),
        Some("openai" | "openai_compat" | "compat") => Ok(Box::new(OpenAiCompatPlanner::from_preset("openai")?)),
        Some(other) => {
            if other.starts_with("http://") || other.starts_with("https://") {
                Ok(Box::new(OpenAiCompatPlanner::new("custom", other, "default", None)))
            } else {
                bail!("planificador desconocido: {other} (usa «local», «ollama», «groq», «openrouter», «gemini», «opencode» o «claude»)")
            }
        }
        // Jerarquía de fallback transparente:
        // 1. Proveedores en la nube con free tiers o claves configuradas.
        // 2. Claude si hay clave de API configurada.
        // 3. Ollama local si está disponible en la máquina.
        // 4. OpenCode / llama.cpp local si está disponible en puerto 8080.
        // 5. Planificador local determinista sin dependencias externas.
        None => {
            if let Ok(p) = OpenAiCompatPlanner::from_preset("groq") {
                return Ok(Box::new(p));
            }
            if let Ok(p) = OpenAiCompatPlanner::from_preset("openrouter") {
                return Ok(Box::new(p));
            }
            if let Ok(p) = ClaudePlanner::from_env() {
                return Ok(Box::new(p));
            }
            if let Ok(p) = OpenAiCompatPlanner::from_preset("gemini") {
                return Ok(Box::new(p));
            }
            if let Ok(o) = OllamaPlanner::from_env() {
                if o.is_available() {
                    return Ok(Box::new(o));
                }
            }
            if let Ok(oc) = OpenAiCompatPlanner::from_preset("opencode") {
                if oc.is_available() {
                    return Ok(Box::new(oc));
                }
            }
            Ok(Box::new(LocalPlanner))
        }
    }
}

// ------------------------------------------------------------------ llm

fn cmd_llm(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let ollama = OllamaPlanner::from_env()?;

    match sub {
        "list" | "models" => {
            if !ollama.is_available() {
                println!(
                    "\n{} No se pudo conectar con Ollama en {}\n  Asegúrate de que el servicio esté corriendo con: ollama serve\n",
                    paint("✗ Ollama no responde:", RED),
                    paint(&ollama.endpoint, BOLD)
                );
                return Ok(());
            }

            let models = ollama.list_models()?;
            println!(
                "\n{}",
                paint("antOS · Modelos LLM Locales Disponibles (Ollama)", BOLD)
            );
            println!("  Endpoint: {}", paint(&ollama.endpoint, GREEN));
            println!("  Modelo Activo: {}\n", paint(&ollama.model, BOLD));

            if models.is_empty() {
                println!("  No hay modelos descargados en Ollama.");
                println!(
                    "  Descarga uno con: ollama pull qwen2.5-coder o ollama pull deepseek-coder\n"
                );
            } else {
                for m in models {
                    let is_active = m.starts_with(&ollama.model) || ollama.model.starts_with(&m);
                    let mark = if is_active {
                        paint("●", GREEN)
                    } else {
                        paint("○", DIM)
                    };
                    let tag = if is_active {
                        paint("(activo)", YELLOW)
                    } else {
                        "".to_string()
                    };
                    println!("  {mark} {:<30} {tag}", paint(&m, BOLD));
                }
                println!();
            }
        }
        "status" | _ => {
            println!(
                "\n{}",
                paint("antOS · Estado de Motores de Inferencia LLM (T6.1)", BOLD)
            );

            // 1. Proveedor Claude
            let claude_status = match ClaudePlanner::from_env() {
                Ok(_) => paint("● Conectado (Clave API detectada)", GREEN),
                Err(_) => paint("○ No configurado (Sin ANTHROPIC_API_KEY)", DIM),
            };
            println!("  ● Claude API (Nube):");
            println!("    Estado: {claude_status}");

            // 2. Proveedor Ollama local
            let is_ollama_up = ollama.is_available();
            let ollama_status = if is_ollama_up {
                paint("● Online (Local)", GREEN)
            } else {
                paint("○ Desconectado (Servidor no responde)", YELLOW)
            };

            println!("\n  ● Ollama / Local LLM (Offline):");
            println!("    Endpoint: {}", paint(&ollama.endpoint, BOLD));
            println!("    Modelo configurado: {}", paint(&ollama.model, BOLD));
            println!("    Estado: {ollama_status}");

            if is_ollama_up {
                if let Ok(models) = ollama.list_models() {
                    println!(
                        "    Modelos instalados: {}",
                        paint(&format!("{} modelos", models.len()), GREEN)
                    );
                }
            } else {
                println!("    Nota: Para arrancar Ollama ejecuta: ollama serve");
            }

            // 3. Fallback determinista
            println!("\n  ● Planificador Determinista Local:");
            println!(
                "    Estado: {}",
                paint("● Siempre activo (Reglas locales deterministas)", GREEN)
            );
            println!();
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ memory

fn cmd_memory(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let db_path = memory::MemoryEngine::default_db_path(&ctx.workspace);

    match sub {
        "index" | "reindex" => {
            println!(
                "\n{} Escaneando e indexando espacio de trabajo: {}",
                paint("●", GREEN),
                paint(&ctx.workspace.display().to_string(), BOLD)
            );
            let store = memory::MemoryEngine::index_workspace(&ctx.workspace)?;
            memory::MemoryEngine::save(&store, &db_path)?;
            println!(
                "{} Indexación completada: {} fragmentos y {} nodos de grafo guardados en {}\n",
                paint("✓", GREEN),
                paint(&store.chunks.len().to_string(), BOLD),
                paint(&store.graph.nodes.len().to_string(), BOLD),
                paint(&db_path.display().to_string(), DIM)
            );
        }
        "search" | "find" => {
            let query = args.get(1).map(String::as_str).unwrap_or_default();
            if query.is_empty() {
                bail!("uso: antos memory search <texto_de_busqueda> [--limit N]");
            }
            let limit = args
                .iter()
                .position(|a| a == "--limit" || a == "-n")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(5);

            let store = if db_path.exists() {
                memory::MemoryEngine::load(&db_path)?
            } else {
                println!(
                    "{} No existe índice previo. Indexando espacio de trabajo por primera vez...",
                    paint("i", YELLOW)
                );
                let s = memory::MemoryEngine::index_workspace(&ctx.workspace)?;
                memory::MemoryEngine::save(&s, &db_path)?;
                s
            };

            let hits = memory::MemoryEngine::search(&store, query, limit);
            println!(
                "\n{}",
                paint(
                    &format!("antOS · Búsqueda Semántica Vectorial para «{query}»"),
                    BOLD
                )
            );
            println!("  Resultados encontrados: {}\n", hits.len());

            if hits.is_empty() {
                println!("  No se encontraron coincidencias relevantes en el código o tickets.\n");
            } else {
                for (idx, h) in hits.iter().enumerate() {
                    let score_badge = paint(&format!("[{:.2}]", h.score), GREEN);
                    let kind_badge = paint(&format!("{:?}", h.kind), DIM);
                    println!(
                        "  {}. {} {} {}:{}",
                        idx + 1,
                        score_badge,
                        kind_badge,
                        paint(&h.path, BOLD),
                        h.line_start
                    );
                    println!("     Título: {}", paint(&h.title, YELLOW));
                    println!("     Extracto: {}\n", paint(&h.snippet, DIM));
                }
            }
        }
        "graph" => {
            let target = args.get(1).map(String::as_str);
            let store = if db_path.exists() {
                memory::MemoryEngine::load(&db_path)?
            } else {
                let s = memory::MemoryEngine::index_workspace(&ctx.workspace)?;
                memory::MemoryEngine::save(&s, &db_path)?;
                s
            };

            println!(
                "\n{}",
                paint(
                    "antOS · Grafo de Contexto y Dependencias del Proyecto",
                    BOLD
                )
            );
            match target {
                Some(t) => {
                    let related = store.graph.related_to(t);
                    println!(
                        "  Relaciones para símbolo o archivo «{}»: {}\n",
                        paint(t, BOLD),
                        related.len()
                    );
                    for (node, edge) in related {
                        println!(
                            "    • {:<18} ──> {} ({})",
                            format!("{:?}", edge),
                            paint(&node.label, BOLD),
                            node.kind
                        );
                    }
                    println!();
                }
                None => {
                    println!(
                        "  Total de nodos:   {}",
                        paint(&store.graph.nodes.len().to_string(), GREEN)
                    );
                    println!(
                        "  Total de aristas: {}\n",
                        paint(&store.graph.edges.len().to_string(), GREEN)
                    );
                    println!("  Usa: antos memory graph <nodo> para inspeccionar relaciones.");
                    println!("  Ejemplo: antos memory graph ticket:T6.1 o file:system/antosd/src/main.rs\n");
                }
            }
        }
        "status" | _ => {
            let exists = db_path.exists();
            println!(
                "\n{}",
                paint("antOS · Memoria Semántica y Grafo de Contexto (T6.2)", BOLD)
            );
            println!(
                "  Ubicación: {}",
                paint(&db_path.display().to_string(), DIM)
            );
            if exists {
                if let Ok(store) = memory::MemoryEngine::load(&db_path) {
                    println!("  Estado:    {}", paint("● Activo / Sincronizado", GREEN));
                    println!(
                        "  Fragmentos: {}",
                        paint(&store.chunks.len().to_string(), BOLD)
                    );
                    println!(
                        "  Nodos:      {}",
                        paint(&store.graph.nodes.len().to_string(), BOLD)
                    );
                    println!(
                        "  Aristas:    {}",
                        paint(&store.graph.edges.len().to_string(), BOLD)
                    );
                } else {
                    println!(
                        "  Estado:    {}",
                        paint("! Archivo de memoria corrupto", RED)
                    );
                }
            } else {
                println!(
                    "  Estado:    {}",
                    paint("○ Sin indexar (Ejecuta: antos memory index)", YELLOW)
                );
            }
            println!();
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ env

fn cmd_env(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "init" | "setup" => {
            let profile_arg = args.get(1).map(String::as_str);
            let profile = match profile_arg {
                Some(p) => env::EnvProfile::from_str_loose(p).ok_or_else(|| {
                    anyhow::anyhow!(
                        "perfil desconocido «{p}». Opciones válidas: rust, node, python, go, base"
                    )
                })?,
                None => {
                    env::EnvEngine::detect_stack(&ctx.workspace).unwrap_or(env::EnvProfile::Base)
                }
            };

            let summary = env::EnvEngine::init_profile(&ctx.workspace, profile, true, true)?;
            println!(
                "\n{} Perfil de entorno declarativo inicializado exitosamente.",
                paint("✓", GREEN)
            );
            println!("  Perfil:   {}", paint(&summary.profile, BOLD));
            println!(
                "  Archivos: {}",
                paint(&summary.created_files.join(", "), GREEN)
            );
            println!("  Paquetes: {}\n", paint(&summary.packages.join(", "), DIM));
            println!(
                "  Ejecuta: antos env sync para comprobar la disponibilidad de las herramientas.\n"
            );
        }
        "sync" | "check" => {
            println!(
                "\n{}",
                paint(
                    "antOS · Sincronización y Diagnóstico de Toolchains (T7.1)",
                    BOLD
                )
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            let statuses = env::EnvEngine::check_toolchains(&ctx.workspace)?;
            let mut all_ok = true;

            for s in statuses {
                if s.available {
                    let loc = s.path.unwrap_or_default();
                    println!(
                        "  {} {:<16} ({})",
                        paint("✓", GREEN),
                        paint(&s.name, BOLD),
                        paint(&loc, DIM)
                    );
                } else {
                    all_ok = false;
                    println!(
                        "  {} {:<16} ({})",
                        paint("✗", RED),
                        paint(&s.name, BOLD),
                        paint("no instalado en el sistema o nix-store", RED)
                    );
                }
            }

            println!();
            if all_ok {
                println!(
                    "  {} Todas las toolchains declaradas están disponibles y operativas.\n",
                    paint("✓ Entorno listo:", GREEN)
                );
            } else {
                println!("  {} Faltan herramientas por aprovisionar. Puedes usar devbox shell o nix develop.\n", paint("! Advertencia:", YELLOW));
            }
        }
        "status" | _ => {
            println!(
                "\n{}",
                paint("antOS · Estado del Perfil de Entorno (T7.1)", BOLD)
            );
            let cfg = env::EnvEngine::load_config(&ctx.workspace)?;

            match cfg {
                Some(c) => {
                    println!("  Perfil activo:      {}", paint(&c.profile, GREEN));
                    println!(
                        "  Paquetes declarados: {}",
                        paint(&c.packages.join(", "), BOLD)
                    );
                    let statuses = env::EnvEngine::check_toolchains(&ctx.workspace)?;
                    let available_count = statuses.iter().filter(|s| s.available).count();
                    println!(
                        "  Disponibilidad:     {}/{} herramientas en PATH\n",
                        available_count,
                        statuses.len()
                    );
                }
                None => {
                    let detected = env::EnvEngine::detect_stack(&ctx.workspace);
                    let det_str = detected.map(|d| d.as_str()).unwrap_or("no detectado");
                    println!("  Perfil configurado: {}", paint("○ Ninguno", YELLOW));
                    println!("  Stack detectado:    {}", paint(det_str, BOLD));
                    println!("\n  Usa antos env init [{det_str}] para inicializar devbox.json y flake.nix.\n");
                }
            }
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ quota

fn cmd_quota(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "set" => {
            let mut q = sandbox::quota::load_quota(&ctx.workspace)?;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--timeout" | "-t" => {
                        if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u64>().ok()) {
                            q.timeout_secs = val;
                            i += 1;
                        }
                    }
                    "--memory" | "-m" => {
                        if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u64>().ok()) {
                            q.max_memory_mb = val;
                            i += 1;
                        }
                    }
                    "--cpu" | "-c" => {
                        if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                            q.cpu_quota_percent = val;
                            i += 1;
                        }
                    }
                    "--pids" | "-p" => {
                        if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                            q.max_pids = val;
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }

            sandbox::quota::save_quota(&ctx.workspace, &q)?;
            println!(
                "\n{} Cuotas de recursos de sandbox actualizadas.",
                paint("✓", GREEN)
            );
            println!(
                "  • Timeout:   {}s",
                paint(&q.timeout_secs.to_string(), BOLD)
            );
            println!(
                "  • Memoria:   {} MB",
                paint(&q.max_memory_mb.to_string(), BOLD)
            );
            println!(
                "  • CPU:       {}%",
                paint(&q.cpu_quota_percent.to_string(), BOLD)
            );
            println!(
                "  • Max PIDs:  {} procesos\n",
                paint(&q.max_pids.to_string(), BOLD)
            );
        }
        "reset" => {
            let def = sandbox::quota::ResourceQuota::default();
            sandbox::quota::save_quota(&ctx.workspace, &def)?;
            println!(
                "\n{} Cuotas de sandbox restablecidas a los valores por defecto del sistema.\n",
                paint("✓", GREEN)
            );
        }
        "status" | _ => {
            println!(
                "\n{}",
                paint(
                    "antOS · Cuotas y Límites de Recursos para Sandboxes (T7.2)",
                    BOLD
                )
            );
            let q = sandbox::quota::load_quota(&ctx.workspace)?;
            let cgroup_avail = sandbox::quota::CgroupV2Manager::is_available();

            let backend = if cgroup_avail {
                paint("● cgroups v2 (Linux)", GREEN)
            } else {
                paint("● Seatbelt + Supervisor de Procesos (macOS)", BLUE)
            };

            println!("  Mecanismo:          {backend}");
            println!(
                "  Timeout de agente:  {}",
                paint(&format!("{}s", q.timeout_secs), GREEN)
            );
            println!(
                "  Límite de memoria:  {}",
                paint(&format!("{} MB", q.max_memory_mb), GREEN)
            );
            println!(
                "  Cuota de CPU:       {}",
                paint(&format!("{}%", q.cpu_quota_percent), GREEN)
            );
            println!(
                "  Límite de procesos: {}\n",
                paint(&format!("{} PIDs", q.max_pids), GREEN)
            );
            println!(
                "  Usa antos quota set [--timeout N] [--memory N] [--cpu N] para modificar.\n"
            );
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ diff

fn cmd_diff(ctx: &Ctx, args: &[String]) -> Result<()> {
    // T17.2 argument parsing:
    //   antos diff                  — detect project from cwd or scan workspace
    //   antos diff <project>        — diff the named project
    //   antos diff <project> <ref>  — diff the named project against <ref>
    //   antos diff <ref>            — backwards-compat: diff active project / workspace against <ref>
    //
    // A token is treated as a project name when it matches a directory under workspace/.
    // Otherwise it is treated as a git target ref.

    let antos_root = ctx.antos_root.clone();
    let workspace  = &ctx.workspace;

    println!(
        "\n{}",
        paint("antOS · Visor Interactivo de Diffs y Parches (T8.1 / T17.2)", BOLD)
    );
    println!(
        "  Espacio de trabajo: {}",
        paint(&workspace.display().to_string(), DIM)
    );

    // Parse arguments into (project_path, target_ref).
    let (project_path, target) = parse_diff_args(args, workspace, ctx.current_project.as_deref());

    if let Some(ref p) = project_path {
        println!("  Proyecto:           {}", paint(&p.display().to_string(), DIM));
    } else if let Some(ref p) = ctx.current_project {
        println!("  Proyecto activo:    {}", paint(&p.display().to_string(), DIM));
    }
    println!("  Objetivo:           {}\n", paint(&target, BOLD));

    // Case A: a specific project was requested — diff only that project.
    if let Some(proj) = project_path {
        diff_single_project(&proj, &target, antos_root.as_deref());
        return Ok(());
    }

    // Case B: we are inside a project (contextual detection from T17.1).
    if let Some(ref proj) = ctx.current_project {
        diff_single_project(proj, &target, antos_root.as_deref());
        return Ok(());
    }

    // Case C: no project context — scan all projects in workspace/.
    let projects = crate::exec::scan_workspace_projects(workspace);
    if projects.is_empty() {
        println!(
            "  {} No se encontraron proyectos en el espacio de trabajo.\n",
            paint("ℹ Sin proyectos:", DIM)
        );
        return Ok(());
    }

    println!(
        "  {} {} proyecto(s) detectado(s)\n",
        paint("↓ Escaneando:", CYAN),
        projects.len()
    );

    for proj in &projects {
        let proj_name = proj.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| proj.display().to_string());
        println!("  {} {}", paint("┌ proyecto:", BOLD), paint(&proj_name, CYAN));
        diff_single_project(proj, &target, antos_root.as_deref());
    }

    Ok(())
}

/// Parses CLI arguments into (optional project path, target ref).
///
/// Logic:
/// - If the first arg is a directory under `workspace/`, it is the project.
///   The second arg (if present) is the target ref.
/// - Otherwise the first arg (if present) is the target ref.
/// - Falls back to (current_project, "HEAD").
fn parse_diff_args(
    args: &[String],
    workspace: &std::path::Path,
    current_project: Option<&std::path::Path>,
) -> (Option<std::path::PathBuf>, String) {
    match args {
        [] => (None, "HEAD".into()),
        [first] => {
            let candidate = workspace.join(first);
            if candidate.is_dir() {
                (Some(candidate), "HEAD".into())
            } else {
                // Treat as a target ref, keep detected project (or None).
                (current_project.map(|p| p.to_path_buf()), first.clone())
            }
        }
        [first, second, ..] => {
            let candidate = workspace.join(first);
            if candidate.is_dir() {
                (Some(candidate), second.clone())
            } else {
                // first is a ref, not a project name.
                (current_project.map(|p| p.to_path_buf()), first.clone())
            }
        }
    }
}

/// Diffs a single project directory, printing the result to stdout.
///
/// Uses ceiling-aware git root detection (T17.1) to verify the project has its
/// own `.git` before invoking `git diff`. If it does not, prints an informative
/// file listing and actionable guidance.
fn diff_single_project(
    proj: &std::path::Path,
    target: &str,
    antos_root: Option<&std::path::Path>,
) {
    let project_name = proj.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| proj.display().to_string());

    let has_git = git::find_git_root_with_ceiling(proj, antos_root).is_some();

    if !has_git {
        let files = crate::exec::collect_project_files(proj, 20);
        if files.is_empty() {
            println!(
                "  {} {} — directorio vacío (sin archivos ni repositorio Git)\n",
                paint("⚠", YELLOW),
                paint(&project_name, BOLD)
            );
        } else {
            println!(
                "  {} {} — {} archivo(s) detectado(s), sin repositorio Git:",
                paint("⚠", YELLOW),
                paint(&project_name, BOLD),
                files.len()
            );
            for f in &files {
                println!("    {}", paint(f, DIM));
            }
            println!();
            println!(
                "  {} Inicializa el repositorio con: {}",
                paint("→", CYAN),
                paint(&format!("antos project init {}", project_name), DIM)
            );
            println!();
        }
        return;
    }

    // Build git diff with ceiling.
    let ceiling_val = antos_root
        .and_then(|r| r.parent())
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    // Check if HEAD exists. If not, the repo is newly initialized with no commits.
    let mut check_head = std::process::Command::new("git");
    check_head.current_dir(proj).args(["rev-parse", "--verify", "HEAD"]);
    if !ceiling_val.is_empty() {
        check_head.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
    }
    let has_commits = check_head.output().map(|o| o.status.success()).unwrap_or(false);

    if !has_commits && target == "HEAD" {
        let mut status_cmd = std::process::Command::new("git");
        status_cmd.current_dir(proj).args(["status", "--porcelain"]);
        if !ceiling_val.is_empty() {
            status_cmd.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
        }
        if let Ok(st_out) = status_cmd.output() {
            let st_str = String::from_utf8_lossy(&st_out.stdout);
            let lines: Vec<&str> = st_str.lines().filter(|l| !l.trim().is_empty()).collect();
            if lines.is_empty() {
                println!(
                    "  {} {} — repositorio Git inicializado (árbol limpio, sin commits aún).
",
                    paint("✓", GREEN),
                    paint(&project_name, BOLD)
                );
            } else {
                println!(
                    "  {} {} — repositorio Git inicializado ({} archivo(s) pendientes de commit inicial):",
                    paint("●", CYAN),
                    paint(&project_name, BOLD),
                    lines.len()
                );
                for line in &lines {
                    println!("    {}", paint(line.trim(), DIM));
                }
                println!();
            }
        }
        return;
    }

    let mut git_cmd = std::process::Command::new("git");
    git_cmd.current_dir(proj).args(&["diff", target]);
    if !ceiling_val.is_empty() {
        git_cmd.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
    }

    match git_cmd.output() {
        Ok(out) if out.status.success() => {
            let diff_str = String::from_utf8_lossy(&out.stdout);
            let files = diff_view::DiffEngine::parse_unified_diff(&diff_str);
            if files.is_empty() {
                println!(
                    "  {} {} — sin cambios pendientes contra «{}».\n",
                    paint("✓", GREEN),
                    paint(&project_name, BOLD),
                    target
                );
            } else {
                println!(
                    "  {} {} — {} archivo(s) modificado(s):",
                    paint("~", CYAN),
                    paint(&project_name, BOLD),
                    files.len()
                );
                let rendered = diff_view::DiffEngine::render_terminal(&files);
                print!("{rendered}");
            }
        }
        _ => {
            println!(
                "  {} No se pudo ejecutar git diff en «{}».\n",
                paint("✗", RED),
                proj.display()
            );
        }
    }
}


// ----------------------------------------------------------- project / git (T17.3)

fn cmd_project(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");

    match sub {
        "init" => cmd_project_init(ctx, &args[1..]),
        "list" | "ls" => cmd_project_list(ctx),
        "use" => cmd_use(ctx, &args[1..]),
        "current" => cmd_use(ctx, &[]),
        _ => {
            println!(
                "\n{}\n",
                paint("antOS · Gestión de Proyectos en Workspace (T17.3)", BOLD)
            );
            println!("  Uso:");
            println!("    antos use <nombre>            Fija el proyecto activo en el workspace");
            println!("    antos use --clear             Limpia la selección del proyecto activo");
            println!("    antos project init <nombre> [--branch <rama>] [--lang <lenguaje>]");
            println!("    antos project list            Lista proyectos en workspace/");
            println!("    antos git init [nombre]");
            println!();
            Ok(())
        }
    }
}

fn cmd_use(ctx: &Ctx, args: &[String]) -> Result<()> {
    let active_file = ctx.state.join("active_project");

    if let Some(target) = args.first() {
        if target == "--clear" || target == "clear" || target == "none" || target == "system" {
            if active_file.exists() {
                let _ = std::fs::remove_file(&active_file);
            }
            println!(
                "\n{} Selección de proyecto restablecida. antOS operará en ámbito global / automático.\n",
                paint("antOS ·", BOLD)
            );
            return Ok(());
        }

        // Validar si el proyecto existe en workspace
        let project_dir = ctx.workspace.join(target);
        if !project_dir.exists() || !project_dir.is_dir() {
            println!(
                "\n{} El proyecto '{}' no existe en {}",
                paint("antOS Error ·", RED),
                paint(target, YELLOW),
                ctx.workspace.display()
            );
            let projects = crate::exec::scan_workspace_projects(&ctx.workspace);
            if !projects.is_empty() {
                println!("\n  Proyectos disponibles en el workspace:");
                for p in projects {
                    let pname = p.file_name().and_then(|n| n.to_str()).unwrap_or("proyecto");
                    println!("    • {}", paint(pname, CYAN));
                }
            } else {
                println!("  (no hay proyectos creados aún en workspace/)");
            }
            println!(
                "\n  Puedes inicializarlo con: {}\n",
                paint(&format!("antos project init {}", target), GREEN)
            );
            return Ok(());
        }

        // Guardar proyecto activo en state
        std::fs::create_dir_all(&ctx.state)?;
        std::fs::write(&active_file, target.trim())?;

        println!(
            "\n{} Proyecto activo fijado en: {}\n  Directorio: {}\n  Todos los comandos de antOS (tickets, git, agentes, panel, etc.) operarán sobre este proyecto por defecto.\n",
            paint("antOS ·", BOLD),
            paint(target, GREEN),
            project_dir.display()
        );
    } else {
        // Mostrar proyecto activo actual
        println!(
            "\n{} Estado del Proyecto Activo en Workspace:",
            paint("antOS ·", BOLD)
        );
        if let Some(ref cur) = ctx.current_project {
            let name = cur.file_name().and_then(|n| n.to_str()).unwrap_or("desconocido");
            println!("  • Proyecto seleccionado: {}", paint(name, GREEN));
            println!("  • Ruta en disco:         {}", cur.display());
            if let Ok(active_name) = std::fs::read_to_string(&active_file) {
                if active_name.trim() == name {
                    println!("  • Origen:                Configurado persistentemente vía 'antos use'");
                } else {
                    println!("  • Origen:                Detectado automáticamente por directorio actual (CWD)");
                }
            } else {
                println!("  • Origen:                Detectado automáticamente por directorio actual (CWD)");
            }
        } else {
            println!("  • Proyecto seleccionado: {}", paint("(ninguno / ámbito del sistema)", YELLOW));
            println!("  • Espacio de trabajo:    {}", ctx.workspace.display());
        }
        println!("\n  Uso:");
        println!("    antos use <nombre-proyecto>   Selecciona el proyecto activo para todos los comandos");
        println!("    antos use --clear             Limpia la selección activa (vuelve a detección automática)");
        println!("    antos project list            Lista todos los proyectos en workspace/\n");
    }
    Ok(())
}

fn cmd_git(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "init" => cmd_project_init(ctx, &args[1..]),
        "diff" => cmd_diff(ctx, &args[1..]),
        "status" | "st" => {
            let target_dir = ctx.current_project.as_deref().unwrap_or(&ctx.workspace);
            let proj_name = target_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| target_dir.display().to_string());

            println!(
                "\n{} {}",
                paint("antOS Git · Estado de", BOLD),
                paint(&proj_name, CYAN)
            );
            println!("  Directorio: {}", paint(&target_dir.display().to_string(), DIM));

            let antos_root = ctx.antos_root.as_deref();
            if git::find_git_root_with_ceiling(target_dir, antos_root).is_none() {
                println!(
                    "  {} No es un repositorio Git propio.\n  Inicialízalo con: {}\n",
                    paint("⚠", YELLOW),
                    paint(&format!("antos project init {}", proj_name), CYAN)
                );
                return Ok(());
            }

            if let Some(status) = crate::git::GitAnalyzer::global().consultar_estado(target_dir)? {
                let branch_str = status.branch.unwrap_or_else(|| "HEAD desacoplado".into());
                println!("  Rama activa:    {}", paint(&branch_str, GREEN));
                println!("  Sincronización: +{} / -{}", status.ahead, status.behind);
                println!("  Modificados:    {}", status.modified.len());
                println!("  Staged:         {}", status.staged.len());
                println!("  Sin seguimiento: {}\n", status.untracked.len());
            } else {
                println!("  (no se pudo determinar el estado)\n");
            }
            Ok(())
        }
        _ => {
            println!(
                "\n{}\n",
                paint("antOS · Integración Git Aislada (T17.3)", BOLD)
            );
            println!("  Subcomandos:");
            println!("    antos git init [nombre]");
            println!("    antos git status");
            println!("    antos git diff [ref]");
            println!();
            Ok(())
        }
    }
}

fn cmd_project_init(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut project_name: Option<String> = None;
    let mut branch = "main".to_string();
    let mut language_hint: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-b" | "--branch" => {
                if let Some(b) = args.get(i + 1) {
                    branch = b.clone();
                    i += 1;
                }
            }
            "-l" | "--lang" | "--language" => {
                if let Some(l) = args.get(i + 1) {
                    language_hint = Some(l.clone());
                    i += 1;
                }
            }
            other if !other.starts_with('-') && project_name.is_none() => {
                project_name = Some(other.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let project_dir = if let Some(ref name) = project_name {
        let candidate = std::path::Path::new(name);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            ctx.workspace.join(name)
        }
    } else if let Some(ref current) = ctx.current_project {
        current.clone()
    } else {
        bail!(
            "Especifica el nombre del proyecto a inicializar:\n    antos project init <nombre>\n    antos git init <nombre>"
        );
    };

    println!(
        "\n{}",
        paint("antOS · Inicialización Declarativa de Proyecto Git (T17.3)", BOLD)
    );
    println!(
        "  Espacio de trabajo: {}",
        paint(&ctx.workspace.display().to_string(), DIM)
    );

    let display_name = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project_dir.display().to_string());

    println!("  Proyecto:           {}", paint(&display_name, CYAN));
    println!("  Ruta física:        {}", paint(&project_dir.display().to_string(), DIM));
    println!("  Rama principal:     {}", paint(&branch, GREEN));

    let res = crate::exec::init_project_git_repo(&project_dir, &branch, language_hint.as_deref())?;

    println!("\n  {} {}\n", paint("✓", GREEN), res);
    println!("  Para inspeccionar los cambios del proyecto, ejecuta:");
    println!(
        "      {}\n",
        paint(&format!("antos diff {}", display_name), DIM)
    );

    Ok(())
}

fn cmd_project_list(ctx: &Ctx) -> Result<()> {
    println!(
        "\n{}",
        paint("antOS · Proyectos en Espacio de Trabajo (T17.3)", BOLD)
    );
    println!(
        "  Espacio de trabajo: {}\n",
        paint(&ctx.workspace.display().to_string(), DIM)
    );

    let projects = crate::exec::scan_workspace_projects(&ctx.workspace);
    if projects.is_empty() {
        println!("  (no se encontraron proyectos en workspace/)\n");
        println!(
            "  Crea uno con: {}\n",
            paint("antos project init <nombre>", CYAN)
        );
        return Ok(());
    }

    let antos_root = ctx.antos_root.as_deref();

    for proj in &projects {
        let name = proj
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| proj.display().to_string());

        let has_git = git::find_git_root_with_ceiling(proj, antos_root).is_some();
        let lang = crate::exec::detect_project_language(proj);
        let file_count = crate::exec::collect_project_files(proj, 100).len();

        let git_badge = if has_git {
            paint("● Git activo", GREEN)
        } else {
            paint("○ Sin Git", YELLOW)
        };

        let is_active = ctx.current_project.as_ref() == Some(proj);
        let active_badge = if is_active {
            format!(" {}", paint("[ACTIVO]", GREEN))
        } else {
            String::new()
        };

        println!(
            "  • {}{}  [{}]  (stack: {}, {} archivos)",
            paint(&name, BOLD),
            active_badge,
            git_badge,
            paint(&lang, CYAN),
            file_count
        );
    }
    println!();
    println!("  Para fijar el proyecto activo en todos los comandos:");
    println!("    {}\n", paint("antos use <nombre>", CYAN));

    Ok(())
}

// ------------------------------------------------------------------ terminal / vte

fn cmd_terminal(args: &[String]) -> Result<()> {
    let mut session = vte::TerminalSession::new("vte-cli");
    println!(
        "\n{}",
        paint("antOS · Consola Terminal VTE Embebida (T8.1)", BOLD)
    );
    println!(
        "  Shell interactivo:  {}\n",
        paint(&session.active_shell, GREEN)
    );

    if args.is_empty() {
        println!("  Consola terminal interactiva lista. Para ejecutar comandos usa:");
        println!("    antos terminal \"<comando>\"\n");
    } else {
        let cmd = args.join(" ");
        session.execute_command(&cmd)?;
        for line in &session.buffer {
            println!("{}", line.raw);
        }
        println!();
    }

    Ok(())
}

// ------------------------------------------------------------------ notify / approvals

fn cmd_notify(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str);
    let engine = notification::NotificationEngine::global();

    match sub {
        Some("approve" | "aprobar") => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos notify approve <ID>"))?;
            let (ok, msg) = engine.handle_action(
                &ctx.workspace,
                id,
                antos_protocolo::NotificationAction::Approve,
            )?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("reject" | "rechazar" | "rollback") => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos notify reject <ID>"))?;
            let (ok, msg) = engine.handle_action(
                &ctx.workspace,
                id,
                antos_protocolo::NotificationAction::Reject,
            )?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("dismiss" | "descartar") => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos notify dismiss <ID>"))?;
            let (ok, msg) = engine.handle_action(
                &ctx.workspace,
                id,
                antos_protocolo::NotificationAction::Dismiss,
            )?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("clear" | "limpiar") => {
            let count = engine.clear(&ctx.workspace)?;
            println!(
                "\n{} Se limpiaron {} notificaciones leídas.\n",
                paint("✓", GREEN),
                count
            );
        }
        _ => {
            let list = engine.list(&ctx.workspace)?;
            println!(
                "\n{}",
                paint(
                    "antOS · Bandeja de Notificaciones y Aprobaciones Asíncronas (T8.2)",
                    BOLD
                )
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            if list.is_empty() {
                println!(
                    "  {} No hay notificaciones ni aprobaciones pendientes.\n",
                    paint("✓ Bandeja al día:", GREEN)
                );
                println!(
                    "  Los agentes multi-agente antFlow notificarán aquí cuando completen tareas."
                );
                println!("  Comandos: antos notify approve <ID> | antos notify reject <ID>\n");
            } else {
                for n in &list {
                    let mark = if n.read {
                        paint("○ leída", DIM)
                    } else {
                        paint("● NUEVA", YELLOW)
                    };
                    let kind_badge = match n.kind {
                        antos_protocolo::NotificationKind::ApprovalRequired => {
                            paint("⚠️ APROBACIÓN REQUERIDA", YELLOW)
                        }
                        antos_protocolo::NotificationKind::TaskFinished => {
                            paint("✓ TAREA COMPLETADA", GREEN)
                        }
                        antos_protocolo::NotificationKind::QAFailed => paint("✗ QA FALLIDO", RED),
                        antos_protocolo::NotificationKind::SecurityAlert => {
                            paint("🛡️ ALERTA SEGURIDAD", RED)
                        }
                        antos_protocolo::NotificationKind::System => paint("ℹ️ SISTEMA", CYAN),
                    };

                    println!(
                        "  {} [{}] {} — {}",
                        mark,
                        paint(&n.id, BOLD),
                        kind_badge,
                        paint(&n.ticket_id, BOLD)
                    );
                    println!("     {}: {}", paint("Título", DIM), n.title);
                    println!("     {}: {}\n", paint("Detalle", DIM), n.body);
                }
                println!("  Usa «antos notify approve <ID>» para autorizar o «antos notify reject <ID>» para rollback.\n");
            }
        }
    }

    Ok(())
}

// --------------------------------------------------------------------- mesh

fn cmd_mesh(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = mesh::MeshEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("connect" | "conectar" | "add") => {
            let addr = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos mesh connect <IP:PUERTO|MULTIADDR>"))?;
            let peer = engine.connect_peer(&ctx.workspace, addr)?;
            println!(
                "\n{} Conectado al nodo peer en la malla antMesh.",
                paint("✓", GREEN)
            );
            println!("  • Nodo ID:    {}", paint(&peer.id, BOLD));
            println!("  • Hostname:   {}", paint(&peer.hostname, BOLD));
            println!("  • Dirección:  {}", paint(&peer.address, YELLOW));
            println!(
                "  • Latencia:   {} ms",
                paint(&peer.latency_ms.to_string(), GREEN)
            );
            println!(
                "  • Recursos:   {} cores CPU, {} MB RAM, VRAM: {:?}",
                peer.resources.cpu_cores, peer.resources.memory_mb, peer.resources.vram_mb
            );
            println!(
                "  • Modelos:    {}\n",
                peer.resources.available_models.join(", ")
            );
        }
        Some("pair" | "token" | "emparejar") => {
            let token = engine.generate_pairing_token(&ctx.workspace)?;
            let expires_mins = (token.expires_at.saturating_sub(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            ) / 60)
                .max(1);

            println!(
                "\n{}",
                paint("antOS · Token de Emparejamiento antMesh (T9.1)", BOLD)
            );
            println!("  Token de enlace:    {}", paint(&token.token, GREEN));
            println!("  Identidad del nodo: {}", paint(&token.node_id, BOLD));
            println!(
                "  Válido durante:     {} minutos",
                paint(&expires_mins.to_string(), YELLOW)
            );
            println!(
                "\n  Usa «{}» en el nodo remoto para unirte a este clúster.\n",
                paint(&format!("antos mesh connect <ESTA_IP>:9042"), BOLD)
            );
        }
        Some("status" | "list") | _ => {
            let status = engine.status(&ctx.workspace)?;
            println!(
                "\n{}",
                paint("antOS · Red P2P Cifrada antMesh (T9.1)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            println!("  {}", paint("● NODO LOCAL", BOLD));
            println!(
                "    ID criptográfico: {}",
                paint(&status.local_node.id, BOLD)
            );
            println!(
                "    Hostname:         {}",
                paint(&status.local_node.hostname, BOLD)
            );
            println!(
                "    Dirección escucha:{}",
                paint(&status.local_node.address, YELLOW)
            );
            println!(
                "    Capacidades:      {} cores CPU · {} MB RAM · VRAM: {:?}",
                status.local_node.resources.cpu_cores,
                status.local_node.resources.memory_mb,
                status.local_node.resources.vram_mb
            );
            println!(
                "    Modelos locales:  {}\n",
                status.local_node.resources.available_models.join(", ")
            );

            println!("  {}", paint("● PEERS CONECTADOS EN LA MALLA", BOLD));
            if status.peers.is_empty() {
                println!("    {} No hay nodos vecinos conectados.", paint("○", DIM));
                println!("    Usa «antos mesh pair» para generar un token de invitación.");
                println!("    Usa «antos mesh connect <IP:9042>» para vincular un nodo remoto.\n");
            } else {
                for p in &status.peers {
                    let conn_mark = if p.connected {
                        paint("● conectado", GREEN)
                    } else {
                        paint("○ desconectado", DIM)
                    };
                    println!(
                        "    {} [{}] {} · {} ({} ms)",
                        conn_mark,
                        paint(&p.id, BOLD),
                        paint(&p.hostname, BOLD),
                        paint(&p.address, YELLOW),
                        paint(&p.latency_ms.to_string(), GREEN)
                    );
                    println!(
                        "       Recursos: {} cores, {} MB RAM, modelos: {}",
                        p.resources.cpu_cores,
                        p.resources.memory_mb,
                        p.resources.available_models.join(", ")
                    );
                }
                println!();
            }
        }
    }

    Ok(())
}

// -------------------------------------------------------------------- swarm

fn cmd_swarm(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = distributed::SwarmEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("dispatch" | "despacha") => {
            let ticket_id = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("uso: antos swarm dispatch <TID> [--role <coder|qa>] [--node <ID>]")
            })?;
            let role = if args.iter().any(|a| a == "--qa") {
                antos_protocolo::AgentRole::QA
            } else {
                antos_protocolo::AgentRole::Coder
            };
            let node_target = args
                .iter()
                .position(|a| a == "--node" || a == "-n")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str);
            let task = engine.dispatch_remote_role(&ctx.workspace, ticket_id, role, node_target)?;
            println!(
                "\n{} Tarea distribuida despachada al Swarm.",
                paint("✓", GREEN)
            );
            println!("  • Tarea ID:   {}", paint(&task.task_id, BOLD));
            println!("  • Ticket:     {}", paint(&task.ticket_id, YELLOW));
            println!("  • Rol:        {}", paint(task.role.nombre(), BOLD));
            println!("  • Nodo:       {}", paint(&task.assigned_node_id, GREEN));
            println!("  • Rama:       {}", paint(&task.worktree_branch, DIM));
            if let Some(m) = task.target_model {
                println!("  • Modelo LLM: {}", paint(&m, CYAN));
            }
            println!();
        }
        _ => {
            let status = engine.status(&ctx.workspace)?;
            println!(
                "\n{}",
                paint("antOS · Centro de Control Swarm Multi-Nodo (T9.2)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            println!(
                "  Nodos en el clúster: {} · Tareas activas: {}\n",
                paint(&status.nodes.len().to_string(), BOLD),
                paint(&status.total_tasks.to_string(), GREEN)
            );

            for n in &status.nodes {
                let badge = if n.is_local {
                    paint("● LOCAL", GREEN)
                } else {
                    paint("🌐 REMOTO", CYAN)
                };
                let vram_str = n
                    .vram_available_mb
                    .map(|v| format!("{} MB VRAM", v))
                    .unwrap_or_else(|| "N/A".into());
                println!(
                    "  {} [{}] {} · {} ({} CPUs · {})",
                    badge,
                    paint(&n.node_id, BOLD),
                    paint(&n.hostname, BOLD),
                    paint(&n.address, YELLOW),
                    n.cpu_cores,
                    vram_str
                );

                if n.running_tasks.is_empty() {
                    println!("     {} Sin tareas en ejecución.", paint("○", DIM));
                } else {
                    for t in &n.running_tasks {
                        println!(
                            "     └─ Tarea {}: rol {:?} en rama {} [{}]",
                            paint(&t.task_id, BOLD),
                            t.role,
                            paint(&t.worktree_branch, DIM),
                            paint(&t.status, YELLOW)
                        );
                    }
                }
                println!();
            }
            println!("  Usa «antos swarm dispatch <TID> [--node <ID>]» para delegar trabajo a un nodo.\n");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------- vfs

fn cmd_vfs(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = vfs::VfsEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("mount" | "monta") => {
            let target = args.get(1).map(String::as_str);
            let path = engine.mount(&ctx.workspace, target)?;
            println!(
                "\n{} Sistema de ficheros semántico /antfs montado con éxito.",
                paint("✓", GREEN)
            );
            println!(
                "  • Punto de montaje: {}",
                paint(&path.display().to_string(), BOLD)
            );
            println!(
                "  • Inspección:       {} o {}\n",
                paint(&format!("ls {}", path.display()), YELLOW),
                paint(&format!("cat {}/README.antfs", path.display()), YELLOW)
            );
        }
        Some("unmount" | "umount" | "desmonta") => {
            let target = args.get(1).map(String::as_str);
            engine.unmount(&ctx.workspace, target)?;
            println!(
                "\n{} Sistema de ficheros semántico /antfs desmontado correctamente.\n",
                paint("✓", GREEN)
            );
        }
        Some("ls" | "list") => {
            let vpath = args.get(1).map(String::as_str).unwrap_or("/antfs");
            let entries = engine.list_dir(&ctx.workspace, vpath)?;
            println!(
                "\n{} Listado de {}",
                paint("antOS VFS ·", BOLD),
                paint(vpath, YELLOW)
            );
            if entries.is_empty() {
                println!("  (directorio vacío)\n");
            } else {
                for e in entries {
                    let mark = if e.is_dir {
                        paint("📁", BLUE)
                    } else {
                        paint("📄", GREEN)
                    };
                    println!(
                        "  {} {:<26} {:<15} ({} bytes)",
                        mark,
                        paint(&e.name, BOLD),
                        paint(&e.node_type, DIM),
                        e.size
                    );
                }
                println!();
            }
        }
        Some("cat" | "read" | "lee") => {
            let vpath = args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos vfs read <ruta_virtual> (ej. /antfs/symbols/structs/MeshStatus)"
                )
            })?;
            let content = engine.read_path(&ctx.workspace, vpath)?;
            println!("\n{}\n", content);
        }
        Some("symbols" | "simbolos" | "símbolos") => {
            let symbols = engine.discover_symbols(&ctx.workspace)?;
            println!(
                "\n{} ({} descubiertos)\n",
                paint("antOS VFS · Símbolos Semánticos del Proyecto", BOLD),
                paint(&symbols.len().to_string(), GREEN)
            );
            for cat in &["structs", "functions", "enums", "traits"] {
                let cat_syms: Vec<_> = symbols.iter().filter(|s| &s.category == cat).collect();
                if !cat_syms.is_empty() {
                    println!(
                        "  {} {} ({}):",
                        paint("●", YELLOW),
                        paint(*cat, BOLD),
                        cat_syms.len()
                    );
                    for s in cat_syms {
                        println!(
                            "    • {:<28} {}:{}",
                            paint(&s.name, BOLD),
                            paint(&s.file_path, DIM),
                            s.line_number
                        );
                    }
                    println!();
                }
            }
        }
        Some("validate" | "check" | "valida") => {
            let rel = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("uso: antos vfs validate <archivo> (ej. src/main.rs)")
            })?;
            let abs_path = ctx.workspace.join(rel);
            if !abs_path.exists() {
                bail!(
                    "el archivo «{}» no existe en el espacio de trabajo",
                    abs_path.display()
                );
            }
            let text = std::fs::read_to_string(&abs_path)?;
            let guard = vfs_guard::VfsGuardEngine::global();
            let res = guard.validate_content(rel, &text);
            println!(
                "\n{} Validación de integridad sintáctica VFS",
                paint("antOS ·", BOLD)
            );
            println!("  Archivo:  {}", paint(rel, YELLOW));
            println!(
                "  Lenguaje: {} ({} líneas)\n",
                paint(&res.language, BOLD),
                res.line_count
            );
            if res.is_valid {
                println!(
                    "  {} El archivo es sintácticamente válido y seguro para persistir.\n",
                    paint("✓ Aprobado:", GREEN)
                );
            } else {
                println!(
                    "  {} Se detectaron {} problema(s) sintáctico(s):",
                    paint("✗ Rechazado:", RED),
                    res.errors.len()
                );
                for err in res.errors {
                    println!(
                        "    • Línea {}, columna {}: {}",
                        paint(&err.line.to_string(), YELLOW),
                        err.column,
                        err.message
                    );
                }
                println!();
            }
        }
        Some("guard" | "guardia" | "interceptor") => {
            let guard = vfs_guard::VfsGuardEngine::global();
            let status = guard.status()?;
            println!(
                "\n{}",
                paint(
                    "antOS VFS · Interceptor de Escrituras Semánticas (T10.2)",
                    BOLD
                )
            );
            let state_str = if status.enabled {
                paint("● ACTIVO (ENFORCING)", GREEN)
            } else {
                paint("○ INACTIVO", DIM)
            };
            println!("  Estado del interceptor:   {}", state_str);
            println!(
                "  Escrituras interceptadas: {}",
                paint(&status.total_intercepted.to_string(), BOLD)
            );
            println!(
                "  Escrituras rechazadas:    {}",
                paint(
                    &status.total_rejected.to_string(),
                    if status.total_rejected > 0 {
                        RED
                    } else {
                        GREEN
                    }
                )
            );
            if !status.rejected_paths.is_empty() {
                println!("\n  Ficheros protegidos contra corrupción sintáctica:");
                for p in status.rejected_paths {
                    println!("    • {}", paint(&p, YELLOW));
                }
            }
            println!("\n  Usa «antos vfs validate <archivo>» para probar validación previa.\n");
        }
        _ => {
            let status = engine.status(&ctx.workspace)?;
            println!(
                "\n{}",
                paint(
                    "antOS · Sistema de Ficheros Virtual FUSE (/antfs) - T10.1 & T10.2",
                    BOLD
                )
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            let mnt_badge = if status.is_mounted {
                paint("● MONTADO", GREEN)
            } else {
                paint("○ NO MONTADO", DIM)
            };
            println!("  Estado del VFS:     {}", mnt_badge);
            if let Some(mnt) = status.mount_point {
                println!("  Punto de montaje:   {}", paint(&mnt, YELLOW));
            }
            println!(
                "  Símbolos AST:       {} indexados",
                paint(&status.total_symbols.to_string(), BOLD)
            );
            println!(
                "  Módulos navegables: {} en /antfs/graph\n",
                paint(&status.total_modules.to_string(), BOLD)
            );

            println!("  Subcomandos disponibles:");
            println!("    • antos vfs symbols          Lista símbolos AST (structs, functions, enums, traits)");
            println!("    • antos vfs ls [ruta]        Explora la jerarquía /antfs (symbols, graph, git)");
            println!("    • antos vfs read <ruta>      Lee el código o diff de un inodo virtual");
            println!("    • antos vfs mount [ruta]     Proyecta /antfs en el disco local");
            println!("    • antos vfs unmount [ruta]   Desmonta la proyección /antfs");
            println!("    • antos vfs validate <file>  Valida la integridad sintáctica antes de persistir");
            println!(
                "    • antos vfs guard            Muestra métricas del interceptor de escrituras\n"
            );
        }
    }
    Ok(())
}

// --------------------------------------------------------------------- ebpf

fn cmd_ebpf(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = ebpf::EbpfSentinelEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("status" | "info" | "estado") => {
            let status = engine.status()?;
            println!(
                "\n{}",
                paint("antOS · Supervisor Kernel eBPF LSM (T11.1)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            let lsm_badge = if status.lsm_enabled {
                paint("● KERNEL LSM ACTIVO (BPF Enforcing)", GREEN)
            } else {
                paint("○ EMULACIÓN ESPACIO DE USUARIO (Auditoría activa)", YELLOW)
            };
            println!("  Estado del soporte eBPF: {}", lsm_badge);
            println!("  Sondas activas ({}):", status.active_probes.len());
            for probe in status.active_probes {
                println!("    • {}", paint(&probe, CYAN));
            }
            println!(
                "  Capacidad del ring buffer: {} entradas (uso: {})",
                status.ring_buffer_capacity, status.ring_buffer_utilization
            );
            println!(
                "  Eventos capturados:        {}",
                paint(&status.total_events_captured.to_string(), BOLD)
            );
            println!(
                "  Violaciones bloqueadas:    {}\n",
                paint(
                    &status.total_violations_blocked.to_string(),
                    if status.total_violations_blocked > 0 {
                        RED
                    } else {
                        GREEN
                    }
                )
            );
        }
        Some("trace" | "traza") => {
            let pid_opt = args.get(1).and_then(|p| p.parse::<u32>().ok());
            let events = match pid_opt {
                Some(pid) => engine.trace_pid(pid),
                None => engine.get_audit_log(25),
            };
            println!(
                "\n{} Traza en vivo de syscalls y eventos de seguridad",
                paint("antOS eBPF ·", BOLD)
            );
            if let Some(p) = pid_opt {
                println!("  Filtro por PID: {}\n", paint(&p.to_string(), YELLOW));
            } else {
                println!("  Mostrando los últimos {} eventos:\n", events.len());
            }

            if events.is_empty() {
                println!("  (no hay eventos en el ring buffer)\n");
            } else {
                for ev in events {
                    let mark = match ev.action_taken {
                        antos_protocolo::EbpfSecurityAction::Allowed => paint("✓ ALLOW", GREEN),
                        antos_protocolo::EbpfSecurityAction::Blocked => paint("⛔ BLOCK", RED),
                        antos_protocolo::EbpfSecurityAction::Audited => paint("👁 AUDIT", YELLOW),
                    };
                    println!(
                        "  {} [{}] PID {}:{} ➔ {} ({:?})",
                        mark,
                        paint(&ev.id, DIM),
                        ev.pid,
                        paint(&ev.comm, BOLD),
                        paint(&ev.target_resource, YELLOW),
                        ev.hook
                    );
                    if let Some(ref r) = ev.violation_reason {
                        println!("      └─ {}", paint(r, DIM));
                    }
                }
                println!();
            }
        }
        Some("audit" | "log" | "registro") => {
            let limit = args
                .get(1)
                .and_then(|l| l.parse::<usize>().ok())
                .unwrap_or(20);
            let events = engine.get_audit_log(limit);
            println!(
                "\n{} Registro de auditoría eBPF (últimos {} eventos)\n",
                paint("antOS eBPF ·", BOLD),
                events.len()
            );
            if events.is_empty() {
                println!("  (registro vacío)\n");
            } else {
                for ev in events {
                    let mark = match ev.action_taken {
                        antos_protocolo::EbpfSecurityAction::Allowed => paint("✓", GREEN),
                        antos_protocolo::EbpfSecurityAction::Blocked => paint("⛔", RED),
                        antos_protocolo::EbpfSecurityAction::Audited => paint("👁", YELLOW),
                    };
                    println!(
                        "  {} [{}] {:<18} PID {}:{} ➔ {}",
                        mark,
                        paint(&ev.id, DIM),
                        format!("{:?}", ev.hook),
                        ev.pid,
                        paint(&ev.comm, BOLD),
                        ev.target_resource
                    );
                }
                println!();
            }
        }
        Some("simulate" | "simula" | "test") => {
            let kind_str = args.get(1).map(String::as_str).unwrap_or("file");
            let hook = match kind_str {
                "socket" | "net" | "red" => antos_protocolo::EbpfHookKind::SocketConnect,
                "bprm" | "exec" => antos_protocolo::EbpfHookKind::BprmCheckSecurity,
                "syscall" => antos_protocolo::EbpfHookKind::SyscallTrace,
                _ => antos_protocolo::EbpfHookKind::FileOpen,
            };
            let target = args
                .get(2)
                .map(String::as_str)
                .unwrap_or_else(|| match hook {
                    antos_protocolo::EbpfHookKind::SocketConnect => "192.168.1.50:4444",
                    antos_protocolo::EbpfHookKind::FileOpen => "/etc/shadow",
                    antos_protocolo::EbpfHookKind::BprmCheckSecurity => "/bin/nc",
                    antos_protocolo::EbpfHookKind::SyscallTrace => "ptrace",
                });

            let ev = engine.simulate_violation(hook, target);
            println!(
                "\n{} Simulación de intento de evasión de sandbox",
                paint("antOS eBPF ·", BOLD)
            );
            println!("  Hook interceptado: {:?}", ev.hook);
            println!(
                "  Recurso objetivo:  {}",
                paint(&ev.target_resource, YELLOW)
            );
            println!("  Acción del kernel: {}", paint("⛔ BLOQUEADO", RED));
            println!(
                "  Alerta disparada:  {} Se envió notificación prioritaria a la bandeja Wayland.\n",
                paint("✓", GREEN)
            );
        }
        _ => {
            let status = engine.status()?;
            println!(
                "\n{}",
                paint("antOS · Supervisor Kernel eBPF LSM (T11.1)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            let lsm_badge = if status.lsm_enabled {
                paint("● KERNEL LSM ACTIVO", GREEN)
            } else {
                paint("○ EMULACIÓN ESPACIO USUARIO", YELLOW)
            };
            println!("  Soporte:            {}", lsm_badge);
            println!("  Sondas activas:     {}", status.active_probes.len());
            println!(
                "  Eventos capturados: {}",
                paint(&status.total_events_captured.to_string(), BOLD)
            );
            println!(
                "  Bloqueos evasión:   {}\n",
                paint(
                    &status.total_violations_blocked.to_string(),
                    if status.total_violations_blocked > 0 {
                        RED
                    } else {
                        GREEN
                    }
                )
            );

            println!("  Subcomandos disponibles:");
            println!(
                "    • antos ebpf status            Diagnóstico de sondas y soporte de kernel"
            );
            println!(
                "    • antos ebpf trace [pid]       Traza de llamadas al sistema en tiempo real"
            );
            println!("    • antos ebpf audit [limit]     Registro de auditoría del ring buffer");
            println!(
                "    • antos ebpf simulate <tipo>   Simula evasión (file|socket|bprm) y alerta\n"
            );
        }
    }
    Ok(())
}

// ------------------------------------------------------------------- profile

fn cmd_profile(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = profiler::ProfilerEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("run" | "ejecutar") => {
            let command = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                "cargo test".to_string()
            };
            println!(
                "\n{} Ejecutando perfilado continuo para: {}",
                paint("antOS Profiler ·", BOLD),
                paint(&command, YELLOW)
            );
            let report = engine.run_and_profile(&ctx.workspace, &command)?;

            let peak_mb = report.peak_memory_bytes as f64 / (1024.0 * 1024.0);
            let status_badge = if report.exit_code == 0 {
                paint("EXIT 0 (Éxito)", GREEN)
            } else {
                paint(&format!("EXIT {}", report.exit_code), RED)
            };

            println!("\n{}", paint("Resultado del Perfilado:", BOLD));
            println!("  Estado del comando:       {}", status_badge);
            println!(
                "  Duración de Wall-Clock:   {} ms",
                paint(&report.duration_ms.to_string(), BOLD)
            );
            println!(
                "  Tiempo de CPU:            {} ms usuario, {} ms sistema",
                report.cpu_user_ms, report.cpu_sys_ms
            );
            println!(
                "  Memoria Pico (RSS):       {} MB",
                paint(&format!("{peak_mb:.2}"), CYAN)
            );
            println!("  Fallas de Página (Faults): {}\n", report.page_faults);

            if !report.hotspots.is_empty() {
                println!(
                    "{}",
                    paint("  Puntos Calientes de Ejecución (Hotspots):", BOLD)
                );
                for h in report.hotspots {
                    println!(
                        "    • {:<32} CPU: {:>4.1}% | Mem: {:>4.1}% ({} muestras)",
                        paint(&h.name, YELLOW),
                        h.percentage_cpu,
                        h.percentage_memory,
                        h.calls_or_samples
                    );
                }
                println!();
            }

            if !report.suggestions.is_empty() {
                println!(
                    "{}",
                    paint(
                        "  Recomendaciones de Optimización para Agentes Coder / QA:",
                        BOLD
                    )
                );
                for s in report.suggestions {
                    let impact_color = if s.potential_impact.contains("Alto") {
                        RED
                    } else {
                        YELLOW
                    };
                    println!(
                        "    ★ [{}] {}",
                        paint(&s.potential_impact, impact_color),
                        paint(&s.title, BOLD)
                    );
                    println!("      └─ {}", paint(&s.description, DIM));
                    if let Some(target) = s.target_symbol_or_path {
                        println!("         Objetivo: {}", paint(&target, CYAN));
                    }
                }
                println!();
            }
        }
        Some("top" | "hotspots" | "cuellos") => {
            let (hotspots, _) = engine.analyze_aggregate(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS Profiler · Top Cuellos de Botella (Hotspots)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            if hotspots.is_empty() {
                println!(
                    "  (no hay hotspots registrados; ejecuta «antos profile run <comando>»)\n"
                );
            } else {
                for (idx, h) in hotspots.iter().enumerate() {
                    println!(
                        "  {}. {:<32} CPU: {:>5.1}% | Mem: {:>5.1}% ({} llamadas)",
                        idx + 1,
                        paint(&h.name, YELLOW),
                        h.percentage_cpu,
                        h.percentage_memory,
                        h.calls_or_samples
                    );
                }
                println!();
            }
        }
        Some("analyze" | "analiza" | "sugerencias") => {
            let (hotspots, suggestions) = engine.analyze_aggregate(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS Profiler · Análisis y Recomendaciones Técnicas", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            if suggestions.is_empty() {
                println!(
                    "  (sin recomendaciones activas; ejecuta «antos profile run <comando>»)\n"
                );
            } else {
                println!("  Puntos calientes consolidados: {}\n", hotspots.len());
                for s in suggestions {
                    let impact_color = if s.potential_impact.contains("Alto") {
                        RED
                    } else {
                        YELLOW
                    };
                    println!(
                        "  ★ [{}] {}",
                        paint(&s.potential_impact, impact_color),
                        paint(&s.title, BOLD)
                    );
                    println!("    └─ {}", paint(&s.description, DIM));
                }
                println!();
            }
        }
        Some("list" | "reports" | "reportes" | "historial") => {
            let reports = engine.load_reports(&ctx.workspace);
            println!(
                "\n{}",
                paint(
                    "antOS Profiler · Histórico de Reportes de Rendimiento",
                    BOLD
                )
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            if reports.is_empty() {
                println!("  (no hay reportes guardados)\n");
            } else {
                for r in reports {
                    let peak_mb = r.peak_memory_bytes as f64 / (1024.0 * 1024.0);
                    println!(
                        "  • [{}] «{}» — {} ms | {:.1} MB RSS (código {})",
                        paint(&r.id, DIM),
                        paint(&r.command, BOLD),
                        r.duration_ms,
                        peak_mb,
                        r.exit_code
                    );
                }
                println!();
            }
        }
        _ => {
            let reports = engine.load_reports(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS · Profiler Continuo de Runtime (T11.2)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );
            println!(
                "  Reportes registrados:   {}",
                paint(&reports.len().to_string(), BOLD)
            );

            println!("  Subcomandos disponibles:");
            println!(
                "    • antos profile run <cmd>      Ejecuta y perfila un comando en tiempo real"
            );
            println!("    • antos profile top            Lista los principales puntos calientes (hotspots)");
            println!(
                "    • antos profile analyze        Sintetiza recomendaciones para Coder y QA"
            );
            println!(
                "    • antos profile list           Muestra el histórico de reportes guardados\n"
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------- lsp

fn cmd_lsp(ctx: &Ctx, args: &[String]) -> Result<()> {
    let server = lsp::LspServer::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("status" | "info") => {
            let status = server.get_status(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS · Unified Language Server Protocol (LSP)", BOLD)
            );
            println!(
                "  Espacio de trabajo:     {}",
                paint(&ctx.workspace.display().to_string(), DIM)
            );
            println!(
                "  Estado del servidor:    {}",
                if status.running {
                    paint("Activo", GREEN)
                } else {
                    paint("En espera", YELLOW)
                }
            );
            println!(
                "  Transporte:             {}",
                paint(&status.transport, CYAN)
            );
            println!("  Clientes conectados:    {}", status.connected_clients);
            println!(
                "  Símbolos AST indexados: {}",
                paint(&status.indexed_symbols_count.to_string(), BOLD)
            );
            println!(
                "  Capacidades LSP:        {}\n",
                paint(&status.capabilities.join(", "), DIM)
            );
        }
        Some("config" | "conf") => {
            let editor_str = args.get(1).map(String::as_str).unwrap_or("vscode");
            let editor_kind = match editor_str.to_lowercase().as_str() {
                "vscode" | "code" => antos_protocolo::LspEditorKind::VsCode,
                "neovim" | "nvim" | "vim" => antos_protocolo::LspEditorKind::Neovim,
                "helix" | "hx" => antos_protocolo::LspEditorKind::Helix,
                "emacs" => antos_protocolo::LspEditorKind::Emacs,
                _ => antos_protocolo::LspEditorKind::Generic,
            };

            let (snippet, target_file) = server.generate_config(editor_kind, &ctx.workspace);
            println!(
                "\n{} Configuración de antOS LSP para: {}",
                paint("antOS LSP ·", BOLD),
                paint(editor_kind.name(), YELLOW)
            );
            println!(
                "  Archivo de configuración: {}\n",
                paint(&target_file, CYAN)
            );
            println!("{}\n", snippet);
        }
        Some("stdio" | "run" | "start") => {
            server.run_stdio(&ctx.workspace)?;
        }
        None => {
            server.run_stdio(&ctx.workspace)?;
        }
        _ => {
            let status = server.get_status(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS · Unified Language Server Protocol (LSP)", BOLD)
            );
            println!(
                "  Espacio de trabajo:     {}",
                paint(&ctx.workspace.display().to_string(), DIM)
            );
            println!(
                "  Símbolos AST indexados: {}\n",
                paint(&status.indexed_symbols_count.to_string(), BOLD)
            );
            println!("  Subcomandos disponibles:");
            println!(
                "    • antos lsp [stdio]            Inicia el servidor JSON-RPC 2.0 sobre stdio"
            );
            println!("    • antos lsp status             Diagnostica el estado del servidor y conexiones");
            println!("    • antos lsp config <editor>    Genera configuración para vscode, neovim, helix, emacs\n");
        }
    }
    Ok(())
}

// --------------------------------------------------------------------- pair

fn cmd_pair(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = collab::CollabEngine::global();
    let first = args.first().map(String::as_str);

    let (ticket_id, file_path) = match first {
        Some(arg)
            if arg.starts_with('T')
                && arg
                    .chars()
                    .nth(1)
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false) =>
        {
            let file = args.get(1).map(String::as_str).unwrap_or("src/main.rs");
            (Some(arg.to_string()), file.to_string())
        }
        Some(file) => (None, file.to_string()),
        None => (None, "src/main.rs".to_string()),
    };

    println!(
        "\n{} Iniciando sesión interactiva de Pair Programming con Coder...",
        paint("antOS Pair ·", BOLD)
    );
    let status = engine.start_session(&ctx.workspace, &file_path, ticket_id)?;

    println!("\n{}", paint("Sesión de Co-Edición Activa:", BOLD));
    println!(
        "  ID de Sesión:          {}",
        paint(&status.session_id, CYAN)
    );
    println!(
        "  Archivo compartido:    {}",
        paint(&status.file_path, YELLOW)
    );
    println!(
        "  Colaboradores:         {}",
        paint(&status.collaborators.join(" & "), GREEN)
    );
    if let Some(ref t) = status.active_ticket_id {
        println!("  Ticket vinculado:      {}", paint(t, BOLD));
    }
    println!(
        "  Tamaño del buffer:     {} caracteres\n",
        status.buffer_length
    );

    println!(
        "{}",
        paint("Cursores y Sugerencias de Código (Ghost Text):", BOLD)
    );
    for c in &status.cursors {
        println!(
            "  • [{}] Línea {}, Columna {}",
            paint(&c.client_id, BOLD),
            c.line,
            c.character
        );
        if let Some(ref ghost) = c.ghost_text {
            println!("      └─ Ghost text sugerido: {}", paint(ghost, DIM));
        }
    }
    println!("\n  Consejo: Presiona <Tab> en tu editor o ejecuta «antos intent acepta el ghost text» para fusionar.\n");

    Ok(())
}

// -------------------------------------------------------------------- debug

fn cmd_debug(_ctx: &Ctx, args: &[String]) -> Result<()> {
    let command = if !args.is_empty() {
        args.join(" ")
    } else {
        "cargo test".to_string()
    };

    println!(
        "\n{} Conectando adaptador de depuración DAP para: {}",
        paint("antOS DAP Debugger ·", BOLD),
        paint(&command, YELLOW)
    );
    let mut dap = collab::DapServer::new("dap-cli".into(), command.clone());
    let bp = dap.add_breakpoint("src/main.rs", 1);

    println!("\n{}", paint("Sesión de Depuración Supervisada:", BOLD));
    println!("  ID de Sesión:          {}", paint(&dap.session_id, CYAN));
    println!("  Comando en sandbox:    {}", paint(&dap.command, BOLD));
    println!("  Estado:                {}", paint(&dap.state, GREEN));
    println!(
        "  Punto de interrupción: {}:{} (verificado: {})\n",
        bp.file_path, bp.line, bp.verified
    );

    println!("{}", paint("Pila de Llamadas (Call Stack):", BOLD));
    for (i, frame) in dap.call_stack.iter().enumerate() {
        println!("  {}. {}", i + 1, frame);
    }
    println!();

    println!("{}", paint("Variables Locales en Alcance:", BOLD));
    for var in &dap.variables {
        println!(
            "  • {:<16} ({}) = {}",
            paint(&var.name, YELLOW),
            var.type_name,
            paint(&var.value, CYAN)
        );
    }
    println!();

    Ok(())
}

// ------------------------------------------------------------------ desktop

fn cmd_desktop(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "start" | "iniciar" | "run" => {
            let nested = args.iter().any(|a| a == "--nested" || a == "-n");
            println!(
                "\n{} Inicializando entorno gráfico Wayland de antOS...",
                paint("antOS Desktop ·", BOLD)
            );
            let _ = desktop::DesktopManager::sync_configuration(&ctx.workspace)?;
            let out = desktop::DesktopManager::start_session(&ctx.workspace, nested)?;
            println!("{out}");
        }
        "keys" | "hotkeys" | "atajos" => {
            let keys = desktop::DesktopManager::get_hotkeys();
            println!(
                "\n{} Atajos de Teclado Globales del Entorno de Escritorio:",
                paint("antOS Desktop ·", BOLD)
            );
            println!(
                "  {:<16} {:<24} {}",
                paint("ATAJO", BOLD),
                paint("ACCIÓN", BOLD),
                paint("DESCRIPCIÓN", BOLD)
            );
            println!("  {}", "─".repeat(78));
            for k in keys {
                println!(
                    "  {:<16} {:<24} {}",
                    paint(&k.key, CYAN),
                    paint(&k.action, YELLOW),
                    k.description
                );
            }
            println!();
        }
        "status" | "estado" | _ => {
            let status = desktop::DesktopManager::get_status();
            let st = if status.running {
                paint("En ejecución", GREEN)
            } else {
                paint("Inactivo / Headless", DIM)
            };
            println!(
                "\n{} Diagnóstico de Sesión Gráfica Wayland:",
                paint("antOS Desktop ·", BOLD)
            );
            println!("  Estado:                {}", st);
            println!(
                "  Compositor:            {}",
                paint(&status.compositor_name, CYAN)
            );
            println!(
                "  WAYLAND_DISPLAY:       {}",
                paint(
                    status.wayland_display.as_deref().unwrap_or("ninguno"),
                    YELLOW
                )
            );
            println!("  Clientes de capa:      {}", status.active_clients_count);
            println!(
                "  Atajos registrados:    {} combinaciones globales\n",
                status.registered_hotkeys.len()
            );
            println!("  Uso:");
            println!("    antos desktop start       Arranca la sesión de escritorio");
            println!("    antos desktop keys        Muestra todos los atajos de teclado globales");
            println!("    antos desktop status      Diagnostica la sesión activa\n");
        }
    }
    Ok(())
}

// -------------------------------------------------------------------- barra

fn cmd_barra(_ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let manager = barra::BarraManager::global();

    match sub {
        "alert" | "notif" | "notify" | "alerta" => {
            let clean_parts: Vec<&str> = args[1..]
                .iter()
                .filter(|a| *a != "--urgent" && *a != "-u")
                .map(|s| s.as_str())
                .collect();
            let msg = if clean_parts.is_empty() {
                "Prueba de alerta visual".to_string()
            } else {
                clean_parts.join(" ")
            };
            let alert = antos_protocolo::BarraAlert {
                category: "cli".into(),
                message: msg.clone(),
                urgent: args.iter().any(|a| a == "--urgent" || a == "-u"),
            };
            manager.emit_alert(alert)?;
            println!(
                "\n{} Alerta visual emitida a la barra de escritorio: «{}»\n",
                paint("antOS Barra ·", BOLD),
                paint(&msg, GREEN)
            );
        }
        "status" | "telemetry" | "telemetria" | _ => {
            let t = manager.get_telemetry();
            let mb = t.profiler_rss_bytes as f64 / (1024.0 * 1024.0);
            println!(
                "\n{} Telemetría en Tiempo Real de la Barra de Escritorio:",
                paint("antOS Barra ·", BOLD)
            );
            println!(
                "  • eBPF LSM Guard:       {}",
                if t.ebpf_lsm_active {
                    paint("Activo", GREEN)
                } else {
                    paint("Auditoría", YELLOW)
                }
            );
            println!(
                "  • Violaciones LSM:      {}",
                if t.ebpf_violations_count > 0 {
                    paint(&t.ebpf_violations_count.to_string(), RED)
                } else {
                    paint("0", GREEN)
                }
            );
            println!("  • Consumo RSS Pico:     {:.2} MB", mb);
            println!("  • CPU Estimada:         {:.1}%", t.profiler_cpu_percent);
            println!(
                "  • Sesión de Pair:       {}",
                paint(t.active_pair_session.as_deref().unwrap_or("inactiva"), CYAN)
            );
            println!(
                "  • Nodos antMesh:        {} vecinos descubiertos",
                t.mesh_peers_count
            );
            println!(
                "  • Notificaciones:       {} pendientes",
                t.active_notifications_count
            );
            println!("\n  Alertas recientes en cola:");
            let alerts = manager.get_alerts(3);
            if alerts.is_empty() {
                println!("    (sin alertas recientes)");
            } else {
                for a in alerts {
                    let u = if a.urgent {
                        paint("[URGENTE]", RED)
                    } else {
                        paint("[INFO]", CYAN)
                    };
                    println!("    • {u} {}: {}", a.category, a.message);
                }
            }
            println!();
        }
    }
    Ok(())
}

// --------------------------------------------------------------------- boot

fn cmd_boot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let engine = boot::BootEngine::global();

    match sub {
        "build" | "compile" => {
            println!(
                "\n{} Compilando kernel no_std y empaquetando imagen BIOS/UEFI...",
                paint("antOS Boot ·", BOLD)
            );
            let img = engine.build(&ctx.workspace)?;
            println!(
                "  {} {}\n",
                paint("✓ Imagen generada:", GREEN),
                img.display()
            );
        }
        "test" | "check" => {
            println!(
                "\n{} Ejecutando prueba automatizada de arranque en QEMU (headless)...",
                paint("antOS Boot ·", BOLD)
            );
            let res = engine.test_boot(&ctx.workspace)?;
            println!("{}\n", res);
        }
        "qemu" | "run" => {
            println!("\n{} Iniciando QEMU...", paint("antOS Boot ·", BOLD));
            let img = engine.build(&ctx.workspace)?;
            let run_script = ctx.workspace.join("run.sh");
            if run_script.exists() {
                let status = std::process::Command::new("bash")
                    .arg(&run_script)
                    .status()?;
                if !status.success() {
                    bail!("QEMU finalizó con código {:?}", status.code());
                }
            } else {
                let status = std::process::Command::new("qemu-system-x86_64")
                    .args(["-m", "256M", "-serial", "stdio", "-drive"])
                    .arg(format!("format=raw,file={}", img.display()))
                    .status()?;
                if !status.success() {
                    bail!("QEMU finalizó con código {:?}", status.code());
                }
            }
        }
        "iso" => {
            let mut arch = "x86_64";
            let mut iter = args.iter().skip(1);
            while let Some(a) = iter.next() {
                if a == "--arch" {
                    if let Some(val) = iter.next() {
                        arch = val.as_str();
                    }
                } else if a == "aarch64" || a == "arm64" {
                    arch = "aarch64";
                }
            }
            println!(
                "\n{} Construyendo imagen Live ISO autoarrancable ({arch})...",
                paint("antOS Boot ·", BOLD)
            );
            let iso = engine.build_iso_arch(&ctx.workspace, arch)?;
            println!(
                "  {} {}\n",
                paint("✓ Live ISO generada:", GREEN),
                iso.display()
            );
        }
        "release" | "dist" => {
            println!(
                "\n{} Ejecutando pipeline oficial de empaquetado release...",
                paint("antOS Release ·", BOLD)
            );
            let res = engine.build_release(&ctx.workspace)?;
            println!("{}\n", res);
        }
        "status" | _ => {
            let st = engine.status(&ctx.workspace);
            println!(
                "\n{} Estado del Pipeline de Arranque Bare Metal:",
                paint("antOS Boot ·", BOLD)
            );
            println!("  • Arquitectura:         {}", paint(&st.target_arch, CYAN));
            println!(
                "  • Binario Kernel ELF:   {} ({})",
                if st.kernel_elf_exists {
                    paint("Presente", GREEN)
                } else {
                    paint("No compilado", YELLOW)
                },
                if st.kernel_elf_exists {
                    format!("{} KiB", st.kernel_elf_size_bytes / 1024)
                } else {
                    "0 B".into()
                }
            );
            println!(
                "  • Imagen BIOS/MBR:      {} ({})",
                if st.bios_image_exists {
                    paint("Presente", GREEN)
                } else {
                    paint("No generada", YELLOW)
                },
                if st.bios_image_exists {
                    format!(
                        "{:.1} MB",
                        st.bios_image_size_bytes as f64 / (1024.0 * 1024.0)
                    )
                } else {
                    "0 B".into()
                }
            );
            println!(
                "  • Emulador QEMU:        {}",
                if st.qemu_installed {
                    paint("Disponible (qemu-system-x86_64)", GREEN)
                } else {
                    paint("No instalado", RED)
                }
            );
            println!("\n  Uso:");
            println!(
                "    antos boot build      Compila el kernel no_std y crea la imagen de disco"
            );
            println!("    antos boot test       Prueba automatizada de arranque en QEMU headless");
            println!("    antos boot qemu       Lanza la máquina virtual interactiva en QEMU");
            println!("    antos boot iso        Genera la imagen Live ISO autoarrancable");
            println!("    antos boot release    Ejecuta el pipeline de empaquetado y checksums\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------- plugins

fn cmd_plugin(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let plugins_dir = wasm::PluginManager::get_plugins_dir(&ctx.workspace);

    match sub {
        "list" | "ls" => {
            let list = wasm::PluginManager::list_plugins(&plugins_dir);
            println!(
                "\n{} Plugins WebAssembly (WASM) Registrados:",
                paint("antOS ·", BOLD)
            );
            if list.is_empty() {
                println!(
                    "  (no hay plugins instalados en {})\n",
                    plugins_dir.display()
                );
            } else {
                for p in list {
                    let kb = (p.wasm_size_bytes + 1023) / 1024;
                    println!(
                        "  • {} v{} ({} KiB) - {}",
                        paint(&p.name, GREEN),
                        paint(&p.version, CYAN),
                        kb,
                        p.description
                    );
                    println!("    Acciones disponibles: {}", p.capabilities.join(", "));
                }
                println!();
            }
        }
        "install" => {
            let path_str = args.get(1).map(String::as_str).unwrap_or(".");
            let src = ctx.workspace.join(path_str);
            println!(
                "\n{} Instalando plugin desde {}...",
                paint("antOS ·", BOLD),
                src.display()
            );
            let summary = wasm::PluginManager::install_plugin(&plugins_dir, &src)?;
            println!(
                "  {} {} v{} (acciones: {})\n",
                paint("✓ Plugin instalado:", GREEN),
                summary.name,
                summary.version,
                summary.capabilities.join(", ")
            );
        }
        "run" => {
            let name = args.get(1).map(String::as_str).unwrap_or("");
            if name.is_empty() {
                bail!("Uso: antos plugin run <nombre> [accion] [clave=valor...]");
            }
            let action = args.get(2).map(String::as_str).unwrap_or("run");
            let mut params = std::collections::BTreeMap::new();
            for arg in args.iter().skip(3) {
                if let Some((k, v)) = arg.split_once('=') {
                    params.insert(k.to_string(), v.to_string());
                }
            }

            println!(
                "\n{} Ejecutando plugin [{}:{}] en sandbox aislado WASM...",
                paint("antOS ·", BOLD),
                name,
                action
            );
            let res = wasm::PluginManager::run_plugin(&plugins_dir, name, action, &params);
            if res.success {
                println!("  {} {}", paint("✓ Resultado:", GREEN), res.output);
                println!("    Ciclos de instrucción:  {}", res.fuel_consumed);
                println!(
                    "    Memoria lineal:         {} KiB (cuota máx: 64 MB)\n",
                    res.memory_allocated_bytes / 1024
                );
            } else {
                let err = res.error.unwrap_or_else(|| "Error desconocido".into());
                println!("  {} {}\n", paint("✗ Error:", RED), err);
                bail!("Fallo durante ejecución en sandbox WASM");
            }
        }
        _ => {
            println!(
                "\n{} Gestor de Plugins WebAssembly (WASM):",
                paint("antOS Plugins ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos plugin list                   Enumera plugins instalados");
            println!("    antos plugin install <directorio>   Instala un plugin con plugin.toml");
            println!(
                "    antos plugin run <nombre> [accion]  Ejecuta una acción en sandbox WASM\n"
            );
        }
    }
    Ok(())
}

// -------------------------------------------------------- screenshot & visual qa

fn cmd_screenshot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let target = args.first().map(String::as_str);
    let path = if args.len() > 1 {
        Some(ctx.workspace.join(&args[1]))
    } else if let Some(t) = target {
        if t.ends_with(".png") || t.ends_with(".bmp") {
            Some(ctx.workspace.join(t))
        } else {
            None
        }
    } else {
        None
    };

    let actual_target = if let Some(t) = target {
        if t.ends_with(".png") || t.ends_with(".bmp") {
            None
        } else {
            Some(t)
        }
    } else {
        None
    };

    println!(
        "\n{} Captura de Pantalla Wayland:",
        paint("antOS Screencopy ·", BOLD)
    );
    let engine = vision::VisionEngine::global();
    let res = engine.capture_screen(actual_target, path.as_deref())?;

    let saved = res.saved_path.as_deref().unwrap_or("en memoria");
    println!(
        "  {} Captura completada para «{}»",
        paint("✓", GREEN),
        res.target
    );
    println!("    Destino:      {}", saved);
    println!(
        "    Resolución:   {}x{} píxeles ({})",
        res.width,
        res.height,
        res.format.to_uppercase()
    );
    println!(
        "    Tamaño:       {} KiB ({} bytes)\n",
        res.size_bytes / 1024,
        res.size_bytes
    );
    Ok(())
}

fn cmd_qa(ctx: &Ctx, args: &[String]) -> Result<()> {
    let _ = ctx;
    let sub = args.first().map(String::as_str);
    match sub {
        Some("visual" | "vis" | "ui") => {
            let target = args.get(1).map(String::as_str).unwrap_or("desktop");
            let criteria: Vec<String> = if args.len() > 2 {
                args[2..].iter().map(|s| s.replace('_', " ")).collect()
            } else {
                vec![
                    "Verificar contraste de color accesible".to_string(),
                    "Comprobar márgenes y alineación de elementos".to_string(),
                    "Verificar ausencia de desbordamientos visuales".to_string(),
                ]
            };

            println!(
                "\n{} Agente Multimodal VisualQA:",
                paint("antOS QA Visual ·", BOLD)
            );
            println!("  Objetivo: {}", paint(target, CYAN));
            println!("  Criterios evaluados: {}\n", criteria.len());

            let engine = vision::VisionEngine::global();
            let report = engine.inspect_visual(target, &criteria, None)?;

            let status_badge = if report.pass {
                paint("APROBADO", GREEN)
            } else {
                paint("RECHAZADO", RED)
            };

            println!("  {} {}", paint("Resultado:", BOLD), status_badge);
            println!("  Resumen:     {}", report.summary);
            println!(
                "  Resolución:  {}x{} píxeles ({} KiB)\n",
                report.image_width,
                report.image_height,
                report.image_size_bytes / 1024
            );

            println!("  {}:", paint("Hallazgos de Inspección", BOLD));
            for f in &report.findings {
                let sev = match f.severity.as_str() {
                    "critical" => paint("[CRÍTICO]", RED),
                    "warning" => paint("[ADVERTENCIA]", YELLOW),
                    _ => paint("[INFO]", CYAN),
                };
                println!(
                    "    • {} {}: {}",
                    sev,
                    paint(&f.category, BOLD),
                    f.description
                );
                if let Some(ref coords) = f.coordinates {
                    println!("      Coordenadas:   {}", coords);
                }
                println!("      Recomendación: {}", f.recommendation);
            }
            println!();

            if !report.pass {
                bail!("Inspección visual rechazada debido a fallos críticos");
            }
        }
        _ => {
            println!(
                "\n{} Inspección de Calidad Visual (antFlow QA):",
                paint("antOS QA ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos qa visual [target] [criterios...]  Auditoría visual con agente multimodal\n");
        }
    }
    Ok(())
}

// -------------------------------------------------------- disk & partitioning (T15.1)

fn cmd_disk(_ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");

    match sub {
        "list" | "ls" => {
            println!(
                "\n{} Unidades de Almacenamiento Detectadas:",
                paint("antOS Almacenamiento ·", BOLD)
            );
            let disks = installer::DiskManager::list_disks()?;
            if disks.is_empty() {
                println!("  (no se detectaron unidades de bloque en el sistema)\n");
                return Ok(());
            }

            for d in &disks {
                let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                println!(
                    "  • {} ({:.1} GB, Bus: {}, Tabla: {})",
                    paint(&d.path, CYAN),
                    gb,
                    d.bus_type,
                    d.partition_table
                );
                println!("    Modelo:      {}", d.model);
                println!(
                    "    Sectores:    {} bytes / sector (RO: {})",
                    d.sector_size, d.is_read_only
                );
                if d.partitions.is_empty() {
                    println!("    Particiones: (disco sin particiones)");
                } else {
                    println!("    Particiones: {} detectadas", d.partitions.len());
                    for p in &d.partitions {
                        let p_mb = p.size_bytes / (1024 * 1024);
                        let efi_badge = if p.is_efi {
                            paint(" [EFI ESP]", GREEN)
                        } else {
                            "".into()
                        };
                        let fs = p.fs_type.as_deref().unwrap_or("desconocido");
                        let mount = p.mountpoint.as_deref().unwrap_or("no montada");
                        println!(
                            "      - {} ({:.0} MB, {}) → {}{}",
                            paint(&p.name, BOLD),
                            p_mb,
                            fs,
                            mount,
                            efi_badge
                        );
                    }
                }
                println!();
            }
        }
        "inspect" | "info" => {
            let target = args.get(1).map(String::as_str).unwrap_or("");
            if target.is_empty() {
                bail!("Uso: antos disk inspect <dispositivo>");
            }

            println!(
                "\n{} Inspeccionando Dispositivo {}:",
                paint("antOS Almacenamiento ·", BOLD),
                paint(target, CYAN)
            );
            let disk = installer::DiskManager::inspect_disk(target)?;
            match disk {
                Some(d) => {
                    let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    println!("  • Ruta física:       {}", paint(&d.path, BOLD));
                    println!("  • Modelo / Vendor:   {}", d.model);
                    println!(
                        "  • Tamaño total:      {:.2} GB ({} bytes)",
                        gb, d.size_bytes
                    );
                    println!("  • Tamaño de sector:  {} bytes (LBA)", d.sector_size);
                    println!("  • Tipo de Bus:       {}", d.bus_type);
                    println!("  • Tabla:             {}", d.partition_table);
                    println!("  • Solo Lectura:      {}\n", d.is_read_only);

                    println!("  {}:", paint("Mapa de Particiones", BOLD));
                    if d.partitions.is_empty() {
                        println!("    (sin particiones registradas)");
                    } else {
                        for p in &d.partitions {
                            let efi_str = if p.is_efi {
                                paint(" [SISTEMA EFI]", GREEN)
                            } else {
                                "".into()
                            };
                            println!(
                                "    #{}: {} | {:.1} MB | FS: {} | UUID: {}{}",
                                p.number,
                                paint(&p.name, CYAN),
                                p.size_bytes as f64 / (1024.0 * 1024.0),
                                p.fs_type.as_deref().unwrap_or("none"),
                                p.uuid.as_deref().unwrap_or("N/A"),
                                efi_str
                            );
                        }
                    }
                    println!();
                }
                None => {
                    bail!("No se encontró el dispositivo «{}»", target);
                }
            }
        }
        "partition" | "part" => {
            let target = args.get(1).map(String::as_str).unwrap_or("");
            if target.is_empty() {
                bail!("Uso: antos disk partition <dispositivo> [--clean | --dual-boot] [--apply]");
            }

            let clean = args.iter().any(|a| a == "--clean");
            let apply = args.iter().any(|a| a == "--apply");
            let dry_run = !apply;

            println!(
                "\n{} Calculando esquema de particionado para {}:",
                paint("antOS Particionador ·", BOLD),
                paint(target, CYAN)
            );

            let plan = installer::DiskManager::plan_partitioning(target, clean)?;
            let report = installer::DiskManager::apply_partitioning(target, &plan, dry_run)?;
            println!("{}\n", report);
            if dry_run {
                println!("  {} Para aplicar estos cambios en el disco use «--apply» (acción destructiva).\n", paint("Nota:", YELLOW));
            }
        }
        _ => {
            println!(
                "\n{} Gestor de Discos y Particiones GPT:",
                paint("antOS Almacenamiento ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos disk list                           Enumera discos físicos y particiones");
            println!("    antos disk inspect <dispositivo>          Muestra el mapa de particiones y metadatos");
            println!("    antos disk partition <dispositivo> [modo] Calcula o aplica tabla GPT (--clean / --dual-boot)\n");
        }
    }
    Ok(())
}

// ---------------------------------------------------- installer & deploy (T15.2)

fn cmd_install(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");

    match sub {
        "list" | "disks" => {
            println!(
                "\n{} Discos Compatibles para Instalación de antOS:",
                paint("antOS Instalador ·", BOLD)
            );
            let disks = installer::DiskManager::list_disks()?;
            if disks.is_empty() {
                println!("  (no se detectaron unidades de almacenamiento compatibles)\n");
                return Ok(());
            }

            for (idx, d) in disks.iter().enumerate() {
                let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let suitable = d.size_bytes >= 8 * 1024 * 1024 * 1024;
                let status_str = if suitable {
                    paint("COMPATIBLE (≥ 8 GB)", GREEN)
                } else {
                    paint("INSUFICIENTE (< 8 GB)", RED)
                };

                let has_efi = d.partitions.iter().any(|p| p.is_efi);
                let mode_rec = if has_efi {
                    paint("Recomendado: Dual Boot", CYAN)
                } else {
                    paint("Recomendado: Sistema Principal Limpio", YELLOW)
                };

                println!(
                    "  [{}] {} ({:.1} GB, Bus: {})",
                    idx + 1,
                    paint(&d.path, BOLD),
                    gb,
                    d.bus_type
                );
                println!("      Modelo:       {}", d.model);
                println!("      Estado:       {} | {}", status_str, mode_rec);
                println!("      Particiones:  {} existentes\n", d.partitions.len());
            }
        }
        "run" | "deploy" => {
            let mut target_device = "/dev/nvme0n1".to_string();
            let mut clean_install = false;
            let mut username = "antos".to_string();
            let mut hostname = "antos-box".to_string();
            let mut dry_run = true;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--target" | "-t" => {
                        if i + 1 < args.len() {
                            target_device = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--clean" => clean_install = true,
                    "--dual-boot" => clean_install = false,
                    "--user" | "-u" => {
                        if i + 1 < args.len() {
                            username = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--host" => {
                        if i + 1 < args.len() {
                            hostname = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--apply" => dry_run = false,
                    _ => {}
                }
                i += 1;
            }

            let config = antos_protocolo::InstallConfig {
                target_device: target_device.clone(),
                clean_install,
                target_mount: "/mnt/antos".into(),
                hostname,
                username,
                timezone: "UTC".into(),
                dry_run,
            };

            let mode_str = if clean_install {
                "Sistema Principal (Limpio)"
            } else {
                "Sistema Secundario (Dual Boot)"
            };
            println!(
                "\n{} Iniciando Despliegue de antOS:",
                paint("antOS Instalador ·", BOLD)
            );
            println!("  • Dispositivo:      {}", paint(&target_device, CYAN));
            println!("  • Modo de instalación: {}", paint(mode_str, BOLD));
            println!(
                "  • Modo de ejecución:   {}\n",
                if dry_run {
                    paint("SIMULACIÓN SEGURA (Dry-Run)", YELLOW)
                } else {
                    paint("INSTALACIÓN EN DISCO REAL", RED)
                }
            );

            let report = installer::DeployEngine::deploy_system(&config, &ctx.workspace)?;
            println!(
                "  {}: {}",
                paint("Resultado", BOLD),
                if report.success {
                    paint("EXITOSO", GREEN)
                } else {
                    paint("FALLIDO", RED)
                }
            );
            println!("  {}\n", report.summary);
            println!("  {}:", paint("Pasos Ejecutados", BOLD));
            for s in &report.steps {
                println!("    ✓ {}: {}", paint(&s.name, CYAN), s.description);
            }
            println!("\n  {}:", paint("Entradas /etc/fstab Generadas", BOLD));
            for f in &report.fstab_entries {
                println!("    {}", f);
            }
            println!();

            if dry_run {
                println!("  {} Para aplicar esta instalación de forma definitiva en el hardware ejecute con «--apply».\n", paint("Nota:", YELLOW));
            }
        }
        "wizard" | "gui" => {
            println!(
                "\n{}",
                paint(
                    "╔════════════════════════════════════════════════════════════════╗",
                    CYAN
                )
            );
            println!(
                "{}",
                paint(
                    "║           antOS · Asistente de Instalación Guiada             ║",
                    BOLD
                )
            );
            println!(
                "{}\n",
                paint(
                    "╚════════════════════════════════════════════════════════════════╝",
                    CYAN
                )
            );

            let disks = installer::DiskManager::list_disks()?;
            if disks.is_empty() {
                bail!("No se detectaron discos de almacenamiento disponibles para instalar");
            }

            let chosen_disk = &disks[0];
            let has_efi = chosen_disk.partitions.iter().any(|p| p.is_efi);
            let mode_str = if has_efi {
                "Dual Boot (preservando partición EFI y SO vecino)"
            } else {
                "Sistema Principal Completo"
            };

            println!(
                "  Disco detectado para instalación: {} ({:.1} GB)",
                paint(&chosen_disk.path, CYAN),
                chosen_disk.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            );
            println!(
                "  Modo seleccionado automáticamente: {}",
                paint(mode_str, GREEN)
            );
            println!(
                "  Usuario predeterminado:           {}",
                paint("antos", BOLD)
            );
            println!(
                "  Hostname:                         {}",
                paint("antos-box", BOLD)
            );
            println!("\n  Ejecutando simulación de instalación guiada...");

            let cfg = antos_protocolo::InstallConfig {
                target_device: chosen_disk.path.clone(),
                clean_install: !has_efi,
                target_mount: "/mnt/antos".into(),
                hostname: "antos-box".into(),
                username: "antos".into(),
                timezone: "UTC".into(),
                dry_run: true,
            };

            let report = installer::DeployEngine::deploy_system(&cfg, &ctx.workspace)?;
            println!(
                "\n  {} {}",
                paint("✓ Verificación de instalación completada:", GREEN),
                report.summary
            );
            println!("    • Partición ESP:   {}", report.efi_partition);
            println!("    • Partición Raíz:  {}", report.root_partition);
            println!(
                "    • Pasos validados: {}/{}",
                report.steps.len(),
                report.steps.len()
            );
            println!("\n  Para proceder a instalar en vivo sobre este equipo, ejecute:");
            println!(
                "    {}\n",
                paint(
                    &format!("antos install run --target {} --apply", chosen_disk.path),
                    CYAN
                )
            );
        }
        _ => {
            println!(
                "\n{} Asistente de Instalación en Disco Duro y Dual Boot:",
                paint("antOS Instalador ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos install list                           Enumera discos compatibles y sugerencias de modo");
            println!("    antos install wizard                         Asistente interactivo guiado de instalación");
            println!("    antos install run --target <dev> [--clean]   Ejecuta el despliegue del sistema base");
            println!("    antos install run --target <dev> --apply     Aplica los cambios irreversibles al disco\n");
        }
    }
    Ok(())
}

// ---------------------------------------------------- bootloader & uefi (T15.3)

fn cmd_bootloader(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");

    match sub {
        "probe" | "detect" | "os" => {
            let mut esp_path = "/boot/efi".to_string();
            let mut i = 1;
            while i < args.len() {
                if args[i] == "--esp" && i + 1 < args.len() {
                    esp_path = args[i + 1].clone();
                    i += 1;
                }
                i += 1;
            }

            println!(
                "\n{} Sondeando sistemas operativos en «{}»:",
                paint("antOS Bootloader ·", BOLD),
                esp_path
            );
            let entries = installer::BootloaderEngine::probe_operating_systems(
                std::path::Path::new(&esp_path),
            )?;
            if entries.is_empty() {
                println!(
                    "  (no se detectaron sistemas operativos en el directorio especificado)\n"
                );
                return Ok(());
            }

            for (idx, os) in entries.iter().enumerate() {
                let badge = match os.os_type.as_str() {
                    "windows" => paint("[WINDOWS]", CYAN),
                    "linux" => paint("[LINUX]", GREEN),
                    "macos" => paint("[MACOS]", YELLOW),
                    _ => paint("[ANTOS]", BOLD),
                };
                println!("  [{}] {} {}", idx + 1, badge, paint(&os.name, BOLD));
                println!("      Ruta binario EFI:  {}", os.efi_path);
                println!(
                    "      Dispositivo/Part:  {} (Partición #{})\n",
                    os.disk_device, os.partition_number
                );
            }
        }
        "install" | "deploy" => {
            let mut esp_path = "/boot/efi".to_string();
            let mut target_device = "/dev/nvme0n1".to_string();
            let mut efi_partition = 1u32;
            let mut timeout_seconds = 5u32;
            let mut dry_run = true;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--esp" => {
                        if i + 1 < args.len() {
                            esp_path = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--target" | "-t" => {
                        if i + 1 < args.len() {
                            target_device = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--partition" | "-p" => {
                        if i + 1 < args.len() {
                            if let Ok(num) = args[i + 1].parse::<u32>() {
                                efi_partition = num;
                            }
                            i += 1;
                        }
                    }
                    "--timeout" => {
                        if i + 1 < args.len() {
                            if let Ok(num) = args[i + 1].parse::<u32>() {
                                timeout_seconds = num;
                            }
                            i += 1;
                        }
                    }
                    "--apply" => dry_run = false,
                    _ => {}
                }
                i += 1;
            }

            let esp = std::path::PathBuf::from(if dry_run {
                ctx.workspace
                    .join("target/esp-staging")
                    .display()
                    .to_string()
            } else {
                esp_path.clone()
            });

            let config = antos_protocolo::BootloaderConfig {
                esp_mount: esp.display().to_string(),
                target_device: target_device.clone(),
                efi_partition,
                default_os: "antos".into(),
                timeout_seconds,
                detected_os: Vec::new(),
                dry_run,
            };

            println!(
                "\n{} Instalando Gestor de Arranque UEFI (systemd-boot):",
                paint("antOS Bootloader ·", BOLD)
            );
            println!("  • Directorio ESP:     {}", paint(&config.esp_mount, CYAN));
            println!("  • Dispositivo destino: {}", paint(&target_device, CYAN));
            println!("  • Partición EFI:      #{}", efi_partition);
            println!("  • Timeout menú:       {} segundos", timeout_seconds);
            println!(
                "  • Modo de ejecución:  {}\n",
                if dry_run {
                    paint("SIMULACIÓN SEGURA (Dry-Run)", YELLOW)
                } else {
                    paint("ESCRITURA EN ESP Y NVRAM", RED)
                }
            );

            let report = installer::BootloaderEngine::install_bootloader(&config)?;
            println!(
                "  {}: {}",
                paint("Resultado", BOLD),
                if report.success {
                    paint("EXITOSO", GREEN)
                } else {
                    paint("FALLIDO", RED)
                }
            );
            println!("  {}\n", report.summary);
            println!("  {}:", paint("Entradas de Arranque Generadas", BOLD));
            for e in &report.entries_configured {
                println!("    ✓ {}", e);
            }
            println!("\n  {}:", paint("Comando de Registro NVRAM", BOLD));
            println!("    {}\n", paint(&report.efibootmgr_command, CYAN));

            if dry_run {
                println!(
                    "  {} Para aplicar estos cambios en el firmware UEFI use «--apply».\n",
                    paint("Nota:", YELLOW)
                );
            }
        }
        _ => {
            println!(
                "\n{} Gestor de Arranque UEFI y Dual Boot:",
                paint("antOS Bootloader ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos bootloader probe [--esp <ruta>]        Sondea sistemas operativos instalados");
            println!("    antos bootloader install [--esp <ruta>]      Genera y valida la configuración de systemd-boot");
            println!("    antos bootloader install --apply             Registra antOS en la NVRAM UEFI con efibootmgr\n");
        }
    }
    Ok(())
}

// ----------------------------------------------------------------- microvms

fn cmd_vm(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "spawn" | "start" | "run" => {
            let mut vm_id = format!("vm-{}", chrono::Local::now().format("%Y%m%d%H%M%S"));
            let mut vcpu_count = 2u8;
            let mut memory_mb = 512u32;
            let mut kernel_image = "/boot/antos-vmlinuz".to_string();
            let mut command = None;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--id" if i + 1 < args.len() => {
                        vm_id = args[i + 1].clone();
                        i += 1;
                    }
                    "--cpus" | "-c" if i + 1 < args.len() => {
                        if let Ok(c) = args[i + 1].parse::<u8>() {
                            vcpu_count = c;
                        }
                        i += 1;
                    }
                    "--memory" | "-m" if i + 1 < args.len() => {
                        if let Ok(m) = args[i + 1].parse::<u32>() {
                            memory_mb = m;
                        }
                        i += 1;
                    }
                    "--kernel" | "-k" if i + 1 < args.len() => {
                        kernel_image = args[i + 1].clone();
                        i += 1;
                    }
                    "--cmd" if i + 1 < args.len() => {
                        command = Some(args[i + 1].clone());
                        i += 1;
                    }
                    _ => {}
                }
                i += 1;
            }

            println!("\n{} Instanciando microVM con aislamiento por hipervisor...", paint("antOS MicroVM ·", BOLD));
            let cfg = antos_protocol::MicrovmConfig {
                vm_id: vm_id.clone(),
                vcpu_count,
                memory_mb,
                kernel_image: kernel_image.clone(),
                initrd_image: None,
                overlay_disk: None,
                vsock_port: 5252,
                command,
            };

            let instance = vm::MicrovmManager::spawn_vm(&ctx.state, &cfg)?;
            println!("  {} MicroVM «{}» arrancada exitosamente", paint("✓", GREEN), paint(&instance.id, BOLD));
            println!("    • PID:          {}", instance.pid);
            println!("    • vCPUs:        {}", instance.vcpus);
            println!("    • Memoria:      {} MB", instance.memory_mb);
            println!("    • Puerto vsock: {}", instance.vsock_port);
            println!("    • Estado:       {}\n", paint(&instance.status, GREEN));
        }
        "exec" => {
            let vm_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos vm exec <VM_ID> <comando>"))?;
            let command = if args.len() > 2 {
                args[2..].join(" ")
            } else {
                bail!("Uso: antos vm exec <VM_ID> <comando>");
            };

            println!("\n{} Ejecutando comando en microVM «{}»...", paint("antOS MicroVM ·", BOLD), paint(vm_id, CYAN));
            let res = vm::MicrovmManager::exec_vm(&ctx.state, vm_id, &command)?;
            let status_badge = if res.success { paint("EXITOSO", GREEN) } else { paint("FALLIDO", RED) };
            println!("  Resultado:  {} (código {})", status_badge, res.exit_code);
            println!("  Duración:   {} ms", res.duration_ms);
            if !res.stdout.is_empty() {
                println!("\n  Salida:\n{}", res.stdout.trim());
            }
            if !res.stderr.is_empty() {
                println!("\n  Errores:\n{}", paint(&res.stderr, RED));
            }
            println!();
        }
        "list" | "ls" => {
            println!("\n{} MicroVMs Activas en el Sistema:", paint("antOS MicroVM ·", BOLD));
            let vms = vm::MicrovmManager::list_vms(&ctx.state)?;
            if vms.is_empty() {
                println!("  (no hay microVMs activas en este momento)\n");
            } else {
                for v in &vms {
                    println!("  • [{}] {} (PID {}, {} vCPUs, {} MB RAM, vsock {})",
                        paint(&v.id, BOLD),
                        paint(&v.status, GREEN),
                        v.pid,
                        v.vcpus,
                        v.memory_mb,
                        v.vsock_port
                    );
                }
                println!();
            }
        }
        "kill" | "stop" | "destroy" => {
            let vm_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos vm kill <VM_ID>"))?;
            vm::MicrovmManager::kill_vm(&ctx.state, vm_id)?;
            println!("\n{} MicroVM «{}» detenida y eliminada.\n", paint("✓", GREEN), paint(vm_id, BOLD));
        }
        "status" | _ => {
            println!("\n{} Diagnóstico de Hipervisor y MicroVMs:", paint("antOS MicroVM ·", BOLD));
            let st = vm::MicrovmManager::get_status(&ctx.state)?;
            let kvm_badge = if st.kvm_available { paint("Disponible (/dev/kvm)", GREEN) } else { paint("No detectado (Emulación)", YELLOW) };
            println!("  • Soporte KVM:             {}", kvm_badge);
            println!("  • Motor de Hipervisor:     {}", paint(&st.hypervisor_engine, CYAN));
            println!("  • Kernel del Host:         {}", st.kernel_version);
            println!("  • MicroVMs activas:        {}", st.active_vms_count);
            println!("  • Memoria asignada a VMs:  {} MB", st.total_memory_allocated_mb);
            println!("  • Canales vsock:           {}", if st.vsock_supported { paint("Soportado", GREEN) } else { paint("No disponible", RED) });
            println!("\n  Uso:");
            println!("    antos vm spawn [--cpus N] [--memory MB]   Arranca una microVM efímera");
            println!("    antos vm exec <VM_ID> <comando>          Ejecuta un comando en la microVM");
            println!("    antos vm list                             Lista microVMs en ejecución");
            println!("    antos vm kill <VM_ID>                     Detiene y libera una microVM\n");
        }
    }
    Ok(())
}

// ----------------------------------------------------------- antpkg (T16.2)

fn cmd_pkg(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "install" | "add" | "i" => {
            let pkg_or_recipe = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos pkg install <paquete|receta.toml> [--dry-run]"))?;
            let dry_run = args.iter().any(|a| a == "--dry-run" || a == "-d");

            println!("\n{} Instalando paquete en el almacén inmutable...", paint("antOS antpkg ·", BOLD));
            let rep = pkg::PackageEngine::install(&ctx.state, pkg_or_recipe, dry_run)?;
            let status_badge = if rep.success { paint("INSTALADO", GREEN) } else { paint("ERROR", RED) };
            println!("  Resultado:     {}", status_badge);
            println!("  Paquete:       {} v{}", paint(&rep.name, BOLD), rep.version);
            println!("  Generación:    {}", paint(&rep.generation.to_string(), CYAN));
            println!("  Almacén:       {}", rep.store_path);
            if !rep.binaries_linked.is_empty() {
                println!("  Binarios:      {}", rep.binaries_linked.join(", "));
            }
            println!("  Mensaje:       {}\n", rep.message);
        }
        "remove" | "rm" | "uninstall" => {
            let pkg = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos pkg remove <nombre_paquete>"))?;
            println!("\n{} Desvinculando paquete del perfil activo...", paint("antOS antpkg ·", BOLD));
            let rep = pkg::PackageEngine::remove(&ctx.state, pkg)?;
            println!("  {} Paquete «{}» desvinculado.", paint("✓", GREEN), paint(pkg, BOLD));
            println!("  Nueva generación activa: {}\n", paint(&rep.generation.to_string(), CYAN));
        }
        "list" | "ls" => {
            println!("\n{} Paquetes en el Perfil Activo:", paint("antOS antpkg ·", BOLD));
            let pkgs = pkg::PackageEngine::list(&ctx.state)?;
            if pkgs.is_empty() {
                println!("  (no hay paquetes instalados en el perfil activo)\n");
            } else {
                for p in &pkgs {
                    let kb = p.installed_size_bytes / 1024;
                    println!("  • {} v{} ({} KiB, gen {}) [bin: {}]",
                        paint(&p.name, BOLD),
                        p.version,
                        kb,
                        p.generation,
                        paint(&p.binaries.join(", "), CYAN)
                    );
                }
                println!();
            }

            let gens = pkg::PackageEngine::list_generations(&ctx.state)?;
            if !gens.is_empty() {
                println!("{} Historial de Generaciones:", paint("antOS antpkg ·", BOLD));
                for g in &gens {
                    let mark = if g.active { paint(" (activa)", GREEN) } else { "".to_string() };
                    println!("  • Generación {}{}: {} paquetes [{}] ({})",
                        g.generation,
                        mark,
                        g.packages.len(),
                        g.packages.join(", "),
                        g.timestamp
                    );
                }
                println!();
            }
        }
        "rollback" | "revert" => {
            let target_gen = args.get(1).and_then(|g| g.parse::<u64>().ok());
            println!("\n{} Revirtiendo perfil de paquetes de forma atómica...", paint("antOS antpkg ·", BOLD));
            let rep = pkg::PackageEngine::rollback(&ctx.state, target_gen)?;
            println!("  {} Rollback exitoso a la generación {}.", paint("✓", GREEN), paint(&rep.generation.to_string(), CYAN));
            println!("  Binarios activos: {}\n", rep.binaries_linked.join(", "));
        }
        "verify" | "check" => {
            println!("\n{} Verificando integridad criptográfica y sumas SHA-256...", paint("antOS antpkg ·", BOLD));
            let (all_valid, count, details) = pkg::PackageEngine::verify(&ctx.state)?;
            let badge = if all_valid { paint("INTEGRIDAD VERIFICADA", GREEN) } else { paint("FALLO DE INTEGRIDAD", RED) };
            println!("  Estado: {} ({} paquetes comprobados)", badge, count);
            for d in &details {
                println!("    {d}");
            }
            println!();
        }
        "status" | _ => {
            println!("\n{} Estado del Almacén Inmutable y Perfiles:", paint("antOS antpkg ·", BOLD));
            let st = pkg::PackageEngine::status(&ctx.state)?;
            let mb = st.total_store_bytes as f64 / (1024.0 * 1024.0);
            println!("  • Directorio de Almacén:   {}", paint(&st.store_path, CYAN));
            println!("  • Directorio de Perfil:    {}", paint(&st.current_profile_path, CYAN));
            println!("  • Generación Activa:       {}", paint(&st.current_generation.to_string(), BOLD));
            println!("  • Paquetes Instalados:     {}", st.total_packages);
            println!("  • Generaciones Totales:    {}", st.generations_count);
            println!("  • Espacio Ocupado Store:   {:.2} MB", mb);
            println!("\n  Uso:");
            println!("    antos pkg install <paquete|receta.toml> [--dry-run]  Instala un paquete en el store");
            println!("    antos pkg remove <paquete>                         Desvincula un paquete del perfil");
            println!("    antos pkg list                                     Lista paquetes y generaciones");
            println!("    antos pkg rollback [generacion]                    Restaura una generación previa");
            println!("    antos pkg verify                                   Verifica hashes y firmas ed25519");
            println!("    antos pkg status                                   Muestra estado del almacén\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------- autopilot (T16.3)

fn cmd_autopilot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "start" => {
            let mut interval = 5u64;
            let mut auto_merge = false;
            let mut i = 1;
            while i < args.len() {
                if (args[i] == "--interval" || args[i] == "-i") && i + 1 < args.len() {
                    if let Ok(v) = args[i + 1].parse::<u64>() {
                        interval = v;
                    }
                    i += 2;
                } else if args[i] == "--auto-merge" || args[i] == "-m" {
                    auto_merge = true;
                    i += 1;
                } else {
                    i += 1;
                }
            }

            let config = antos_protocol::AutopilotConfig {
                enabled: true,
                poll_interval_secs: interval,
                watch_paths: Vec::new(),
                auto_merge,
                target_branch: "master".to_string(),
            };

            println!("\n{} Iniciando centinela continuo en segundo plano...", paint("antOS Autopilot ·", BOLD));
            let st = autopilot::AutopilotEngine::start(&ctx.state, &ctx.workspace, config)?;
            println!("  Estado:                {}", paint("ACTIVO (Vigilando)", GREEN));
            println!("  Intervalo de sondeo:   {}s", st.poll_interval_secs);
            println!("  Espacio de trabajo:    {}", st.workspace_path);
            println!("  Incidentes detectados: {}\n", paint(&st.active_incidents_count.to_string(), if st.active_incidents_count > 0 { YELLOW } else { CYAN }));
        }

        "stop" => {
            println!("\n{} Deteniendo centinela...", paint("antOS Autopilot ·", BOLD));
            let st = autopilot::AutopilotEngine::stop(&ctx.state, &ctx.workspace)?;
            println!("  Estado:               {}", paint("DETENIDO", RED));
            println!("  Incidentes resueltos: {}\n", st.resolved_incidents_count);
        }

        "scan" => {
            println!("\n{} Escaneando el workspace en busca de errores y fallos...", paint("antOS Autopilot ·", BOLD));
            let incs = autopilot::AutopilotEngine::scan_workspace(&ctx.state, &ctx.workspace)?;
            if incs.is_empty() {
                println!("  ✓ {} Repositorio limpio, cero incidencias.", paint("OK", GREEN));
            } else {
                println!("  ⚠ Detectadas {} incidencias con propuestas generadas:", paint(&incs.len().to_string(), YELLOW));
                for inc in incs {
                    println!("    • [{}] {} en «{}» — {}", paint(&inc.id, BOLD), paint(&inc.incident_type, CYAN), inc.file_path, inc.error_message);
                    if let Some(ref prop) = inc.fix_proposal {
                        println!("      Rama: {} | QA: {}", prop.branch, prop.test_output.lines().next().unwrap_or(""));
                    }
                }
            }
            println!();
        }

        "list" | "log" | "incidents" => {
            let incs = autopilot::AutopilotEngine::list_incidents(&ctx.state)?;
            if incs.is_empty() {
                println!("\n{} No hay incidencias registradas.", paint("antOS Autopilot ·", BOLD));
            } else {
                println!("\n{} Historial de Incidencias ({}):", paint("antOS Autopilot ·", BOLD), incs.len());
                for inc in incs {
                    let st_badge = match inc.status.as_str() {
                        "resolved" => paint("RESUELTO", GREEN),
                        "ready_for_approval" => paint("PENDIENTE", YELLOW),
                        "dismissed" => paint("DESCARTADO", DIM),
                        other => paint(other, CYAN),
                    };
                    println!("  • [{}] {} en «{}» [{}]", paint(&inc.id, BOLD), paint(&inc.incident_type, CYAN), inc.file_path, st_badge);
                    println!("    Detalle: {}", inc.error_message);
                    if let Some(ref prop) = inc.fix_proposal {
                        println!("    Fix:     {}", prop.title);
                    }
                }
            }
            println!();
        }

        "approve" | "merge" => {
            let incident_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos autopilot approve <incident_id>"))?;
            println!("\n{} Aprobando propuesta para incidente «{}»...", paint("antOS Autopilot ·", BOLD), incident_id);
            let inc = autopilot::AutopilotEngine::resolve_incident(&ctx.state, &ctx.workspace, incident_id, true)?;
            println!("  ✓ {} Corrección aplicada en {}", paint("APROBADO", GREEN), inc.file_path);
            println!("  Estado: {}\n", paint(&inc.status, BOLD));
        }

        "reject" | "dismiss" => {
            let incident_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos autopilot reject <incident_id>"))?;
            println!("\n{} Descartando propuesta para incidente «{}»...", paint("antOS Autopilot ·", BOLD), incident_id);
            let _inc = autopilot::AutopilotEngine::resolve_incident(&ctx.state, &ctx.workspace, incident_id, false)?;
            println!("  ✓ {} Incidente descartado.\n", paint("DESCARTADO", DIM));
        }

        "status" | _ => {
            let st = autopilot::AutopilotEngine::status(&ctx.state, &ctx.workspace)?;
            let status_badge = if st.active { paint("ACTIVO (Vigilando)", GREEN) } else { paint("DETENIDO", RED) };
            println!("\n{} Estado del Centinela Autónomo:", paint("antOS Autopilot ·", BOLD));
            println!("  • Estado:                 {}", status_badge);
            println!("  • Espacio de Trabajo:     {}", st.workspace_path);
            println!("  • Intervalo de Sondeo:    {}s", st.poll_interval_secs);
            println!("  • Incidencias Activas:    {}", paint(&st.active_incidents_count.to_string(), if st.active_incidents_count > 0 { YELLOW } else { GREEN }));
            println!("  • Incidencias Resueltas:  {}", st.resolved_incidents_count);
            if let Some(ts) = st.last_scan_timestamp {
                println!("  • Último Escaneo:         {}", ts);
            }
            println!("\n  Uso:");
            println!("    antos autopilot start [--interval <secs>] [--auto-merge]  Arranca el centinela");
            println!("    antos autopilot stop                                      Detiene el centinela");
            println!("    antos autopilot status                                    Muestra métricas y estado");
            println!("    antos autopilot scan                                      Escaneo manual reactivo");
            println!("    antos autopilot list                                      Historial de incidentes");
            println!("    antos autopilot approve <incident_id>                     Aprueba y aplica fix");
            println!("    antos autopilot reject <incident_id>                      Descarta la propuesta\n");
        }
    }
    Ok(())
}

// ----------------------------------------------------- web console (T16.4)

fn cmd_web(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "start" => {
            let mut bind = "127.0.0.1".to_string();
            let mut port = 8088u16;
            let mut i = 1;
            while i < args.len() {
                if (args[i] == "--bind" || args[i] == "-b") && i + 1 < args.len() {
                    bind = args[i + 1].clone();
                    i += 2;
                } else if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
                    if let Ok(p) = args[i + 1].parse::<u16>() {
                        port = p;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }

            let config = antos_protocol::WebConsoleConfig {
                bind_addr: bind,
                port,
                auth_required: true,
                ws_ping_interval_secs: 30,
            };

            println!("\n{} Iniciando consola web remota y bridge WebSocket...", paint("antOS Web Console ·", BOLD));
            let st = web::WebEngine::start(&ctx.state, &ctx.workspace, config)?;
            let token_sess = web::WebEngine::generate_token(&ctx.state, Some("admin".into()), Some(86400))?;
            println!("  Estado:               {}", paint("ACTIVO (En línea)", GREEN));
            println!("  URL de Acceso:        {}", paint(&st.url, CYAN));
            println!("  URL con Token:        {}", paint(&format!("{}?token={}", st.url, token_sess.token), BOLD));
            println!("  WebSocket Bridge:     {}/ws/events\n", st.url);
        }

        "stop" => {
            println!("\n{} Deteniendo servidor de consola web...", paint("antOS Web Console ·", BOLD));
            let st = web::WebEngine::stop(&ctx.state)?;
            println!("  Estado:               {}\n", paint(if st.running { "ACTIVO" } else { "DETENIDO" }, RED));
        }

        "token" => {
            let mut label = None;
            let mut ttl = None;
            let mut i = 1;
            while i < args.len() {
                if (args[i] == "--label" || args[i] == "-l") && i + 1 < args.len() {
                    label = Some(args[i + 1].clone());
                    i += 2;
                } else if (args[i] == "--ttl" || args[i] == "-t") && i + 1 < args.len() {
                    if let Ok(v) = args[i + 1].parse::<u64>() {
                        ttl = Some(v);
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }

            println!("\n{} Generando token de autenticación...", paint("antOS Web Console ·", BOLD));
            let session = web::WebEngine::generate_token(&ctx.state, label, ttl)?;
            let st = web::WebEngine::status(&ctx.state)?;
            println!("  Token:                {}", paint(&session.token, BOLD));
            println!("  Expira en:            {}s", session.expires_at.saturating_sub(session.created_at));
            if let Some(ref l) = session.client_label {
                println!("  Cliente / Dispositivo: {}", l);
            }
            println!("  Enlace de Conexión:   {}\n", paint(&format!("{}?token={}", st.url, session.token), CYAN));
        }

        "status" | _ => {
            let st = web::WebEngine::status(&ctx.state)?;
            let status_badge = if st.running { paint("ACTIVO (En línea)", GREEN) } else { paint("DETENIDO", RED) };
            println!("\n{} Estado del Servidor Web y Bridge WebSocket:", paint("antOS Web Console ·", BOLD));
            println!("  • Estado:                 {}", status_badge);
            println!("  • Dirección y Puerto:     {}:{}", st.bind_addr, st.port);
            println!("  • URL de Acceso:          {}", paint(&st.url, CYAN));
            println!("  • Clientes Conectados:    {}", st.connected_clients);
            println!("  • Sesiones Token Activas: {}", st.active_sessions_count);
            println!("\n  Uso:");
            println!("    antos web start [--port <puerto>] [--bind <ip>]  Inicia el servidor web");
            println!("    antos web stop                                  Detiene el servidor");
            println!("    antos web status                                Muestra estado y métricas");
            println!("    antos web token [--label <nombre>] [--ttl <s]>  Genera enlace seguro\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ tickets

fn render_project_tickets(proj_path: &std::path::Path, proj_name: &str, engine: &spec::SpecEngine) -> Result<()> {
    if !proj_path.exists() {
        println!(
            "\n{} El proyecto «{}» no existe en el workspace ({}).\n",
            paint("✗", RED),
            paint(proj_name, BOLD),
            paint(&proj_path.display().to_string(), DIM)
        );
        return Ok(());
    }

    let tickets = engine.listar_tickets(proj_path)?;
    if tickets.is_empty() {
        println!(
            "\n{} {}\n",
            paint("antOS · Catálogo de Tickets", BOLD),
            paint(&format!("— Proyecto: «{proj_name}»"), CYAN)
        );
        println!("  (no se encontraron tickets definidos en este proyecto)");
        println!("  Crea el primer ticket con:\n");
        println!("    antos ticket new T1.1 \"Título del Ticket\" --project {proj_name}\n");
        return Ok(());
    }

    println!(
        "\n{} {}\n",
        paint("antOS · Catálogo de Tickets", BOLD),
        paint(&format!("— Proyecto: «{proj_name}»"), CYAN)
    );
    println!(
        "  {:<8} {:<8} {:<55} {}",
        paint("FASE", DIM),
        paint("ID", DIM),
        paint("TÍTULO", DIM),
        paint("ESTADO", DIM)
    );
    println!("  {}", "─".repeat(88));

    let mut completados = 0;
    for t in &tickets {
        if t.status == antos_protocolo::TicketStatus::Completado {
            completados += 1;
        }
        println!(
            "  {:<8} {:<8} {:<55} {}",
            paint(&t.phase, DIM),
            paint(&t.id, BOLD),
            ellipsis(&t.title, 53),
            t.status.etiqueta()
        );
    }
    println!("  {}", "─".repeat(88));
    println!(
        "  Total: {} tickets | {} completados | {} pendientes\n",
        tickets.len(),
        completados,
        tickets.len() - completados
    );
    Ok(())
}

fn cmd_tickets(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = spec::SpecEngine::global();

    // 1. Extraer flags globales de proyecto o sistema: --project <P>, -p <P>, --system, --os
    let mut project_flag: Option<String> = None;
    let mut is_system = false;
    let mut clean_args: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--project" | "-p" => {
                if i + 1 < args.len() {
                    project_flag = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
            }
            "--system" | "--os" => {
                is_system = true;
                i += 1;
                continue;
            }
            _ => {
                clean_args.push(args[i].clone());
                i += 1;
            }
        }
    }

    let sub = clean_args.first().map(String::as_str);

    // 2. Resolver directorio objetivo base
    let default_target_ws = if is_system {
        ctx.antos_root.clone().unwrap_or_else(|| ctx.workspace.clone())
    } else if let Some(ref p) = project_flag {
        ctx.workspace.join(p)
    } else if let Some(ref cur) = ctx.current_project {
        cur.clone()
    } else {
        ctx.workspace.clone()
    };

    match sub {
        Some("new" | "create" | "add") => {
            let id = clean_args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos ticket new <ID> <Título> [--project <proyecto>] [--fase \"...\"] [--desc \"...\"]"
                )
            })?;
            let title = clean_args.get(2).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos ticket new <ID> <Título> [--project <proyecto>] [--fase \"...\"] [--desc \"...\"]"
                )
            })?;

            let phase = clean_args
                .iter()
                .position(|a| a == "--fase" || a == "-f")
                .and_then(|idx| clean_args.get(idx + 1))
                .cloned();

            let desc = clean_args
                .iter()
                .position(|a| a == "--desc" || a == "-d")
                .and_then(|idx| clean_args.get(idx + 1))
                .cloned();

            let path = engine.create_ticket(
                &default_target_ws,
                id,
                title,
                desc.as_deref(),
                phase.as_deref(),
            )?;

            let scope_name = default_target_ws
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "sistema".to_string());

            println!(
                "\n{} Ticket {} creado exitosamente para «{}» en {}.\n",
                paint("✓", GREEN),
                paint(id, BOLD),
                paint(&scope_name, CYAN),
                paint(&path.display().to_string(), DIM)
            );
            return Ok(());
        }
        Some("status" | "set-status") => {
            let id = clean_args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos ticket status <ID> <completado|progreso|revision|pendiente> [--project <proyecto>]"
                )
            })?;
            let status_raw = clean_args.get(2).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos ticket status <ID> <completado|progreso|revision|pendiente> [--project <proyecto>]"
                )
            })?;
            let st = match status_raw.to_lowercase().as_str() {
                "completado" | "done" | "hecho" => antos_protocolo::TicketStatus::Completado,
                "progreso" | "en_progreso" | "in_progress" => {
                    antos_protocolo::TicketStatus::EnProgreso
                }
                "revision" | "revisión" | "review" => antos_protocolo::TicketStatus::EnRevision,
                _ => antos_protocolo::TicketStatus::Pendiente,
            };
            engine.update_ticket_status(&default_target_ws, id, st)?;
            let scope_name = default_target_ws
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "sistema".to_string());

            println!(
                "\n{} Estado del ticket {} en «{}» actualizado a {}.\n",
                paint("✓", GREEN),
                paint(id, BOLD),
                paint(&scope_name, CYAN),
                st.etiqueta()
            );
            return Ok(());
        }
        Some(arg) if arg != "list" => {
            let is_ticket_id = (arg.starts_with('T') || arg.starts_with('t'))
                && arg.chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(false);

            if is_ticket_id {
                let detalle = engine.obtener_ticket(&default_target_ws, arg)?;
                match detalle {
                    Some(t) => {
                        println!(
                            "\n{} {}  {}",
                            paint(&t.id, BOLD),
                            paint(&t.phase, DIM),
                            t.status.etiqueta()
                        );
                        println!("{}", paint(&t.title, BOLD));
                        println!();
                        println!("{}", paint("Descripción:", BOLD));
                        println!("  {}", t.description);
                        if !t.technical_scope.is_empty() {
                            println!();
                            println!("{}", paint("Alcance Técnico:", BOLD));
                            for a in &t.technical_scope {
                                println!("  • {a}");
                            }
                        }
                        if !t.acceptance_criteria.is_empty() {
                            println!();
                            println!("{}", paint("Criterios de Aceptación:", BOLD));
                            for c in &t.acceptance_criteria {
                                println!("  • {c}");
                            }
                        }
                        println!();
                    }
                    None => {
                        let scope_name = default_target_ws
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "catálogo".to_string());
                        println!("\nticket '{arg}' no encontrado en el ámbito «{scope_name}».\n");
                    }
                }
                return Ok(());
            } else if arg == "system" || arg == "os" {
                is_system = true;
            } else {
                // Es el nombre de un proyecto: `antos tickets api-service`
                let proj_target = ctx.workspace.join(arg);
                return render_project_tickets(&proj_target, arg, engine);
            }
        }
        _ => {}
    }

    // Listar tickets
    if let Some(ref p) = project_flag {
        let proj_target = ctx.workspace.join(p);
        return render_project_tickets(&proj_target, p, engine);
    }

    if !is_system {
        if let Some(ref cur) = ctx.current_project {
            let proj_name = cur.file_name().and_then(|n| n.to_str()).unwrap_or("proyecto");
            return render_project_tickets(cur, proj_name, engine);
        }
    }

    // Si el usuario está ejecutando dentro de `workspace/` (raíz del workspace)
    let cwd_canon = std::env::current_dir().ok().and_then(|c| c.canonicalize().ok());
    let ws_canon = ctx.workspace.canonicalize().ok();
    let in_workspace_root = cwd_canon == ws_canon
        && ctx.workspace.file_name().map(|n| n == "workspace").unwrap_or(false);

    if !is_system && in_workspace_root {
        let projects = crate::exec::scan_workspace_projects(&ctx.workspace);
        println!(
            "\n{}\n",
            paint("antOS · Catálogo de Tickets por Proyecto en Workspace (T17.4)", BOLD)
        );
        if projects.is_empty() {
            println!("  (no hay proyectos inicializados en el workspace)");
            println!("  Crea un nuevo proyecto con: antos project init <nombre>\n");
            return Ok(());
        }

        for p in &projects {
            let p_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("proyecto");
            let p_tickets = engine.listar_tickets(p).unwrap_or_default();
            if p_tickets.is_empty() {
                println!("  • {:<20} (sin catálogo de tickets)", paint(p_name, CYAN));
            } else {
                let comp = p_tickets.iter().filter(|t| t.status == antos_protocolo::TicketStatus::Completado).count();
                let pend = p_tickets.len() - comp;
                println!(
                    "  • {:<20} {} tickets ({} completados, {} pendientes)",
                    paint(p_name, CYAN),
                    p_tickets.len(),
                    comp,
                    pend
                );
            }
        }
        println!("\n  Comandos:");
        println!("    antos tickets <proyecto>               ver tickets de un proyecto");
        println!("    antos ticket new <ID> <Título> -p <p>  crear ticket en proyecto");
        println!("    antos tickets --system                 ver tickets del sistema antOS\n");
        return Ok(());
    }

    let target = if is_system {
        ctx.antos_root.clone().unwrap_or_else(|| ctx.workspace.clone())
    } else {
        ctx.workspace.clone()
    };

    let tickets = engine.listar_tickets(&target)?;
    if tickets.is_empty() {
        println!("\nno se encontraron tickets en el sistema operativo.");
        println!("Crea uno con: antos ticket new <ID> <Título>\n");
        return Ok(());
    }

    println!(
        "\n{}",
        paint("antOS · Catálogo y Hoja de Ruta de Tickets (Sistema Operativo)", BOLD)
    );
    println!();
    println!(
        "  {:<8} {:<8} {:<55} {}",
        paint("FASE", DIM),
        paint("ID", DIM),
        paint("TÍTULO", DIM),
        paint("ESTADO", DIM)
    );
    println!("  {}", "─".repeat(88));

    let mut completados = 0;
    for t in &tickets {
        if t.status == antos_protocolo::TicketStatus::Completed {
            completados += 1;
        }
        println!(
            "  {:<8} {:<8} {:<55} {}",
            paint(&t.phase, DIM),
            paint(&t.id, BOLD),
            ellipsis(&t.title, 53),
            t.status.etiqueta()
        );
    }
    println!("  {}", "─".repeat(88));
    println!(
        "  Total: {} tickets | {} completados | {} pendientes\n",
        tickets.len(),
        completados,
        tickets.len() - completados
    );
    Ok(())
}

fn cmd_ports(args: &[String]) -> Result<()> {
    let filtro = args.first().and_then(|a| a.parse::<u16>().ok());
    let puertos = net::diagnosticar_puertos(filtro)?;

    println!(
        "\n{}",
        paint("antOS · Diagnóstico de Puertos y Procesos", BOLD)
    );
    if puertos.is_empty() {
        if let Some(p) = filtro {
            println!(
                "  El puerto {} está libre.\n",
                paint(&format!(":{p}"), GREEN)
            );
        } else {
            println!("  No se detectaron puertos de desarrollo en escucha activa.\n");
        }
        return Ok(());
    }

    println!(
        "\n  {:<8} {:<8} {:<16} {:<32} CARPETA",
        paint("PUERTO", DIM),
        paint("PID", DIM),
        paint("PROCESO", DIM),
        paint("COMANDO", DIM)
    );
    println!("  {}", "─".repeat(88));

    for p in &puertos {
        let puerto_fmt = format!(":{}", p.port);
        let dir_fmt = p.working_dir.as_deref().unwrap_or("-");
        let cmd_recortado = if p.command.len() > 30 {
            format!("{}…", &p.command[..29])
        } else {
            p.command.clone()
        };

        println!(
            "  {:<8} {:<8} {:<16} {:<32} {}",
            paint(&puerto_fmt, GREEN),
            paint(&p.pid.to_string(), YELLOW),
            p.process_name,
            cmd_recortado,
            paint(dir_fmt, DIM)
        );
    }

    println!("  {}", "─".repeat(88));
    println!("  Total: {} proceso(s) en escucha\n", puertos.len());
    Ok(())
}

fn cmd_agent(ctx: &Ctx, args: &[String]) -> Result<()> {
    if args.is_empty() || args[0] == "list" || args[0] == "roles" {
        println!(
            "\n{}",
            paint("antOS · Roles de Agentes Especializados (antFlow)", BOLD)
        );
        let roles = [
            antos_protocolo::AgentRole::Arquitecto,
            antos_protocolo::AgentRole::Coder,
            antos_protocolo::AgentRole::QA,
            antos_protocolo::AgentRole::Auditor,
        ];
        for r in roles {
            println!("\n  {} {}", paint("●", GREEN), paint(r.name(), BOLD));
            println!("    {}", paint(r.description(), DIM));
            println!(
                "    {}",
                paint(&format!("Directive: {}", r.system_prompt()), DIM)
            );
        }
        println!();
        return Ok(());
    }

    match args[0].as_str() {
        "run" => {
            let ticket_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos agent run <ticket_id> [--auto]"))?;
            let auto = args.iter().any(|a| a == "--auto" || a == "-a");
            let node_target = args
                .iter()
                .position(|a| a == "--node" || a == "-n" || a == "--remote")
                .and_then(|i| args.get(i + 1));

            println!(
                "\n{}",
                paint(
                    &format!("antOS · Orquestador antFlow para {ticket_id}"),
                    BOLD
                )
            );

            if let Some(target) = node_target {
                println!(
                    "  {} Despachando rol a nodo remoto Swarm: {}",
                    paint("🌐", CYAN),
                    paint(target, BOLD)
                );
                let _ = distributed::SwarmEngine::global().dispatch_remote_role(
                    &ctx.workspace,
                    ticket_id,
                    antos_protocolo::AgentRole::Coder,
                    Some(target),
                )?;
            }

            let engine = flow::FlowEngine::global();

            let task = if auto {
                println!(
                    "  {} Ejecutando pipeline automatizado de agentes...",
                    paint("▶", GREEN)
                );
                engine.ejecutar_pipeline_worktree(&ctx.workspace, &ctx.state, ticket_id, &[])?
            } else {
                engine.iniciar_tarea(&ctx.workspace, &ctx.state, ticket_id)?
            };

            println!("  Tarea ID:       {}", paint(&task.id, YELLOW));
            println!("  Ticket:         {}", paint(&task.ticket_id, BOLD));
            println!("  Estado:         {}", task.state.label());
            if let Some(wt) = &task.worktree_path {
                println!("  Worktree:       {}", paint(wt, DIM));
            }
            if let Some(br) = &task.branch_name {
                println!("  Rama de Agente: {}", paint(br, GREEN));
            }
            if let Some(resumen) = &task.audit_summary {
                println!("  Auditoría:      {}", paint(resumen, GREEN));
            }

            println!(
                "\n  {}",
                paint("Historial de Transiciones de Agentes:", BOLD)
            );
            for t in &task.history {
                let rol_fmt = t
                    .role
                    .map(|r| format!(" [{}]", r.nombre()))
                    .unwrap_or_default();
                println!(
                    "    • {}{}: {}",
                    paint(t.new_state.label(), BOLD),
                    paint(&rol_fmt, DIM),
                    t.detail
                );
            }

            if let Some(diff) = &task.diff_preview {
                if !diff.is_empty() {
                    println!(
                        "\n  {}",
                        paint("Previsualización de Diff Consolidado:", BOLD)
                    );
                    println!("    {}", diff.replace('\n', "\n    "));
                }
            }

            println!("\n  {} Tarea procesada correctamente.\n", paint("✓", GREEN));
        }
        "status" => {
            let ticket_id = args.get(1);
            let engine = flow::FlowEngine::global();
            if let Some(tid) = ticket_id {
                if let Some(task) = engine.consultar_tarea(tid) {
                    println!(
                        "\n{}",
                        paint(
                            &format!("antOS · Estado de Tarea antFlow [{}]", task.ticket_id),
                            BOLD
                        )
                    );
                    println!("  Estado:     {}", task.state.label());
                    println!(
                        "  Rol Activo: {}",
                        task.current_role.map(|r| r.nombre()).unwrap_or("Ninguno")
                    );
                    if let Some(wt) = &task.worktree_path {
                        println!("  Worktree:   {}", paint(wt, DIM));
                    }
                    if let Some(br) = &task.branch_name {
                        println!("  Rama:       {}", paint(br, GREEN));
                    }
                    if let Some(diff) = &task.diff_preview {
                        println!("\n  Previsualización Diff:\n    {diff}");
                    }
                    println!("\n  Transiciones:");
                    for h in &task.history {
                        println!("    • [{}] {}", h.new_state.label(), h.detail);
                    }
                    println!();
                } else {
                    println!("\n  No hay tarea activa para el ticket «{tid}».\n");
                }
            } else {
                let tasks = engine.listar_tareas();
                println!("\n{}", paint("antOS · Tareas antFlow", BOLD));
                if tasks.is_empty() {
                    println!("  No hay tareas en curso.\n");
                } else {
                    for t in tasks {
                        println!(
                            "  • {:<8} {:<30} (reintentos QA: {})",
                            paint(&t.ticket_id, BOLD),
                            t.state.label(),
                            t.qa_retries
                        );
                    }
                    println!();
                }
            }
        }
        "swarm" => {
            cmd_swarm(ctx, &args[1..])?;
        }
        _ => {
            bail!("subcomando desconocido para agent. Usa: antos agent run <ticket_id> [--node <id>] | antos agent status [ticket_id] | antos agent swarm | antos agents");
        }
    }
    Ok(())
}

fn cmd_services(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "up" | "start" => {
            let svc = args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "debes especificar el nombre del servicio (ej. antos service up postgres)"
                )
            })?;
            let port = args.get(2).and_then(|p| p.parse::<u16>().ok());
            let db = args.get(3).map(String::as_str);

            println!(
                "\n{} Aprovisionando servicio efímero «{}»...",
                paint("⚡", BOLD),
                paint(svc, YELLOW)
            );
            let info = service::start_service(svc, port, db, &ctx.state, &ctx.workspace)?;
            println!(
                "  {} Servicio:      {}",
                paint("●", GREEN),
                paint(&info.name, BOLD)
            );
            println!(
                "  {} Puerto:        {}",
                paint("●", GREEN),
                paint(&info.port.to_string(), YELLOW)
            );
            println!(
                "  {} Estado:        {}",
                paint("●", GREEN),
                paint(&info.status, GREEN)
            );
            println!(
                "  {} Variable .env: {}={}",
                paint("●", GREEN),
                paint(&info.env_var_key, BOLD),
                paint(&info.env_var_value, CYAN)
            );
            println!(
                "  {} Almacenamiento: {}\n",
                paint("●", GREEN),
                paint(&info.data_dir, DIM)
            );
        }
        "down" | "stop" => {
            let svc = args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "debes especificar el nombre del servicio (ej. antos service down postgres)"
                )
            })?;
            service::stop_service(svc, &ctx.state)?;
            println!(
                "\n{} Servicio «{}» detenido y limpiado.\n",
                paint("✓", GREEN),
                paint(svc, BOLD)
            );
        }
        "status" | "list" | _ => {
            let svc_filter = if sub != "status" && sub != "list" {
                Some(sub)
            } else {
                args.get(1).map(String::as_str)
            };

            let services = service::get_service_status(svc_filter, &ctx.state)?;
            println!(
                "\n{}",
                paint(
                    "antOS · Servicios Locales Efímeros de Desarrollo (T5.1)",
                    BOLD
                )
            );
            if services.is_empty() {
                println!("  No hay servicios efímeros aprovisionados.");
                println!(
                    "  Inicia uno con: antos service up <postgres|redis|mariadb|meilisearch>\n"
                );
            } else {
                println!("  ┌────────────────┬────────┬───────────┬─────────────────────────────────────────────────────────┐");
                println!(
                    "  │ {:<14} │ {:<6} │ {:<9} │ {:<55} │",
                    paint("SERVICIO", BOLD),
                    paint("PUERTO", BOLD),
                    paint("ESTADO", BOLD),
                    paint("VARIABLE DE ENTORNO (.env)", BOLD)
                );
                println!("  ├────────────────┼────────┼───────────┼─────────────────────────────────────────────────────────┤");
                for s in services {
                    let st_fmt = if s.status == "running" {
                        paint("● running", GREEN)
                    } else {
                        paint("○ stopped", DIM)
                    };
                    println!(
                        "  │ {:<14} │ {:<6} │ {:<20} │ {}={} │",
                        s.name,
                        s.port,
                        st_fmt,
                        paint(&s.env_var_key, BOLD),
                        ellipsis(&s.env_var_value, 38)
                    );
                }
                println!("  └────────────────┴────────┴───────────┴─────────────────────────────────────────────────────────┘\n");
            }
        }
    }
    Ok(())
}

fn cmd_panel(ctx: &Ctx, args: &[String]) -> Result<()> {
    if let Some(pos) = args.iter().position(|a| a == "--dispatch" || a == "-d") {
        if let Some(target_ticket) = args.get(pos + 1) {
            println!(
                "\n{} Despachando ticket {} al equipo multi-agente antFlow...",
                paint("🚀", BOLD),
                paint(target_ticket, YELLOW)
            );
            let run_args = vec![
                "run".to_string(),
                target_ticket.clone(),
                "--auto".to_string(),
            ];
            return cmd_agent(ctx, &run_args);
        }
    }

    let spec_engine = spec::SpecEngine::global();
    let target_ws = ctx.current_project.as_deref().unwrap_or(&ctx.workspace);
    let tickets = spec_engine.list_tickets(target_ws)?;
    let flow_engine = flow::FlowEngine::global();
    let tasks = flow_engine.list_tasks();

    let banner_text = if let Some(ref cur) = ctx.current_project {
        let name = cur.file_name().and_then(|n| n.to_str()).unwrap_or("proyecto");
        format!("antOS · KANBAN - PROYECTO: {} (Super + A)", name)
    } else {
        "antOS · CENTRO DE CONTROL DE AGENTES Y TABLERO KANBAN (Super + A)".to_string()
    };

    println!("\n{}", paint("╔══════════════════════════════════════════════════════════════════════════════════════╗", BOLD));
    println!("║ {:^84} ║", paint(&banner_text, BOLD));
    println!("{}\n", paint("╚══════════════════════════════════════════════════════════════════════════════════════╝", BOLD));

    // Monitor de Agentes
    println!(
        "  {}",
        paint("● MONITOR DE AGENTES ACTIVOS (antFlow)", BOLD)
    );
    let roles = [
        ("📐 Arquitecto", antos_protocolo::AgentRole::Architect),
        ("💻 Coder", antos_protocolo::AgentRole::Coder),
        ("🧪 QA / Tester", antos_protocolo::AgentRole::QA),
        ("🛡️ Auditor", antos_protocolo::AgentRole::Auditor),
    ];

    for (etiqueta_rol, rol) in roles {
        let active_tasks: Vec<_> = tasks.iter().filter(|t| t.current_role == Some(rol)).collect();
        if active_tasks.is_empty() {
            println!(
                "    {} {:<18} {}",
                paint("○", DIM),
                etiqueta_rol,
                paint("[Inactivo / En espera]", DIM)
            );
        } else {
            for t in active_tasks {
                println!(
                    "    {} {:<18} {} → Tarea: {} ({})",
                    paint("●", GREEN),
                    paint(etiqueta_rol, BOLD),
                    paint(t.state.label(), YELLOW),
                    paint(&t.ticket_id, BOLD),
                    t.worktree_path.as_deref().unwrap_or("sandbox")
                );
            }
        }
    }
    println!();

    // Columnas Kanban
    let pendientes: Vec<_> = tickets
        .iter()
        .filter(|t| t.status == antos_protocolo::TicketStatus::Pending)
        .collect();
    let en_progreso: Vec<_> = tickets
        .iter()
        .filter(|t| t.status == antos_protocolo::TicketStatus::InProgress)
        .collect();
    let en_revision: Vec<_> = tickets
        .iter()
        .filter(|t| t.status == antos_protocolo::TicketStatus::InReview)
        .collect();
    let completados: Vec<_> = tickets
        .iter()
        .filter(|t| t.status == antos_protocolo::TicketStatus::Completed)
        .collect();

    println!("  {}", paint("● TABLERO DE TICKETS (docs/tickets/)", BOLD));
    println!("  ┌────────────────────────┬────────────────────────┬────────────────────────┬────────────────────────┐");
    let hdr_backlog = format!("⏳ BACKLOG ({})", pendientes.len());
    let hdr_progreso = format!("🔄 EN CURSO ({})", en_progreso.len());
    let hdr_revision = format!("🔍 REVISIÓN ({})", en_revision.len());
    let hdr_hecho = format!("✅ HECHO ({})", completados.len());
    println!(
        "  │ {:<22} │ {:<22} │ {:<22} │ {:<22} │",
        paint(&hdr_backlog, BOLD),
        paint(&hdr_progreso, BOLD),
        paint(&hdr_revision, BOLD),
        paint(&hdr_hecho, BOLD)
    );
    println!("  ├────────────────────────┼────────────────────────┼────────────────────────┼────────────────────────┤");

    let max_filas = [
        pendientes.len(),
        en_progreso.len(),
        en_revision.len(),
        completados.len(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);

    for i in 0..max_filas {
        let col1 = pendientes
            .get(i)
            .map(|t| format!("{} {}", t.id, ellipsis(&t.title, 14)))
            .unwrap_or_default();
        let col2 = en_progreso
            .get(i)
            .map(|t| format!("{} {}", t.id, ellipsis(&t.title, 14)))
            .unwrap_or_default();
        let col3 = en_revision
            .get(i)
            .map(|t| format!("{} {}", t.id, ellipsis(&t.title, 14)))
            .unwrap_or_default();
        let col4 = completados
            .get(i)
            .map(|t| format!("{} {}", t.id, ellipsis(&t.title, 14)))
            .unwrap_or_default();

        println!(
            "  │ {:<22} │ {:<22} │ {:<22} │ {:<22} │",
            col1, col2, col3, col4
        );
    }
    println!("  └────────────────────────┴────────────────────────┴────────────────────────┴────────────────────────┘");

    println!(
        "\n  {} Usa {} para despachar un ticket al equipo de agentes.",
        paint("💡", YELLOW),
        paint("antos panel --dispatch <TID>", BOLD)
    );
    println!(
        "  {} Usa {} para lanzar la interfaz gráfica Wayland/GTK4.\n",
        paint("🖥️", BOLD),
        paint("antos-barra", BOLD)
    );

    Ok(())
}

fn help() {
    println!(
        "\
antOS — el sistema hace lo que le pides, y puedes deshacerlo

  antos \"<intención>\"       planifica, enseña el diff y ejecuta
  antos escucha              lo mismo, dictado por voz (transcripción local)
  antos panel                centro de control de agentes y tablero Kanban (Super + A)
  antos tickets [id]         catálogo de tickets y especificaciones
  antos agent run <id>       ejecuta ticket con orquestación multi-agente
  antos agent status [id]    consulta estado y traza de agentes
  antos agents               lista los roles especializados y sus directivas
  antos ports [puerto]       diagnóstico de puertos de red y procesos
  antos services             gestión de servicios efímeros (postgres, redis, mysql)
  antos caps                 catálogo de capacidades y su nivel
  antos log                  bitácora de lo que ha pasado
  antos undo [--ticket id]   revierte el último plan o todos los cambios de un ticket
  antos doctor               comprueba que el recinto es real, atacándolo
  antos diff [proyecto] [ref] visor interactivo de diffs y parches por proyecto
  antos project init <nombre> inicializa repositorio Git aislado y .gitignore en workspace
  antos project list         lista los proyectos y su estado de control de versiones
  antos grant <cap> [--minutos N]
  antos revoke <cap>

opciones
  -p, --planificador <local|claude>
  -s, --si                   no preguntar confirmación
  -n, --seco                 planificar y previsualizar sin ejecutar
      --segundos <N>         escucha: cuánto grabar (por defecto 5)
      --desde <fichero>      escucha: transcribir un audio en vez del micrófono
      --dispositivos         escucha: listar las entradas de audio
      --dispositivo <N>      escucha: cuál usar (por defecto, la del sistema)

entorno
  ANTOS_WORKSPACE  espacio de trabajo (por defecto ./workspace)
  ANTOS_STATE      instantáneas y bitácora (por defecto ./.antos)
  ANTHROPIC_API_KEY  activa el planificador con Claude"
    );
}
