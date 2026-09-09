//! The lifecycle of an intent, without a screen.
//!
//! This was the body of a terminal command. Extracting it has a concrete
//! reason: a desktop needs the SAME lifecycle driven by another interface,
//! and duplicating it would also duplicate its guarantees — the confirmation
//! gate, the blast radius, the snapshot. Two copies of a guarantee is a
//! guarantee that sooner or later holds only in one.

use crate::blast::Blast;
use crate::capability::{Catalog, Tier};
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{self, Outcome, Record};
use crate::plan::{self, Plan, Step};
use crate::planner::Planner;
use crate::protocol::{BlastRadius, Enclosure, ExecutionResult, Proposal, SessionHandler};
use crate::{exec, preview, sandbox, snapshot};
use anyhow::{bail, Result};

pub fn intent_session(
    ctx: &Ctx,
    catalog: &Catalog,
    text: &str,
    planner_ref: &dyn Planner,
    dry_run: bool,
    handler: &mut dyn SessionHandler,
) -> Result<()> {
    handler.on_start(text, planner_ref.name())?;

    // 02 · planning — the only stage where a model participates
    let model_proposal = planner_ref.plan(text, catalog)?;
    if let Some(note) = &model_proposal.nota {
        handler.on_note(note)?;
    }

    // Nothing returned by a planner is considered trustworthy.
    let mut steps: Vec<Step> = Vec::new();
    for mut step in model_proposal.steps {
        let cap = catalog.get(&step.capability)?;
        catalog.validate(cap, &mut step.args)?;
        steps.push(step);
    }

    let plan = Plan {
        id: plan::new_id(),
        intent: text.to_string(),
        planner: planner_ref.name().to_string(),
        steps,
    };

    // 03 · blast radius, computed BEFORE executing anything
    let radius = Blast::compute(
        &plan,
        catalog,
        &ctx.workspace,
        &ctx.system_config,
        &ctx.state,
    )?;
    let (tier, reasons) = radius.required_tier();

    // Changes are computed IN ORDER, and each step sees what the previous
    // ones have already decided.
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

    let proposal = Proposal {
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
        dry_run,
        plan: plan.clone(),
    };

    // 04 · policy
    let mut record = Record {
        id: plan.id.clone(),
        at: chrono::Local::now().to_rfc3339(),
        intent: text.to_string(),
        ticket_id: journal::extract_ticket_id(text),
        planner: planner_ref.name().to_string(),
        plan: plan.clone(),
        tier,
        reasons,
        outcome: Outcome::Cancelled,
        detail: None,
        snapshot: None,
        sandbox: jail.name().to_string(),
        reverted: false,
    };

    // Denials are resolved BEFORE showing anything. Showing a plan that
    // cannot be executed and asking for confirmation would show a door
    // that does not exist.
    if !radius.escapes.is_empty() {
        record.outcome = Outcome::Denied;
        record.detail = Some("plan escapes the workspace".into());
        journal::append(&ctx.journal_path(), &record)?;
        bail!(
            "denied: plan touches paths outside the workspace:\n  {}",
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
            record.outcome = Outcome::Denied;
            record.detail = Some(format!("no grant for: {}", missing.join(", ")));
            journal::append(&ctx.journal_path(), &record)?;
            bail!(
                "denied by default: {} requires explicit grant.\n  \
                 Grant with: antos grant {} --minutes 10",
                missing.join(", "),
                missing[0]
            );
        }
    }

    // The gate. Always passes through here: what changes between interfaces
    // is who answers, not whether there was a question.
    let approved = handler.on_proposal(&proposal)?;

    if dry_run {
        return Ok(());
    }
    if !approved {
        journal::append(&ctx.journal_path(), &record)?;
        handler.on_result(&ExecutionResult {
            ok: false,
            message: "cancelled".into(),
            snapshot: None,
        })?;
        return Ok(());
    }

    // 05 · snapshot and execution
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
            record.outcome = Outcome::Executed;
            journal::append(&ctx.journal_path(), &record)?;

            for output in outputs.iter().filter(|o| !o.is_empty()) {
                handler.on_output(output)?;
            }
            handler.on_result(&ExecutionResult {
                ok: true,
                message: "✓ executed".into(),
                snapshot: record.snapshot.clone(),
            })?;
            Ok(())
        }
        Err(e) => {
            // If execution breaks midway, state is inconsistent.
            // The snapshot exists precisely for this case.
            record.outcome = Outcome::Failed;
            record.detail = Some(e.to_string());
            journal::append(&ctx.journal_path(), &record)?;

            let hint =
                if e.to_string().contains("os error 1") || e.to_string().contains("os error 13") {
                    "\n  The enclosure prevented it: the capability tried to touch something \
                     it had not declared in its effects."
                } else {
                    ""
                };

            if let Some(snap) = &snap {
                snapshot::restore(snap)?;
                bail!("failed midway and reverted to previous state: {e}{hint}");
            }
            bail!("failed: {e}{hint}")
        }
    }
}

/// Backwards compatibility alias.
#[deprecated(note = "use intent_session")]
pub fn intencion(
    ctx: &Ctx,
    catalog: &Catalog,
    texto: &str,
    planificador: &dyn Planner,
    seco: bool,
    con: &mut dyn SessionHandler,
) -> Result<()> {
    intent_session(ctx, catalog, texto, planificador, seco, con)
}
