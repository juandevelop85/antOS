//! syso — el recorrido de una intención.
//!
//! captura → planificación → radio de impacto → política → instantánea y
//! ejecución → registro. El modelo solo participa en la planificación, y su
//! salida se valida entera antes de que nadie la vea.

mod blast;
mod capability;
mod ctx;
mod exec;
mod grants;
pub mod git;
pub mod net;
pub mod spec;
pub mod env;
pub mod flow;
pub mod memory;
pub mod service;
pub mod vault;
pub mod diff_view;
pub mod vte;
pub mod notification;
pub mod mesh;
pub mod distributed;
pub mod vfs;
pub mod vfs_guard;
pub mod ebpf;
pub mod profiler;
mod ipc;
mod journal;
mod plan;
mod planner;
mod preview;
mod protocolo;
mod sandbox;
mod sesion;
mod snapshot;
mod terminal;
mod voz;

use anyhow::{bail, Result};
use capability::{Catalog, Tier};
use ctx::Ctx;
use grants::Grants;
use journal::{Outcome, Record};
use planner::{claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner, Planner};
use terminal::{ellipsis, paint, tier_color, BOLD, DIM, GREEN, RED, YELLOW, BLUE, CYAN};

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
            "--si" | "-s" => opts.assume_yes = true,
            "--seco" | "-n" => opts.dry_run = true,
            "--planificador" | "-p" => opts.planner = argv.next(),
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
    sesion::intencion(ctx, catalog, intent, &*planificador, opts.dry_run, &mut terminal)
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
        println!("{}", paint("elige uno con: antos escucha --dispositivo N", DIM));
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
                paint(&format!("grabando {segundos} s desde {dispositivo} · habla ahora"), BOLD)
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
        println!("\n{} {}", paint("antOS · Reversión granular de ticket", BOLD), paint(&tid, YELLOW));

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
                println!("  {} Worktree efímero ({}) limpiado.", paint("✓", GREEN), wt_path.display());
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

            println!("  {} Transacción {}: «{}»", paint("↩", DIM), paint(&records[idx].id, DIM), records[idx].intent);
            for line in snapshot::restore(&snap)? {
                println!("    {line}");
            }

            records[idx].reverted = true;
            let undone = Record {
                id: plan::nuevo_id(),
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
            println!("  {} Worktree efímero ({}) eliminado.", paint("✓", GREEN), wt_path.display());
        }

        journal::rewrite(&ctx.journal_path(), &records)?;
        println!("\n  {} Reversión completada: {} transacción(es) revertida(s).\n", paint("✓", GREEN), reversiones);
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
    println!("{} {}", paint("deshaciendo", BOLD), paint(&format!("«{}»", records[idx].intent), DIM));
    for line in snapshot::restore(&snap)? {
        println!("  {line}");
    }

    records[idx].reverted = true;
    let undone = Record {
        id: plan::nuevo_id(),
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
            nivel.push_str(if grants.is_granted(&cap.name) { " · concedida" } else { " · denegada" });
        }
        println!();
        println!("  {}  {}", paint(&cap.name, BOLD), paint(&nivel, tier_color(cap.policy.tier)));
        println!("    {}", paint(&cap.summary, DIM));
        let params = cap
            .params
            .iter()
            .map(|(n, s)| if s.optional || s.default.is_some() { format!("[{n}]") } else { n.clone() })
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
        marca(true, "escritura fuera de lo declarado: la deniega el kernel");
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
        marca(false, "escritura dentro de lo declarado: BLOQUEADA (el recinto es demasiado estrecho)");
    }

    // 3) leer fuera de lo declarado. Es la garantía que separa a Landlock de
    //    Seatbelt, así que se pregunta al motor qué promete antes de juzgar.
    let secreto = ctx.state.join("doctor-secreto.txt");
    std::fs::write(&secreto, "credencial de mentira")?;
    let lectura = sandbox::run(
        &*jail,
        &[exec::Change::Read { path: secreto.clone() }],
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
            paint("lectura fuera de lo declarado: este motor no confina lecturas", DIM)
        ),
    }

    // 4) red. Se prueba en los dos sentidos para no confundir «bloqueada»
    //    con «esta máquina no tiene internet».
    let con_red = sandbox::Policy { writes: vec![], reads: vec![], dirs: vec![], network: true, allowed_secrets: vec![], quota: None };
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
            paint("red: no concluyente — esta máquina no llega a internet", DIM)
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
    for r in records.iter().rev().take(20).collect::<Vec<_>>().into_iter().rev() {
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
            bail!("{cap_name} es de nivel «{}»: no necesita concesión", cap.policy.tier.label());
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
    println!("\n{} Concesión revocada: «{}».\n", paint("🔒 REVOCADA", DIM), paint(cap_name, BOLD));
    Ok(())
}

