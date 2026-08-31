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
mod journal;
mod plan;
mod planner;
mod preview;
mod sandbox;
mod snapshot;

use anyhow::{bail, Result};
use blast::Blast;
use capability::{Catalog, Tier};
use ctx::Ctx;
use grants::Grants;
use journal::{Outcome, Record};
use plan::{Plan, Step};
use planner::{claude::ClaudePlanner, local::LocalPlanner, Planner};
use preview::Line;
use std::io::Write;

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
        "doctor" => cmd_doctor(&ctx),
        "log" => cmd_log(&ctx),
        "undo" => cmd_undo(&ctx),
        "grant" => cmd_grant(&ctx, &catalog, &rest[1..]),
        "revoke" => cmd_revoke(&ctx, &rest[1..]),
        _ => cmd_intent(&ctx, &catalog, &rest.join(" "), &opts),
    }
}

// ---------------------------------------------------------------- intención

fn cmd_intent(ctx: &Ctx, catalog: &Catalog, intent: &str, opts: &Opts) -> Result<()> {
    // 01 · captura
    let planner = pick_planner(opts)?;

    println!();
    println!("{}", paint("syso", BOLD));
    println!("  {}", paint(&format!("«{intent}»"), DIM));
    println!("  {}", paint(&format!("planificador: {}", planner.name()), DIM));

    // 02 · planificación — la única etapa donde participa un modelo
    let raw_steps = planner.plan(intent, catalog)?;

    // Nada de lo que devuelve un planificador se considera de fiar.
    let mut steps: Vec<Step> = Vec::new();
    for mut step in raw_steps {
        let cap = catalog.get(&step.capability)?;
        catalog.validate(cap, &mut step.args)?;
        steps.push(step);
    }

    let plan = Plan {
        id: Plan::new_id(),
        intent: intent.to_string(),
        planner: planner.name().to_string(),
        steps,
    };

    println!();
    println!("{}", paint("plan", BOLD));
    for (i, step) in plan.steps.iter().enumerate() {
        let args = step
            .args
            .iter()
            .map(|(k, v)| format!("{k}={}", ellipsis(v, 40)))
            .collect::<Vec<_>>()
            .join(" ");
        println!("  {}. {}  {}", i + 1, step.capability, paint(&args, DIM));
    }

    // 03 · radio de impacto, calculado ANTES de ejecutar nada
    let radius = Blast::compute(&plan, catalog, &ctx.workspace)?;
    let (tier, reasons) = radius.required_tier();

    let mut changes = Vec::new();
    for step in &plan.steps {
        changes.extend(exec::changes_for(step, catalog.get(&step.capability)?, ctx)?);
    }

    println!();
    println!("{}", paint("cambios", BOLD));
    for line in preview::render(ctx, &changes) {
        match line {
            Line::Info(t) => println!("  {t}"),
            Line::Add(t) => println!("  {}", paint(&format!("+{t}"), GREEN)),
            Line::Del(t) => println!("  {}", paint(&format!("-{t}"), RED)),
        }
    }

    // Los efectos DECLARADOS, que son de donde sale el nivel de permiso.
    // No es lo mismo que la sección «cambios»: aquello es lo que se va a
    // escribir, esto es el alcance que la capacidad se comprometió a tener.
    println!();
    println!("{}", paint("radio de impacto", BOLD));
    for (etiqueta, rutas) in [
        ("escribe ", &radius.writes),
        ("borra   ", &radius.deletes),
        ("lee     ", &radius.reads),
    ] {
        if !rutas.is_empty() {
            let lista = rutas.iter().map(|p| ctx.display(p)).collect::<Vec<_>>().join(", ");
            println!("  {etiqueta}  {}", ellipsis(&lista, 68));
        }
    }
    if !radius.network.is_empty() {
        println!("  red       {}", radius.network.iter().cloned().collect::<Vec<_>>().join(", "));
    }
    println!(
        "  nivel     {} {}",
        paint(tier.label(), tier_color(tier)),
        paint(&format!("— {}", reasons.join("; ")), DIM)
    );

    let jail = sandbox::for_host();
    let recinto_color = if jail.name() == "ninguno" { RED } else { GREEN };
    println!(
        "  recinto   {} {}",
        paint(jail.name(), recinto_color),
        paint(&format!("— {}", jail.guarantees()), DIM)
    );

    // 04 · política
    let mut record = Record {
        id: plan.id.clone(),
        at: chrono::Local::now().to_rfc3339(),
        intent: intent.to_string(),
        planner: planner.name().to_string(),
        plan: plan.clone(),
        tier,
        reasons,
        outcome: Outcome::Cancelado,
        detail: None,
        snapshot: None,
        sandbox: jail.name().to_string(),
        reverted: false,
    };

    // Salirse del espacio de trabajo no se negocia con una confirmación.
    if !radius.escapes.is_empty() {
        record.outcome = Outcome::Denegado;
        record.detail = Some("el plan sale del espacio de trabajo".into());
        journal::append(&ctx.journal_path(), &record)?;
        bail!(
            "denegado: el plan toca rutas fuera del espacio de trabajo:\n  {}",
            radius
                .escapes
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    }

    if tier == Tier::Grant {
        let grants = Grants::load(&ctx.grants_path())?;
        let missing: Vec<String> = plan
            .steps
            .iter()
            .map(|s| s.capability.clone())
            .filter(|c| {
                catalog.get(c).map(|k| k.policy.tier == Tier::Grant).unwrap_or(false)
                    && !grants.is_granted(c)
            })
            .collect();
        if !missing.is_empty() {
            record.outcome = Outcome::Denegado;
            record.detail = Some(format!("sin concesión para: {}", missing.join(", ")));
            journal::append(&ctx.journal_path(), &record)?;
            bail!(
                "denegado por defecto: {} requiere concesión explícita.\n  Concédela con: syso grant {} --minutos 10",
                missing.join(", "),
                missing[0]
            );
        }
    }

    if opts.dry_run {
        println!();
        println!("{}", paint("· marcha en seco, no se ejecuta nada", DIM));
        return Ok(());
    }

    if tier > Tier::Auto && !opts.assume_yes && !confirm()? {
        journal::append(&ctx.journal_path(), &record)?;
        println!("{}", paint("cancelado", DIM));
        return Ok(());
    }

    // 05 · instantánea y ejecución
    let to_snapshot = radius.paths_to_snapshot();
    let snap = if to_snapshot.is_empty() {
        None
    } else {
        Some(snapshot::take(&plan.id, &to_snapshot, &ctx.snapshots_dir())?)
    };
    record.snapshot = snap.as_ref().map(|s| s.id.clone());

    // La ejecución ocurre en otro proceso, dentro del recinto. Lo que el
    // recinto permite sale del radio de impacto: exactamente lo declarado.
    let policy = sandbox::Policy::from_blast(&radius);
    match sandbox::run(&*jail, &changes, &policy) {
        Ok(outputs) => {
            record.outcome = Outcome::Ejecutado;
            journal::append(&ctx.journal_path(), &record)?;

            for out in outputs.iter().filter(|o| !o.is_empty()) {
                println!();
                println!("{}", out.trim_end());
            }
            println!();
            match &record.snapshot {
                Some(id) => println!(
                    "{} {}",
                    paint("✓ ejecutado", GREEN),
                    paint(&format!("· instantánea {id} · «syso undo» lo revierte"), DIM)
                ),
                None => println!("{}", paint("✓ ejecutado", GREEN)),
            }
        }
        Err(e) => {
            // Si la ejecución se rompe a medias, el estado queda inconsistente.
            // La instantánea existe precisamente para este caso.
            record.outcome = Outcome::Fallido;
            record.detail = Some(e.to_string());
            journal::append(&ctx.journal_path(), &record)?;

            let pista = if e.to_string().contains("os error 1") {
                "\n  El recinto lo impidió: la capacidad intentó tocar algo que no había\n  declarado en sus efectos. Corrige el manifiesto o compón el plan con una\n  capacidad que sí lo declare."
            } else {
                ""
            };
            if let Some(snap) = &snap {
                snapshot::restore(snap)?;
                bail!("falló a mitad y se revirtió al estado anterior: {e}{pista}");
            }
            bail!("falló: {e}{pista}");
        }
    }
    Ok(())
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

fn pick_planner(opts: &Opts) -> Result<Box<dyn Planner>> {
    match opts.planner.as_deref() {
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

fn confirm() -> Result<bool> {
    print!("\n{} ", paint("¿ejecutar? [s/N]", BOLD));
    std::io::stdout().flush()?;
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line)? == 0 {
        return Ok(false);
    }
    let answer = line.trim().to_lowercase();
    Ok(matches!(answer.as_str(), "s" | "si" | "sí" | "y" | "yes"))
}

fn ellipsis(s: &str, max: usize) -> String {
    let flat = s.replace('\n', "⏎");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max - 1).collect::<String>())
    }
}

