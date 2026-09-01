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
pub mod flow;
pub mod service;
pub mod vault;
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
use planner::{claude::ClaudePlanner, local::LocalPlanner, Planner};
use terminal::{ellipsis, paint, tier_color, BOLD, DIM, GREEN, RED, YELLOW};

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
        "tickets" => cmd_tickets(&ctx, &rest[1..]),
        "ports" => cmd_ports(&rest[1..]),
        "services" | "service" => cmd_services(&ctx, &rest[1..]),
        "secrets" | "secret" => cmd_secrets(&ctx, &rest[1..]),
        "agent" | "agents" | "flow" => cmd_agent(&ctx, &rest[1..]),
        "panel" | "board" => cmd_panel(&ctx, &rest[1..]),
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
    let con_red = sandbox::Policy { writes: vec![], reads: vec![], dirs: vec![], network: true, allowed_secrets: vec![] };
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
        Some(other) => bail!("planificador desconocido: {other} (usa «local» o «claude»)"),
        // Sin elección explícita: Claude si hay clave, y si no el local.
        // Siempre se imprime cuál se ha usado — nunca es una sorpresa.
        None => match ClaudePlanner::from_env() {
            Ok(p) => Ok(Box::new(p)),
            Err(_) => Ok(Box::new(LocalPlanner)),
        },
    }
}

// ------------------------------------------------------------------ tickets

fn cmd_tickets(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = spec::SpecEngine::global();
    if let Some(ticket_id) = args.first() {
        let detalle = engine.obtener_ticket(&ctx.workspace, ticket_id)?;
        match detalle {
            Some(t) => {
                println!(
                    "{} {}  {}",
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
            }
            None => {
                println!("ticket '{ticket_id}' no encontrado en el espacio de trabajo.");
            }
        }
    } else {
        let tickets = engine.listar_tickets(&ctx.workspace)?;
        if tickets.is_empty() {
            println!("no se encontraron tickets en el espacio de trabajo.");
            return Ok(());
        }

        println!("{}", paint("antOS · Catálogo y Hoja de Ruta de Tickets", BOLD));
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
            "  Total: {} tickets | {} completados | {} pendientes",
            tickets.len(),
            completados,
            tickets.len() - completados
        );
    }
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

            println!("\n{}", paint(&format!("antOS · Orquestador antFlow para {ticket_id}"), BOLD));
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
        _ => {
            bail!("subcomando desconocido para agent. Usa: antos agent run <ticket_id> | antos agent status [ticket_id] | antos agents");
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
            println!("  {} Variable .env: {}={}", paint("●", GREEN), paint(&info.env_var_key, BOLD), paint(&info.env_var_value, CYAN_COLOR));
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

const CYAN_COLOR: &str = "\x1b[36m";

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