fn cmd_secrets(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let grants = Grants::load(&ctx.grants_path())?;

    match sub {
        "set" => {
            let key = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos secret set <CLAVE> <VALOR>"))?;
            let val = args.get(2).ok_or_else(|| anyhow::anyhow!("uso: antos secret set <CLAVE> <VALOR>"))?;
            vault::set_secret(&ctx.state, key, val)?;
            println!("\n{} Secreto «{}» almacenado de forma segura en la bóveda de antOS.\n", paint("✓", GREEN), paint(key, BOLD));
        }
        "get" => {
            let key = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos secret get <CLAVE>"))?;
            match vault::get_secret(&ctx.state, key, &grants) {
                Ok(Some(v)) => {
                    println!("\n{} {key} = {}\n", paint("🔑", BOLD), paint(&v, GREEN));
                }
                Ok(None) => {
                    println!("\n{} El secreto «{key}» no existe en la bóveda.\n", paint("○", DIM));
                }
                Err(e) => {
                    println!("\n{} {e}\n", paint("🛡️ Cero Autoridad Ambiental (Bloqueado):", RED));
                }
            }
        }
        "list" | _ => {
            let list = vault::list_secrets(&ctx.state)?;
            let active_grants = grants.list_active();

            println!("\n{}", paint("antOS · Bóveda de Secretos y Blindaje Zero Environmental Authority (T5.2)", BOLD));
            
            // Concesiones activas
            println!("  {}", paint("● CONCESIONES ACTIVAS", BOLD));
            if active_grants.is_empty() {
                println!("    {} No hay concesiones activas. Blindaje al 100%.", paint("○", DIM));
            } else {
                for g in active_grants {
                    let mins_left = ((g.expires_at - chrono::Local::now().timestamp()) / 60).max(1);
                    let reason_str = g.reason.as_deref().map(|r| format!(" (motivo: «{r}»)")).unwrap_or_default();
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
            println!("  {}", paint("● SECRETOS EN BÓVEDA ($STATE/vault.json)", BOLD));
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
                    let has_grant = grants.is_granted("secret.read") || grants.is_granted(&format!("secret.{}", s.key));
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
        Some(other) => bail!("planificador desconocido: {other} (usa «local», «claude» u «ollama»)"),
        // Jerarquía de 3 niveles con fallback automático transparente:
        // 1. Claude si hay clave de API configurada.
        // 2. Ollama local si está disponible en la máquina.
        // 3. Planificador local determinista sin dependencias externas.
        None => {
            if let Ok(p) = ClaudePlanner::from_env() {
                return Ok(Box::new(p));
            }
            if let Ok(o) = OllamaPlanner::from_env() {
                if o.is_available() {
                    return Ok(Box::new(o));
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
            println!("\n{}", paint("antOS · Modelos LLM Locales Disponibles (Ollama)", BOLD));
            println!("  Endpoint: {}", paint(&ollama.endpoint, GREEN));
            println!("  Modelo Activo: {}\n", paint(&ollama.model, BOLD));

            if models.is_empty() {
                println!("  No hay modelos descargados en Ollama.");
                println!("  Descarga uno con: ollama pull qwen2.5-coder o ollama pull deepseek-coder\n");
            } else {
                for m in models {
                    let is_active = m.starts_with(&ollama.model) || ollama.model.starts_with(&m);
                    let mark = if is_active { paint("●", GREEN) } else { paint("○", DIM) };
                    let tag = if is_active { paint("(activo)", YELLOW) } else { "".to_string() };
                    println!("  {mark} {:<30} {tag}", paint(&m, BOLD));
                }
                println!();
            }
        }
        "status" | _ => {
            println!("\n{}", paint("antOS · Estado de Motores de Inferencia LLM (T6.1)", BOLD));
            
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
                    println!("    Modelos instalados: {}", paint(&format!("{} modelos", models.len()), GREEN));
                }
            } else {
                println!("    Nota: Para arrancar Ollama ejecuta: ollama serve");
            }

            // 3. Fallback determinista
            println!("\n  ● Planificador Determinista Local:");
            println!("    Estado: {}", paint("● Siempre activo (Reglas locales deterministas)", GREEN));
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
            println!("\n{} Escaneando e indexando espacio de trabajo: {}", paint("●", GREEN), paint(&ctx.workspace.display().to_string(), BOLD));
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
                println!("{} No existe índice previo. Indexando espacio de trabajo por primera vez...", paint("i", YELLOW));
                let s = memory::MemoryEngine::index_workspace(&ctx.workspace)?;
                memory::MemoryEngine::save(&s, &db_path)?;
                s
            };

            let hits = memory::MemoryEngine::search(&store, query, limit);
            println!("\n{}", paint(&format!("antOS · Búsqueda Semántica Vectorial para «{query}»"), BOLD));
            println!("  Resultados encontrados: {}\n", hits.len());

            if hits.is_empty() {
                println!("  No se encontraron coincidencias relevantes en el código o tickets.\n");
            } else {
                for (idx, h) in hits.iter().enumerate() {
                    let score_badge = paint(&format!("[{:.2}]", h.score), GREEN);
                    let kind_badge = paint(&format!("{:?}", h.kind), DIM);
                    println!("  {}. {} {} {}:{}", idx + 1, score_badge, kind_badge, paint(&h.path, BOLD), h.line_start);
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

            println!("\n{}", paint("antOS · Grafo de Contexto y Dependencias del Proyecto", BOLD));
            match target {
                Some(t) => {
                    let related = store.graph.related_to(t);
                    println!("  Relaciones para símbolo o archivo «{}»: {}\n", paint(t, BOLD), related.len());
                    for (node, edge) in related {
                        println!("    • {:<18} ──> {} ({})", format!("{:?}", edge), paint(&node.label, BOLD), node.kind);
                    }
                    println!();
                }
                None => {
                    println!("  Total de nodos:   {}", paint(&store.graph.nodes.len().to_string(), GREEN));
                    println!("  Total de aristas: {}\n", paint(&store.graph.edges.len().to_string(), GREEN));
                    println!("  Usa: antos memory graph <nodo> para inspeccionar relaciones.");
                    println!("  Ejemplo: antos memory graph ticket:T6.1 o file:system/antosd/src/main.rs\n");
                }
            }
        }
        "status" | _ => {
            let exists = db_path.exists();
            println!("\n{}", paint("antOS · Memoria Semántica y Grafo de Contexto (T6.2)", BOLD));
            println!("  Ubicación: {}", paint(&db_path.display().to_string(), DIM));
            if exists {
                if let Ok(store) = memory::MemoryEngine::load(&db_path) {
                    println!("  Estado:    {}", paint("● Activo / Sincronizado", GREEN));
                    println!("  Fragmentos: {}", paint(&store.chunks.len().to_string(), BOLD));
                    println!("  Nodos:      {}", paint(&store.graph.nodes.len().to_string(), BOLD));
                    println!("  Aristas:    {}", paint(&store.graph.edges.len().to_string(), BOLD));
                } else {
                    println!("  Estado:    {}", paint("! Archivo de memoria corrupto", RED));
                }
            } else {
                println!("  Estado:    {}", paint("○ Sin indexar (Ejecuta: antos memory index)", YELLOW));
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
                    anyhow::anyhow!("perfil desconocido «{p}». Opciones válidas: rust, node, python, go, base")
                })?,
                None => env::EnvEngine::detect_stack(&ctx.workspace).unwrap_or(env::EnvProfile::Base),
            };

            let summary = env::EnvEngine::init_profile(&ctx.workspace, profile, true, true)?;
            println!("\n{} Perfil de entorno declarativo inicializado exitosamente.", paint("✓", GREEN));
            println!("  Perfil:   {}", paint(&summary.profile, BOLD));
            println!("  Archivos: {}", paint(&summary.created_files.join(", "), GREEN));
            println!("  Paquetes: {}\n", paint(&summary.packages.join(", "), DIM));
            println!("  Ejecuta: antos env sync para comprobar la disponibilidad de las herramientas.\n");
        }
        "sync" | "check" => {
            println!("\n{}", paint("antOS · Sincronización y Diagnóstico de Toolchains (T7.1)", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

            let statuses = env::EnvEngine::check_toolchains(&ctx.workspace)?;
            let mut all_ok = true;

            for s in statuses {
                if s.available {
                    let loc = s.path.unwrap_or_default();
                    println!("  {} {:<16} ({})", paint("✓", GREEN), paint(&s.name, BOLD), paint(&loc, DIM));
                } else {
                    all_ok = false;
                    println!("  {} {:<16} ({})", paint("✗", RED), paint(&s.name, BOLD), paint("no instalado en el sistema o nix-store", RED));
                }
            }

            println!();
            if all_ok {
                println!("  {} Todas las toolchains declaradas están disponibles y operativas.\n", paint("✓ Entorno listo:", GREEN));
            } else {
                println!("  {} Faltan herramientas por aprovisionar. Puedes usar devbox shell o nix develop.\n", paint("! Advertencia:", YELLOW));
            }
        }
        "status" | _ => {
            println!("\n{}", paint("antOS · Estado del Perfil de Entorno (T7.1)", BOLD));
            let cfg = env::EnvEngine::load_config(&ctx.workspace)?;

            match cfg {
                Some(c) => {
                    println!("  Perfil activo:      {}", paint(&c.profile, GREEN));
                    println!("  Paquetes declarados: {}", paint(&c.packages.join(", "), BOLD));
                    let statuses = env::EnvEngine::check_toolchains(&ctx.workspace)?;
                    let available_count = statuses.iter().filter(|s| s.available).count();
                    println!("  Disponibilidad:     {}/{} herramientas en PATH\n", available_count, statuses.len());
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
            println!("\n{} Cuotas de recursos de sandbox actualizadas.", paint("✓", GREEN));
            println!("  • Timeout:   {}s", paint(&q.timeout_secs.to_string(), BOLD));
            println!("  • Memoria:   {} MB", paint(&q.max_memory_mb.to_string(), BOLD));
            println!("  • CPU:       {}%", paint(&q.cpu_quota_percent.to_string(), BOLD));
            println!("  • Max PIDs:  {} procesos\n", paint(&q.max_pids.to_string(), BOLD));
        }
        "reset" => {
            let def = sandbox::quota::ResourceQuota::default();
            sandbox::quota::save_quota(&ctx.workspace, &def)?;
            println!("\n{} Cuotas de sandbox restablecidas a los valores por defecto del sistema.\n", paint("✓", GREEN));
        }
        "status" | _ => {
            println!("\n{}", paint("antOS · Cuotas y Límites de Recursos para Sandboxes (T7.2)", BOLD));
            let q = sandbox::quota::load_quota(&ctx.workspace)?;
            let cgroup_avail = sandbox::quota::CgroupV2Manager::is_available();

            let backend = if cgroup_avail {
                paint("● cgroups v2 (Linux)", GREEN)
            } else {
                paint("● Seatbelt + Supervisor de Procesos (macOS)", BLUE)
            };

            println!("  Mecanismo:          {backend}");
            println!("  Timeout de agente:  {}", paint(&format!("{}s", q.timeout_secs), GREEN));
            println!("  Límite de memoria:  {}", paint(&format!("{} MB", q.max_memory_mb), GREEN));
            println!("  Cuota de CPU:       {}", paint(&format!("{}%", q.cpu_quota_percent), GREEN));
            println!("  Límite de procesos: {}\n", paint(&format!("{} PIDs", q.max_pids), GREEN));
            println!("  Usa antos quota set [--timeout N] [--memory N] [--cpu N] para modificar.\n");
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ diff

fn cmd_diff(ctx: &Ctx, args: &[String]) -> Result<()> {
    let target = args.first().map(String::as_str).unwrap_or("HEAD");
    println!("\n{}", paint("antOS · Visor Interactivo de Diffs y Parches (T8.1)", BOLD));
    println!("  Espacio de trabajo: {}", paint(&ctx.workspace.display().to_string(), DIM));
    println!("  Objetivo:           {}\n", paint(target, BOLD));

    let git_out = std::process::Command::new("git")
        .current_dir(&ctx.workspace)
        .args(&["diff", target])
        .output();

    match git_out {
        Ok(out) if out.status.success() => {
            let diff_str = String::from_utf8_lossy(&out.stdout);
            let files = diff_view::DiffEngine::parse_unified_diff(&diff_str);

            if files.is_empty() {
                println!("  {} No hay cambios ni diferencias pendientes contra «{target}».\n", paint("✓ Repositorio limpio:", GREEN));
            } else {
                let rendered = diff_view::DiffEngine::render_terminal(&files);
                print!("{rendered}");
            }
        }
        _ => {
            println!("  {} No se pudo invocar git diff en el espacio de trabajo.\n", paint("✗ Error:", RED));
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ terminal / vte

fn cmd_terminal(args: &[String]) -> Result<()> {
    let mut session = vte::TerminalSession::new("vte-cli");
    println!("\n{}", paint("antOS · Consola Terminal VTE Embebida (T8.1)", BOLD));
    println!("  Shell interactivo:  {}\n", paint(&session.active_shell, GREEN));

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
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos notify approve <ID>"))?;
            let (ok, msg) = engine.handle_action(&ctx.workspace, id, antos_protocolo::NotificationAction::Approve)?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("reject" | "rechazar" | "rollback") => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos notify reject <ID>"))?;
            let (ok, msg) = engine.handle_action(&ctx.workspace, id, antos_protocolo::NotificationAction::Reject)?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("dismiss" | "descartar") => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos notify dismiss <ID>"))?;
            let (ok, msg) = engine.handle_action(&ctx.workspace, id, antos_protocolo::NotificationAction::Dismiss)?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("clear" | "limpiar") => {
            let count = engine.clear(&ctx.workspace)?;
            println!("\n{} Se limpiaron {} notificaciones leídas.\n", paint("✓", GREEN), count);
        }
        _ => {
            let list = engine.list(&ctx.workspace)?;
            println!("\n{}", paint("antOS · Bandeja de Notificaciones y Aprobaciones Asíncronas (T8.2)", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

            if list.is_empty() {
                println!("  {} No hay notificaciones ni aprobaciones pendientes.\n", paint("✓ Bandeja al día:", GREEN));
                println!("  Los agentes multi-agente antFlow notificarán aquí cuando completen tareas.");
                println!("  Comandos: antos notify approve <ID> | antos notify reject <ID>\n");
            } else {
                for n in &list {
                    let mark = if n.read { paint("○ leída", DIM) } else { paint("● NUEVA", YELLOW) };
                    let kind_badge = match n.kind {
                        antos_protocolo::NotificationKind::ApprovalRequired => paint("⚠️ APROBACIÓN REQUERIDA", YELLOW),
                        antos_protocolo::NotificationKind::TaskFinished => paint("✓ TAREA COMPLETADA", GREEN),
                        antos_protocolo::NotificationKind::QAFailed => paint("✗ QA FALLIDO", RED),
                        antos_protocolo::NotificationKind::SecurityAlert => paint("🛡️ ALERTA SEGURIDAD", RED),
                        antos_protocolo::NotificationKind::System => paint("ℹ️ SISTEMA", CYAN),
                    };

                    println!("  {} [{}] {} — {}", mark, paint(&n.id, BOLD), kind_badge, paint(&n.ticket_id, BOLD));
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
            let addr = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos mesh connect <IP:PUERTO|MULTIADDR>"))?;
            let peer = engine.connect_peer(&ctx.workspace, addr)?;
            println!("\n{} Conectado al nodo peer en la malla antMesh.", paint("✓", GREEN));
            println!("  • Nodo ID:    {}", paint(&peer.id, BOLD));
            println!("  • Hostname:   {}", paint(&peer.hostname, BOLD));
            println!("  • Dirección:  {}", paint(&peer.address, YELLOW));
            println!("  • Latencia:   {} ms", paint(&peer.latency_ms.to_string(), GREEN));
            println!("  • Recursos:   {} cores CPU, {} MB RAM, VRAM: {:?}",
                peer.resources.cpu_cores, peer.resources.memory_mb, peer.resources.vram_mb
            );
            println!("  • Modelos:    {}\n", peer.resources.available_models.join(", "));
        }
        Some("pair" | "token" | "emparejar") => {
            let token = engine.generate_pairing_token(&ctx.workspace)?;
            let expires_mins = (token.expires_at.saturating_sub(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            ) / 60).max(1);

            println!("\n{}", paint("antOS · Token de Emparejamiento antMesh (T9.1)", BOLD));
            println!("  Token de enlace:    {}", paint(&token.token, GREEN));
            println!("  Identidad del nodo: {}", paint(&token.node_id, BOLD));
            println!("  Válido durante:     {} minutos", paint(&expires_mins.to_string(), YELLOW));
            println!("\n  Usa «{}» en el nodo remoto para unirte a este clúster.\n",
                paint(&format!("antos mesh connect <ESTA_IP>:9042"), BOLD)
            );
        }
        Some("status" | "list") | _ => {
            let status = engine.status(&ctx.workspace)?;
            println!("\n{}", paint("antOS · Red P2P Cifrada antMesh (T9.1)", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

            println!("  {}", paint("● NODO LOCAL", BOLD));
            println!("    ID criptográfico: {}", paint(&status.local_node.id, BOLD));
            println!("    Hostname:         {}", paint(&status.local_node.hostname, BOLD));
            println!("    Dirección escucha:{}", paint(&status.local_node.address, YELLOW));
            println!("    Capacidades:      {} cores CPU · {} MB RAM · VRAM: {:?}",
                status.local_node.resources.cpu_cores,
                status.local_node.resources.memory_mb,
                status.local_node.resources.vram_mb
            );
            println!("    Modelos locales:  {}\n", status.local_node.resources.available_models.join(", "));

            println!("  {}", paint("● PEERS CONECTADOS EN LA MALLA", BOLD));
            if status.peers.is_empty() {
                println!("    {} No hay nodos vecinos conectados.", paint("○", DIM));
                println!("    Usa «antos mesh pair» para generar un token de invitación.");
                println!("    Usa «antos mesh connect <IP:9042>» para vincular un nodo remoto.\n");
            } else {
                for p in &status.peers {
                    let conn_mark = if p.connected { paint("● conectado", GREEN) } else { paint("○ desconectado", DIM) };
                    println!(
                        "    {} [{}] {} · {} ({} ms)",
                        conn_mark,
                        paint(&p.id, BOLD),
                        paint(&p.hostname, BOLD),
                        paint(&p.address, YELLOW),
                        paint(&p.latency_ms.to_string(), GREEN)
                    );
                    println!("       Recursos: {} cores, {} MB RAM, modelos: {}",
                        p.resources.cpu_cores, p.resources.memory_mb, p.resources.available_models.join(", ")
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
            let ticket_id = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos swarm dispatch <TID> [--role <coder|qa>] [--node <ID>]"))?;
            let role = if args.iter().any(|a| a == "--qa") {
                antos_protocolo::AgentRole::QA
            } else {
                antos_protocolo::AgentRole::Coder
            };
            let node_target = args.iter().position(|a| a == "--node" || a == "-n").and_then(|i| args.get(i + 1)).map(String::as_str);
            let task = engine.dispatch_remote_role(&ctx.workspace, ticket_id, role, node_target)?;
            println!("\n{} Tarea distribuida despachada al Swarm.", paint("✓", GREEN));
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
            println!("\n{}", paint("antOS · Centro de Control Swarm Multi-Nodo (T9.2)", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

            println!("  Nodos en el clúster: {} · Tareas activas: {}\n",
                paint(&status.nodes.len().to_string(), BOLD),
                paint(&status.total_tasks.to_string(), GREEN)
            );

            for n in &status.nodes {
                let badge = if n.is_local { paint("● LOCAL", GREEN) } else { paint("🌐 REMOTO", CYAN) };
                let vram_str = n.vram_available_mb.map(|v| format!("{} MB VRAM", v)).unwrap_or_else(|| "N/A".into());
                println!("  {} [{}] {} · {} ({} CPUs · {})",
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
                        println!("     └─ Tarea {}: rol {:?} en rama {} [{}]",
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
            println!("\n{} Sistema de ficheros semántico /antfs montado con éxito.", paint("✓", GREEN));
            println!("  • Punto de montaje: {}", paint(&path.display().to_string(), BOLD));
            println!("  • Inspección:       {} o {}\n", paint(&format!("ls {}", path.display()), YELLOW), paint(&format!("cat {}/README.antfs", path.display()), YELLOW));
        }
        Some("unmount" | "umount" | "desmonta") => {
            let target = args.get(1).map(String::as_str);
            engine.unmount(&ctx.workspace, target)?;
            println!("\n{} Sistema de ficheros semántico /antfs desmontado correctamente.\n", paint("✓", GREEN));
        }
        Some("ls" | "list") => {
            let vpath = args.get(1).map(String::as_str).unwrap_or("/antfs");
            let entries = engine.list_dir(&ctx.workspace, vpath)?;
            println!("\n{} Listado de {}", paint("antOS VFS ·", BOLD), paint(vpath, YELLOW));
            if entries.is_empty() {
                println!("  (directorio vacío)\n");
            } else {
                for e in entries {
                    let mark = if e.is_dir { paint("📁", BLUE) } else { paint("📄", GREEN) };
                    println!("  {} {:<26} {:<15} ({} bytes)", mark, paint(&e.name, BOLD), paint(&e.node_type, DIM), e.size);
                }
                println!();
            }
        }
        Some("cat" | "read" | "lee") => {
            let vpath = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos vfs read <ruta_virtual> (ej. /antfs/symbols/structs/MeshStatus)"))?;
            let content = engine.read_path(&ctx.workspace, vpath)?;
            println!("\n{}\n", content);
        }
        Some("symbols" | "simbolos" | "símbolos") => {
            let symbols = engine.discover_symbols(&ctx.workspace)?;
            println!("\n{} ({} descubiertos)\n", paint("antOS VFS · Símbolos Semánticos del Proyecto", BOLD), paint(&symbols.len().to_string(), GREEN));
            for cat in &["structs", "functions", "enums", "traits"] {
                let cat_syms: Vec<_> = symbols.iter().filter(|s| &s.category == cat).collect();
                if !cat_syms.is_empty() {
                    println!("  {} {} ({}):", paint("●", YELLOW), paint(*cat, BOLD), cat_syms.len());
                    for s in cat_syms {
                        println!("    • {:<28} {}:{}", paint(&s.name, BOLD), paint(&s.file_path, DIM), s.line_number);
                    }
                    println!();
                }
            }
        }
        Some("validate" | "check" | "valida") => {
            let rel = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos vfs validate <archivo> (ej. src/main.rs)"))?;
            let abs_path = ctx.workspace.join(rel);
            if !abs_path.exists() {
                bail!("el archivo «{}» no existe en el espacio de trabajo", abs_path.display());
            }
            let text = std::fs::read_to_string(&abs_path)?;
            let guard = vfs_guard::VfsGuardEngine::global();
            let res = guard.validate_content(rel, &text);
            println!("\n{} Validación de integridad sintáctica VFS", paint("antOS ·", BOLD));
            println!("  Archivo:  {}", paint(rel, YELLOW));
            println!("  Lenguaje: {} ({} líneas)\n", paint(&res.language, BOLD), res.line_count);
            if res.is_valid {
                println!("  {} El archivo es sintácticamente válido y seguro para persistir.\n", paint("✓ Aprobado:", GREEN));
            } else {
                println!("  {} Se detectaron {} problema(s) sintáctico(s):", paint("✗ Rechazado:", RED), res.errors.len());
                for err in res.errors {
                    println!("    • Línea {}, columna {}: {}", paint(&err.line.to_string(), YELLOW), err.column, err.message);
                }
                println!();
            }
        }
        Some("guard" | "guardia" | "interceptor") => {
            let guard = vfs_guard::VfsGuardEngine::global();
            let status = guard.status()?;
            println!("\n{}", paint("antOS VFS · Interceptor de Escrituras Semánticas (T10.2)", BOLD));
            let state_str = if status.enabled { paint("● ACTIVO (ENFORCING)", GREEN) } else { paint("○ INACTIVO", DIM) };
            println!("  Estado del interceptor:   {}", state_str);
            println!("  Escrituras interceptadas: {}", paint(&status.total_intercepted.to_string(), BOLD));
            println!("  Escrituras rechazadas:    {}", paint(&status.total_rejected.to_string(), if status.total_rejected > 0 { RED } else { GREEN }));
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
            println!("\n{}", paint("antOS · Sistema de Ficheros Virtual FUSE (/antfs) - T10.1 & T10.2", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

            let mnt_badge = if status.is_mounted { paint("● MONTADO", GREEN) } else { paint("○ NO MONTADO", DIM) };
            println!("  Estado del VFS:     {}", mnt_badge);
            if let Some(mnt) = status.mount_point {
                println!("  Punto de montaje:   {}", paint(&mnt, YELLOW));
            }
            println!("  Símbolos AST:       {} indexados", paint(&status.total_symbols.to_string(), BOLD));
            println!("  Módulos navegables: {} en /antfs/graph\n", paint(&status.total_modules.to_string(), BOLD));

            println!("  Subcomandos disponibles:");
            println!("    • antos vfs symbols          Lista símbolos AST (structs, functions, enums, traits)");
            println!("    • antos vfs ls [ruta]        Explora la jerarquía /antfs (symbols, graph, git)");
            println!("    • antos vfs read <ruta>      Lee el código o diff de un inodo virtual");
            println!("    • antos vfs mount [ruta]     Proyecta /antfs en el disco local");
            println!("    • antos vfs unmount [ruta]   Desmonta la proyección /antfs");
            println!("    • antos vfs validate <file>  Valida la integridad sintáctica antes de persistir");
            println!("    • antos vfs guard            Muestra métricas del interceptor de escrituras\n");
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
            println!("\n{}", paint("antOS · Supervisor Kernel eBPF LSM (T11.1)", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

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
            println!("  Capacidad del ring buffer: {} entradas (uso: {})", status.ring_buffer_capacity, status.ring_buffer_utilization);
            println!("  Eventos capturados:        {}", paint(&status.total_events_captured.to_string(), BOLD));
            println!("  Violaciones bloqueadas:    {}\n", paint(&status.total_violations_blocked.to_string(), if status.total_violations_blocked > 0 { RED } else { GREEN }));
        }
        Some("trace" | "traza") => {
            let pid_opt = args.get(1).and_then(|p| p.parse::<u32>().ok());
            let events = match pid_opt {
                Some(pid) => engine.trace_pid(pid),
                None => engine.get_audit_log(25),
            };
            println!("\n{} Traza en vivo de syscalls y eventos de seguridad", paint("antOS eBPF ·", BOLD));
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
                    println!("  {} [{}] PID {}:{} ➔ {} ({:?})",
                        mark, paint(&ev.id, DIM), ev.pid, paint(&ev.comm, BOLD), paint(&ev.target_resource, YELLOW), ev.hook
                    );
                    if let Some(ref r) = ev.violation_reason {
                        println!("      └─ {}", paint(r, DIM));
                    }
                }
                println!();
            }
        }
        Some("audit" | "log" | "registro") => {
            let limit = args.get(1).and_then(|l| l.parse::<usize>().ok()).unwrap_or(20);
            let events = engine.get_audit_log(limit);
            println!("\n{} Registro de auditoría eBPF (últimos {} eventos)\n", paint("antOS eBPF ·", BOLD), events.len());
            if events.is_empty() {
                println!("  (registro vacío)\n");
            } else {
                for ev in events {
                    let mark = match ev.action_taken {
                        antos_protocolo::EbpfSecurityAction::Allowed => paint("✓", GREEN),
                        antos_protocolo::EbpfSecurityAction::Blocked => paint("⛔", RED),
                        antos_protocolo::EbpfSecurityAction::Audited => paint("👁", YELLOW),
                    };
                    println!("  {} [{}] {:<18} PID {}:{} ➔ {}",
                        mark, paint(&ev.id, DIM), format!("{:?}", ev.hook), ev.pid, paint(&ev.comm, BOLD), ev.target_resource
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
            let target = args.get(2).map(String::as_str).unwrap_or_else(|| match hook {
                antos_protocolo::EbpfHookKind::SocketConnect => "192.168.1.50:4444",
                antos_protocolo::EbpfHookKind::FileOpen => "/etc/shadow",
                antos_protocolo::EbpfHookKind::BprmCheckSecurity => "/bin/nc",
                antos_protocolo::EbpfHookKind::SyscallTrace => "ptrace",
            });

            let ev = engine.simulate_violation(hook, target);
            println!("\n{} Simulación de intento de evasión de sandbox", paint("antOS eBPF ·", BOLD));
            println!("  Hook interceptado: {:?}", ev.hook);
            println!("  Recurso objetivo:  {}", paint(&ev.target_resource, YELLOW));
            println!("  Acción del kernel: {}", paint("⛔ BLOQUEADO", RED));
            println!("  Alerta disparada:  {} Se envió notificación prioritaria a la bandeja Wayland.\n", paint("✓", GREEN));
        }
        _ => {
            let status = engine.status()?;
            println!("\n{}", paint("antOS · Supervisor Kernel eBPF LSM (T11.1)", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

            let lsm_badge = if status.lsm_enabled {
                paint("● KERNEL LSM ACTIVO", GREEN)
            } else {
                paint("○ EMULACIÓN ESPACIO USUARIO", YELLOW)
            };
            println!("  Soporte:            {}", lsm_badge);
            println!("  Sondas activas:     {}", status.active_probes.len());
            println!("  Eventos capturados: {}", paint(&status.total_events_captured.to_string(), BOLD));
            println!("  Bloqueos evasión:   {}\n", paint(&status.total_violations_blocked.to_string(), if status.total_violations_blocked > 0 { RED } else { GREEN }));

            println!("  Subcomandos disponibles:");
            println!("    • antos ebpf status            Diagnóstico de sondas y soporte de kernel");
            println!("    • antos ebpf trace [pid]       Traza de llamadas al sistema en tiempo real");
            println!("    • antos ebpf audit [limit]     Registro de auditoría del ring buffer");
            println!("    • antos ebpf simulate <tipo>   Simula evasión (file|socket|bprm) y alerta\n");
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
            println!("\n{} Ejecutando perfilado continuo para: {}", paint("antOS Profiler ·", BOLD), paint(&command, YELLOW));
            let report = engine.run_and_profile(&ctx.workspace, &command)?;

            let peak_mb = report.peak_memory_bytes as f64 / (1024.0 * 1024.0);
            let status_badge = if report.exit_code == 0 {
                paint("EXIT 0 (Éxito)", GREEN)
            } else {
                paint(&format!("EXIT {}", report.exit_code), RED)
            };

            println!("\n{}", paint("Resultado del Perfilado:", BOLD));
            println!("  Estado del comando:       {}", status_badge);
            println!("  Duración de Wall-Clock:   {} ms", paint(&report.duration_ms.to_string(), BOLD));
            println!("  Tiempo de CPU:            {} ms usuario, {} ms sistema", report.cpu_user_ms, report.cpu_sys_ms);
            println!("  Memoria Pico (RSS):       {} MB", paint(&format!("{peak_mb:.2}"), CYAN));
            println!("  Fallas de Página (Faults): {}\n", report.page_faults);

            if !report.hotspots.is_empty() {
                println!("{}", paint("  Puntos Calientes de Ejecución (Hotspots):", BOLD));
                for h in report.hotspots {
                    println!("    • {:<32} CPU: {:>4.1}% | Mem: {:>4.1}% ({} muestras)",
                        paint(&h.name, YELLOW), h.percentage_cpu, h.percentage_memory, h.calls_or_samples
                    );
                }
                println!();
            }

            if !report.suggestions.is_empty() {
                println!("{}", paint("  Recomendaciones de Optimización para Agentes Coder / QA:", BOLD));
                for s in report.suggestions {
                    let impact_color = if s.potential_impact.contains("Alto") { RED } else { YELLOW };
                    println!("    ★ [{}] {}", paint(&s.potential_impact, impact_color), paint(&s.title, BOLD));
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
            println!("\n{}", paint("antOS Profiler · Top Cuellos de Botella (Hotspots)", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

            if hotspots.is_empty() {
                println!("  (no hay hotspots registrados; ejecuta «antos profile run <comando>»)\n");
            } else {
                for (idx, h) in hotspots.iter().enumerate() {
                    println!("  {}. {:<32} CPU: {:>5.1}% | Mem: {:>5.1}% ({} llamadas)",
                        idx + 1, paint(&h.name, YELLOW), h.percentage_cpu, h.percentage_memory, h.calls_or_samples
                    );
                }
                println!();
            }
        }
        Some("analyze" | "analiza" | "sugerencias") => {
            let (hotspots, suggestions) = engine.analyze_aggregate(&ctx.workspace);
            println!("\n{}", paint("antOS Profiler · Análisis y Recomendaciones Técnicas", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

            if suggestions.is_empty() {
                println!("  (sin recomendaciones activas; ejecuta «antos profile run <comando>»)\n");
            } else {
                println!("  Puntos calientes consolidados: {}\n", hotspots.len());
                for s in suggestions {
                    let impact_color = if s.potential_impact.contains("Alto") { RED } else { YELLOW };
                    println!("  ★ [{}] {}", paint(&s.potential_impact, impact_color), paint(&s.title, BOLD));
                    println!("    └─ {}", paint(&s.description, DIM));
                }
                println!();
            }
        }
        Some("list" | "reports" | "reportes" | "historial") => {
            let reports = engine.load_reports(&ctx.workspace);
            println!("\n{}", paint("antOS Profiler · Histórico de Reportes de Rendimiento", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));

            if reports.is_empty() {
                println!("  (no hay reportes guardados)\n");
            } else {
                for r in reports {
                    let peak_mb = r.peak_memory_bytes as f64 / (1024.0 * 1024.0);
                    println!("  • [{}] «{}» — {} ms | {:.1} MB RSS (código {})",
                        paint(&r.id, DIM), paint(&r.command, BOLD), r.duration_ms, peak_mb, r.exit_code
                    );
                }
                println!();
            }
        }
        _ => {
            let reports = engine.load_reports(&ctx.workspace);
            println!("\n{}", paint("antOS · Profiler Continuo de Runtime (T11.2)", BOLD));
            println!("  Espacio de trabajo: {}\n", paint(&ctx.workspace.display().to_string(), DIM));
            println!("  Reportes registrados:   {}", paint(&reports.len().to_string(), BOLD));

            println!("  Subcomandos disponibles:");
            println!("    • antos profile run <cmd>      Ejecuta y perfila un comando en tiempo real");
            println!("    • antos profile top            Lista los principales puntos calientes (hotspots)");
            println!("    • antos profile analyze        Sintetiza recomendaciones para Coder y QA");
            println!("    • antos profile list           Muestra el histórico de reportes guardados\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ tickets

fn cmd_tickets(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = spec::SpecEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("new" | "create" | "add") => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos ticket new <ID> <Título> [--fase \"...\"] [--desc \"...\"]"))?;
            let title = args.get(2).ok_or_else(|| anyhow::anyhow!("uso: antos ticket new <ID> <Título> [--fase \"...\"] [--desc \"...\"]"))?;
            
            let phase = args
                .iter()
                .position(|a| a == "--fase" || a == "-f")
                .and_then(|i| args.get(i + 1))
                .cloned();

            let desc = args
                .iter()
                .position(|a| a == "--desc" || a == "-d")
                .and_then(|i| args.get(i + 1))
                .cloned();

            let path = engine.create_ticket(&ctx.workspace, id, title, desc.as_deref(), phase.as_deref())?;
            println!(
                "\n{} Ticket {} creado exitosamente en {}.\n",
                paint("✓", GREEN),
                paint(id, BOLD),
                paint(&path.display().to_string(), DIM)
            );
            return Ok(());
        }
        Some("status" | "set-status") => {
            let id = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos ticket status <ID> <completado|progreso|revision|pendiente>"))?;
            let status_raw = args.get(2).ok_or_else(|| anyhow::anyhow!("uso: antos ticket status <ID> <completado|progreso|revision|pendiente>"))?;
            let st = match status_raw.to_lowercase().as_str() {
                "completado" | "done" | "hecho" => antos_protocolo::TicketStatus::Completado,
                "progreso" | "en_progreso" | "in_progress" => antos_protocolo::TicketStatus::EnProgreso,
                "revision" | "revisión" | "review" => antos_protocolo::TicketStatus::EnRevision,
                _ => antos_protocolo::TicketStatus::Pendiente,
            };
            engine.update_ticket_status(&ctx.workspace, id, st)?;
            println!(
                "\n{} Estado del ticket {} actualizado a {}.\n",
                paint("✓", GREEN),
                paint(id, BOLD),
                st.etiqueta()
            );
            return Ok(());
        }
        Some(ticket_id) if ticket_id != "list" => {
            let detalle = engine.obtener_ticket(&ctx.workspace, ticket_id)?;
            match detalle {
                Some(t) => {
                    println!(
                        "\n{} {}  {}",
                        paint(&t.id, BOLD),
                        paint(&t.fase, DIM),
                        t.estado.etiqueta()
                    );
                    println!("{}", paint(&t.titulo, BOLD));
                    println!();
                    println!("{}", paint("Descripción:", BOLD));
                    println!("  {}", t.descripcion);
                    if !t.alcance_tecnico.is_empty() {
                        println!();
                        println!("{}", paint("Alcance Técnico:", BOLD));
                        for a in &t.alcance_tecnico {
                            println!("  • {a}");
                        }
                    }
                    if !t.criterios_aceptacion.is_empty() {
                        println!();
                        println!("{}", paint("Criterios de Aceptación:", BOLD));
                        for c in &t.criterios_aceptacion {
                            println!("  • {c}");
                        }
                    }
                    println!();
                }
                None => {
                    println!("\nticket '{ticket_id}' no encontrado en el espacio de trabajo.\n");
                }
            }
            return Ok(());
        }
        _ => {}
    }

    let tickets = engine.listar_tickets(&ctx.workspace)?;
    if tickets.is_empty() {
        println!("\nno se encontraron tickets en el espacio de trabajo.");
        println!("Crea uno con: antos ticket new <ID> <Título>\n");
        return Ok(());
    }

    println!("\n{}", paint("antOS · Catálogo y Hoja de Ruta de Tickets", BOLD));
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
        if t.estado == antos_protocolo::TicketStatus::Completado {
            completados += 1;
        }
        println!(
            "  {:<8} {:<8} {:<55} {}",
            paint(&t.fase, DIM),
            paint(&t.id, BOLD),
            ellipsis(&t.titulo, 53),
            t.estado.etiqueta()
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

    println!("\n{}", paint("antOS · Diagnóstico de Puertos y Procesos", BOLD));
    if puertos.is_empty() {
        if let Some(p) = filtro {
            println!("  El puerto {} está libre.\n", paint(&format!(":{p}"), GREEN));
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
        println!("\n{}", paint("antOS · Roles de Agentes Especializados (antFlow)", BOLD));
        let roles = [
            antos_protocolo::AgentRole::Arquitecto,
            antos_protocolo::AgentRole::Coder,
            antos_protocolo::AgentRole::QA,
            antos_protocolo::AgentRole::Auditor,
        ];
        for r in roles {
            println!("\n  {} {}", paint("●", GREEN), paint(r.nombre(), BOLD));
            println!("    {}", paint(r.descripcion(), DIM));
            println!("    {}", paint(&format!("Directiva: {}", r.prompt_sistema()), DIM));
        }
        println!();
        return Ok(());
    }

    match args[0].as_str() {
        "run" => {
            let ticket_id = args.get(1).ok_or_else(|| anyhow::anyhow!("uso: antos agent run <ticket_id> [--auto]"))?;
            let auto = args.iter().any(|a| a == "--auto" || a == "-a");
            let node_target = args.iter().position(|a| a == "--node" || a == "-n" || a == "--remote").and_then(|i| args.get(i + 1));

            println!("\n{}", paint(&format!("antOS · Orquestador antFlow para {ticket_id}"), BOLD));

            if let Some(target) = node_target {
                println!("  {} Despachando rol a nodo remoto Swarm: {}", paint("🌐", CYAN), paint(target, BOLD));
                let _ = distributed::SwarmEngine::global().dispatch_remote_role(&ctx.workspace, ticket_id, antos_protocolo::AgentRole::Coder, Some(target))?;
            }

            let engine = flow::FlowEngine::global();

            let task = if auto {
                println!("  {} Ejecutando pipeline automatizado de agentes...", paint("▶", GREEN));
                engine.ejecutar_pipeline_worktree(&ctx.workspace, &ctx.state, ticket_id, &[])?
            } else {
                engine.iniciar_tarea(&ctx.workspace, &ctx.state, ticket_id)?
            };

            println!("  Tarea ID:       {}", paint(&task.id, YELLOW));
            println!("  Ticket:         {}", paint(&task.ticket_id, BOLD));
            println!("  Estado:         {}", task.estado.etiqueta());
            if let Some(wt) = &task.worktree_path {
                println!("  Worktree:       {}", paint(wt, DIM));
            }
            if let Some(br) = &task.branch_name {
                println!("  Rama de Agente: {}", paint(br, GREEN));
            }
            if let Some(resumen) = &task.resumen_auditoria {
                println!("  Auditoría:      {}", paint(resumen, GREEN));
            }

            println!("\n  {}", paint("Historial de Transiciones de Agentes:", BOLD));
            for t in &task.historial {
                let rol_fmt = t.rol.map(|r| format!(" [{}]", r.nombre())).unwrap_or_default();
                println!("    • {}{}: {}", paint(t.estado_nuevo.etiqueta(), BOLD), paint(&rol_fmt, DIM), t.detalle);
            }

            if let Some(diff) = &task.diff_preview {
                if !diff.is_empty() {
                    println!("\n  {}", paint("Previsualización de Diff Consolidado:", BOLD));
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
                    println!("\n{}", paint(&format!("antOS · Estado de Tarea antFlow [{}]", task.ticket_id), BOLD));
                    println!("  Estado:     {}", task.estado.etiqueta());
                    println!("  Rol Activo: {}", task.rol_actual.map(|r| r.nombre()).unwrap_or("Ninguno"));
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
                    for h in &task.historial {
                        println!("    • [{}] {}", h.estado_nuevo.etiqueta(), h.detalle);
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
                            t.estado.etiqueta(),
                            t.reintentos_qa
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
            let svc = args.get(1).ok_or_else(|| anyhow::anyhow!("debes especificar el nombre del servicio (ej. antos service up postgres)"))?;
            let port = args.get(2).and_then(|p| p.parse::<u16>().ok());
            let db = args.get(3).map(String::as_str);

            println!("\n{} Aprovisionando servicio efímero «{}»...", paint("⚡", BOLD), paint(svc, YELLOW));
            let info = service::start_service(svc, port, db, &ctx.state, &ctx.workspace)?;
            println!("  {} Servicio:      {}", paint("●", GREEN), paint(&info.name, BOLD));
            println!("  {} Puerto:        {}", paint("●", GREEN), paint(&info.port.to_string(), YELLOW));
            println!("  {} Estado:        {}", paint("●", GREEN), paint(&info.status, GREEN));
            println!("  {} Variable .env: {}={}", paint("●", GREEN), paint(&info.env_var_key, BOLD), paint(&info.env_var_value, CYAN));
            println!("  {} Almacenamiento: {}\n", paint("●", GREEN), paint(&info.data_dir, DIM));
        }
        "down" | "stop" => {
            let svc = args.get(1).ok_or_else(|| anyhow::anyhow!("debes especificar el nombre del servicio (ej. antos service down postgres)"))?;
            service::stop_service(svc, &ctx.state)?;
            println!("\n{} Servicio «{}» detenido y limpiado.\n", paint("✓", GREEN), paint(svc, BOLD));
        }
        "status" | "list" | _ => {
            let svc_filter = if sub != "status" && sub != "list" {
                Some(sub)
            } else {
                args.get(1).map(String::as_str)
            };

            let services = service::get_service_status(svc_filter, &ctx.state)?;
            println!("\n{}", paint("antOS · Servicios Locales Efímeros de Desarrollo (T5.1)", BOLD));
            if services.is_empty() {
                println!("  No hay servicios efímeros aprovisionados.");
                println!("  Inicia uno con: antos service up <postgres|redis|mariadb|meilisearch>\n");
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
                        s.name, s.port, st_fmt, paint(&s.env_var_key, BOLD), ellipsis(&s.env_var_value, 38)
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
            println!("\n{} Despachando ticket {} al equipo multi-agente antFlow...", paint("🚀", BOLD), paint(target_ticket, YELLOW));
            let run_args = vec!["run".to_string(), target_ticket.clone(), "--auto".to_string()];
            return cmd_agent(ctx, &run_args);
        }
    }

    let spec_engine = spec::SpecEngine::global();
    let tickets = spec_engine.list_tickets(&ctx.workspace)?;
    let flow_engine = flow::FlowEngine::global();
    let tasks = flow_engine.list_tasks();

    println!("\n{}", paint("╔══════════════════════════════════════════════════════════════════════════════════════╗", BOLD));
    println!("║       {}        ║", paint("antOS · CENTRO DE CONTROL DE AGENTES Y TABLERO KANBAN (Super + A)", BOLD));
    println!("{}\n", paint("╚══════════════════════════════════════════════════════════════════════════════════════╝", BOLD));

    // Monitor de Agentes
    println!("  {}", paint("● MONITOR DE AGENTES ACTIVOS (antFlow)", BOLD));
    let roles = [
        ("📐 Arquitecto", antos_protocolo::AgentRole::Arquitecto),
        ("💻 Coder", antos_protocolo::AgentRole::Coder),
        ("🧪 QA / Tester", antos_protocolo::AgentRole::QA),
        ("🛡️ Auditor", antos_protocolo::AgentRole::Auditor),
    ];

    for (etiqueta_rol, rol) in roles {
        let active_tasks: Vec<_> = tasks.iter().filter(|t| t.rol_actual == Some(rol)).collect();
        if active_tasks.is_empty() {
            println!("    {} {:<18} {}", paint("○", DIM), etiqueta_rol, paint("[Inactivo / En espera]", DIM));
        } else {
            for t in active_tasks {
                println!(
                    "    {} {:<18} {} → Tarea: {} ({})",
                    paint("●", GREEN),
                    paint(etiqueta_rol, BOLD),
                    paint(t.estado.etiqueta(), YELLOW),
                    paint(&t.ticket_id, BOLD),
                    t.worktree_path.as_deref().unwrap_or("sandbox")
                );
            }
        }
    }
    println!();

    // Columnas Kanban
    let pendientes: Vec<_> = tickets.iter().filter(|t| t.estado == antos_protocolo::TicketStatus::Pendiente).collect();
    let en_progreso: Vec<_> = tickets.iter().filter(|t| t.estado == antos_protocolo::TicketStatus::EnProgreso).collect();
    let en_revision: Vec<_> = tickets.iter().filter(|t| t.estado == antos_protocolo::TicketStatus::EnRevision).collect();
    let completados: Vec<_> = tickets.iter().filter(|t| t.estado == antos_protocolo::TicketStatus::Completado).collect();

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

    let max_filas = [pendientes.len(), en_progreso.len(), en_revision.len(), completados.len()]
        .into_iter()
        .max()
        .unwrap_or(0);

    for i in 0..max_filas {
        let col1 = pendientes.get(i).map(|t| format!("{} {}", t.id, ellipsis(&t.titulo, 14))).unwrap_or_default();
        let col2 = en_progreso.get(i).map(|t| format!("{} {}", t.id, ellipsis(&t.titulo, 14))).unwrap_or_default();
        let col3 = en_revision.get(i).map(|t| format!("{} {}", t.id, ellipsis(&t.titulo, 14))).unwrap_or_default();
        let col4 = completados.get(i).map(|t| format!("{} {}", t.id, ellipsis(&t.titulo, 14))).unwrap_or_default();

        println!(
            "  │ {:<22} │ {:<22} │ {:<22} │ {:<22} │",
            col1, col2, col3, col4
        );
    }
    println!("  └────────────────────────┴────────────────────────┴────────────────────────┴────────────────────────┘");

    println!("\n  {} Usa {} para despachar un ticket al equipo de agentes.", paint("💡", YELLOW), paint("antos panel --dispatch <TID>", BOLD));
    println!("  {} Usa {} para lanzar la interfaz gráfica Wayland/GTK4.\n", paint("🖥️", BOLD), paint("antos-barra", BOLD));

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
  antos demonio              atiende peticiones por socket (lo que usará el escritorio)
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
