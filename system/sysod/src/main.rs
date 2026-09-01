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
use plan::Plan;
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
        "undo" => cmd_undo(&ctx),
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
    let forzar_local = std::env::var_os("SYSO_SIN_DEMONIO").is_some();
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
        println!("{}", paint("elige uno con: syso escucha --dispositivo N", DIM));
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
    println!("{}", paint("syso · escucha", BOLD));
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

fn cmd_undo(ctx: &Ctx) -> Result<()> {
    let mut records = journal::read_all(&ctx.journal_path())?;

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
        id: Plan::new_id(),
        at: chrono::Local::now().to_rfc3339(),
        intent: format!("deshacer {}", records[idx].id),
        planner: "syso".into(),
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
    let con_red = sandbox::Policy { writes: vec![], reads: vec![], dirs: vec![], network: true };
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
        println!(
            "  {mark} {}  {}  {}",
            paint(&r.id, DIM),
            paint(&format!("[{}]", r.tier.label()), tier_color(r.tier)),
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
        bail!("uso: syso grant <capacidad> [--minutos N]");
    };
    let cap = catalog.get(cap_name)?;
    if cap.policy.tier != Tier::Grant {
        bail!("{cap_name} es de nivel «{}»: no necesita concesión", cap.policy.tier.label());
    }
    let minutes = args
        .iter()
        .position(|a| a == "--minutos")
        .and_then(|i| args.get(i + 1))
        .and_then(|m| m.parse::<i64>().ok())
        .unwrap_or(10);

    let mut grants = Grants::load(&ctx.grants_path())?;
    grants.grant(cap_name, minutes);
    grants.save(&ctx.grants_path())?;
    println!(
        "{} {cap_name} durante {minutes} minutos",
        paint("concedida", GREEN)
    );
    Ok(())
}

fn cmd_revoke(ctx: &Ctx, args: &[String]) -> Result<()> {
    let Some(cap_name) = args.first() else {
        bail!("uso: syso revoke <capacidad>");
    };
    let mut grants = Grants::load(&ctx.grants_path())?;
    grants.revoke(cap_name);
    grants.save(&ctx.grants_path())?;
    println!("{} {cap_name}", paint("revocada", DIM));
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

fn help() {
    println!(
        "\
syso — el sistema hace lo que le pides, y puedes deshacerlo

  syso \"<intención>\"        planifica, enseña el diff y ejecuta
  syso escucha               lo mismo, dictado por voz (transcripción local)
  syso caps                  catálogo de capacidades y su nivel
  syso log                   bitácora de lo que ha pasado
  syso undo                  revierte el último plan ejecutado
  syso doctor                comprueba que el recinto es real, atacándolo
  syso demonio               atiende peticiones por socket (lo que usará el escritorio)
  syso grant <cap> [--minutos N]
  syso revoke <cap>

opciones
  -p, --planificador <local|claude>
  -s, --si                   no preguntar confirmación
  -n, --seco                 planificar y previsualizar sin ejecutar
      --segundos <N>         escucha: cuánto grabar (por defecto 5)
      --desde <fichero>      escucha: transcribir un audio en vez del micrófono
      --dispositivos         escucha: listar las entradas de audio
      --dispositivo <N>      escucha: cuál usar (por defecto, la del sistema)

entorno
  SYSO_WORKSPACE   espacio de trabajo (por defecto ./workspace)
  SYSO_STATE       instantáneas y bitácora (por defecto ./.syso)
  ANTHROPIC_API_KEY  activa el planificador con Claude"
    );
}
