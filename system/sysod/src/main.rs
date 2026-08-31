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

    match exec::apply(&changes) {
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

            if let Some(snap) = &snap {
                snapshot::restore(snap)?;
                bail!("falló a mitad y se revirtió al estado anterior: {e}");
            }
            bail!("falló: {e}");
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