fn help() {
    println!(
        "\
syso — el sistema hace lo que le pides, y puedes deshacerlo

  syso \"<intención>\"        planifica, enseña el diff y ejecuta
  syso caps                  catálogo de capacidades y su nivel
  syso log                   bitácora de lo que ha pasado
  syso undo                  revierte el último plan ejecutado
  syso doctor                comprueba que el recinto es real, atacándolo
  syso grant <cap> [--minutos N]
  syso revoke <cap>

opciones
  -p, --planificador <local|claude>
  -s, --si                   no preguntar confirmación
  -n, --seco                 planificar y previsualizar sin ejecutar

entorno
  SYSO_WORKSPACE   espacio de trabajo (por defecto ./workspace)
  SYSO_STATE       instantáneas y bitácora (por defecto ./.syso)
  ANTHROPIC_API_KEY  activa el planificador con Claude"
    );
}

// -------------------------------------------------------------------- color

const BOLD: &str = "1";
const DIM: &str = "2";
const RED: &str = "31";
const GREEN: &str = "32";
const YELLOW: &str = "33";

fn tier_color(tier: Tier) -> &'static str {
    match tier {
        Tier::Auto => GREEN,
        Tier::Confirm => YELLOW,
        Tier::Grant => RED,
    }
}

fn paint(text: &str, code: &str) -> String {
    if std::env::var_os("NO_COLOR").is_some() {
        return text.to_string();
    }
    format!("\x1b[{code}m{text}\x1b[0m")
}
