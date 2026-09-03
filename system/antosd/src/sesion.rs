//! El recorrido de una intención, sin pantalla.
//!
//! Esto era el cuerpo de un comando de terminal. Extraerlo tiene un motivo
//! concreto: un escritorio necesita el MISMO recorrido movido por otra
//! interfaz, y duplicarlo significaría duplicar también sus garantías — la
//! puerta de confirmación, el radio de impacto, la instantánea. Dos copias de
//! una garantía es una garantía que tarde o temprano se cumple solo en una.

use crate::blast::Blast;
use crate::capability::{Catalog, Tier};
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{self, Outcome, Record};
use crate::plan::{self, Plan, Step};
use crate::planner::Planner;
use crate::protocolo::{BlastRadius, Enclosure, ExecutionResult, Interlocutor, Proposal};
use crate::{exec, preview, sandbox, snapshot};
use anyhow::{bail, Result};

pub fn intencion(
    ctx: &Ctx,
    catalog: &Catalog,
    texto: &str,
    planificador: &dyn Planner,
    seco: bool,
    con: &mut dyn Interlocutor,
) -> Result<()> {
    con.inicio(texto, planificador.name())?;

    // 02 · planificación — la única etapa donde participa un modelo
    let propuesta_modelo = planificador.plan(texto, catalog)?;
    if let Some(nota) = &propuesta_modelo.nota {
        con.nota(nota)?;
    }

    // Nada de lo que devuelve un planificador se considera de fiar.
    let mut steps: Vec<Step> = Vec::new();
    for mut step in propuesta_modelo.steps {
        let cap = catalog.get(&step.capability)?;
        catalog.validate(cap, &mut step.args)?;
        steps.push(step);
    }

    let plan = Plan {
        id: plan::new_id(),
        intent: texto.to_string(),
        planner: planificador.name().to_string(),
        steps,
    };

    // 03 · radio de impacto, calculado ANTES de ejecutar nada
    let radius = Blast::compute(
        &plan,
        catalog,
        &ctx.workspace,
        &ctx.system_config,
        &ctx.state,
    )?;
    let (tier, reasons) = radius.required_tier();

    // Los cambios se calculan EN ORDEN, y cada paso ve lo que los anteriores
    // ya decidieron.
    let mut changes = Vec::new();
    let mut pendiente = exec::Pendiente::default();
    for step in &plan.steps {
        let del_paso = exec::changes_for(step, catalog.get(&step.capability)?, ctx, &pendiente)?;
        for cambio in &del_paso {
            pendiente.aplicar(cambio);
        }
        changes.extend(del_paso);
    }

    let jail = sandbox::for_host();

    let propuesta = Proposal {
        changes: preview::render(ctx, &changes),
        blast_radius: BlastRadius {
            writes: radius.writes.iter().map(|p| ctx.display(p)).collect(),
            deletes: radius.deletes.iter().map(|p| ctx.display(p)).collect(),
            reads: radius.reads.iter().map(|p| ctx.display(p)).collect(),
            system: radius
                .system
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            network: radius.network.iter().cloned().collect(),
        },
        tier,
        reasons: reasons.clone(),
        enclosure: Enclosure {
            engine: jail.name().to_string(),
            guarantees: jail.guarantees().to_string(),
        },
        dry_run: seco,
        plan: plan.clone(),
    };

    // 04 · política
    let mut record = Record {
        id: plan.id.clone(),
        at: chrono::Local::now().to_rfc3339(),
        intent: texto.to_string(),
        ticket_id: journal::extraer_ticket_id(texto),
        planner: planificador.name().to_string(),
        plan: plan.clone(),
        tier,
        reasons,
        outcome: Outcome::Cancelado,
        detail: None,
        snapshot: None,
        sandbox: jail.name().to_string(),
        reverted: false,
    };

    // Las denegaciones se resuelven ANTES de enseñar nada. Mostrar un plan
    // que no se puede ejecutar y pedir confirmación sería enseñar una puerta
    // que no existe.
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

    let grants = Grants::load(&ctx.grants_path()).unwrap_or_default();

    if tier == Tier::Grant {
        let missing: Vec<String> = plan
            .steps
            .iter()
            .map(|s| s.capability.clone())
            .filter(|c| {
                catalog
                    .get(c)
                    .map(|k| k.policy.tier == Tier::Grant)
                    .unwrap_or(false)
                    && !grants.is_granted(c)
            })
            .collect();
        if !missing.is_empty() {
            record.outcome = Outcome::Denegado;
            record.detail = Some(format!("sin concesión para: {}", missing.join(", ")));
            journal::append(&ctx.journal_path(), &record)?;
            bail!(
                "denegado por defecto: {} requiere concesión explícita.\n  \
                 Concédela con: antos grant {} --minutos 10",
                missing.join(", "),
                missing[0]
            );
        }
    }

    // La puerta. Siempre se pasa por aquí: lo que cambia entre interfaces es
    // quién contesta, no si hubo pregunta.
    let aprobado = con.propone(&propuesta)?;

    if seco {
        return Ok(());
    }
    if !aprobado {
        journal::append(&ctx.journal_path(), &record)?;
        con.resultado(&ExecutionResult {
            ok: false,
            message: "cancelado".into(),
            snapshot: None,
        })?;
        return Ok(());
    }

    // 05 · instantánea y ejecución
    let to_snapshot = radius.paths_to_snapshot();
    let snap = if to_snapshot.is_empty() {
        None
    } else {
        Some(snapshot::take(
            &plan.id,
            &to_snapshot,
            &ctx.snapshots_dir(),
        )?)
    };
    record.snapshot = snap.as_ref().map(|s| s.id.clone());

    let policy = sandbox::Policy::from_blast(&radius).with_grants(&grants, &ctx.workspace);
    match sandbox::run(&*jail, &changes, &policy) {
        Ok(outputs) => {
            record.outcome = Outcome::Ejecutado;
            journal::append(&ctx.journal_path(), &record)?;

            for salida in outputs.iter().filter(|o| !o.is_empty()) {
                con.salida(salida)?;
            }
            con.resultado(&ExecutionResult {
                ok: true,
                message: "✓ ejecutado".into(),
                snapshot: record.snapshot.clone(),
            })?;
            Ok(())
        }
        Err(e) => {
            // Si la ejecución se rompe a medias, el estado queda inconsistente.
            // La instantánea existe precisamente para este caso.
            record.outcome = Outcome::Fallido;
            record.detail = Some(e.to_string());
            journal::append(&ctx.journal_path(), &record)?;

            let pista =
                if e.to_string().contains("os error 1") || e.to_string().contains("os error 13") {
                    "\n  El recinto lo impidió: la capacidad intentó tocar algo que no había\n  \
                 declarado en sus efectos."
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
}
