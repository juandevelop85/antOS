#![allow(unused_imports, dead_code)]

extern crate antos_protocol;

use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use crate::capability::{Catalog, Tier};
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{Outcome, Record};
use crate::planner::{
    claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner,
    openai_compat::OpenAiCompatPlanner, Planner,
};
use crate::terminal::{ellipsis, paint, tier_color, BLUE, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use crate::cli::args::Opts;

pub fn cmd_undo(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut records = crate::journal::read_all(&ctx.journal_path())?;

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
            if r.outcome == Outcome::Executed && !r.reverted && r.snapshot.is_some() {
                let coincide_ticket = r.ticket_id.as_deref() == Some(&tid)
                    || crate::journal::extract_ticket_id(&r.intent).as_deref() == Some(&tid);
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
            let Some(snap_id) = records[idx].snapshot.clone() else {
                // The loop above only ever pushed indices where
                // `snapshot.is_some()`; reaching this means the journal
                // changed under us mid-loop, which is worth a clear error
                // rather than a panic.
                bail!("el registro «{}» no tiene instantánea asociada (estado del journal inconsistente)", records[idx].id);
            };
            let snap = crate::snapshot::load(&snap_id, &ctx.snapshots_dir())?;

            println!(
                "  {} Transacción {}: «{}»",
                paint("↩", DIM),
                paint(&records[idx].id, DIM),
                records[idx].intent
            );
            for line in crate::snapshot::restore(&snap)? {
                println!("    {line}");
            }

            records[idx].reverted = true;
            let undone = Record {
                id: crate::plan::new_id(),
                at: chrono::Local::now().to_rfc3339(),
                intent: format!("deshacer ticket {tid} ({})", records[idx].id),
                ticket_id: Some(tid.clone()),
                planner: "antos".into(),
                plan: records[idx].plan.clone(),
                tier: records[idx].tier,
                reasons: vec![format!("revierte instantánea {snap_id} del ticket {tid}")],
                outcome: Outcome::Reverted,
                detail: None,
                sandbox: "broker".into(),
                snapshot: Some(snap_id),
                reverted: false,
            };
            crate::journal::append(&ctx.journal_path(), &undone)?;
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

        crate::journal::rewrite(&ctx.journal_path(), &records)?;
        println!(
            "\n  {} Reversión completada: {} transacción(es) revertida(s).\n",
            paint("✓", GREEN),
            reversiones
        );
        return Ok(());
    }

    let idx = records
        .iter()
        .rposition(|r| r.outcome == Outcome::Executed && !r.reverted && r.snapshot.is_some());

    let Some(idx) = idx else {
        println!("no hay nada que deshacer");
        return Ok(());
    };

    let Some(snap_id) = records[idx].snapshot.clone() else {
        // `rposition` above only ever selects an index where
        // `snapshot.is_some()`; reaching this means the invariant broke.
        bail!("el registro «{}» no tiene instantánea asociada (estado del journal inconsistente)", records[idx].id);
    };
    let snap = crate::snapshot::load(&snap_id, &ctx.snapshots_dir())?;

    println!();
    println!(
        "{} {}",
        paint("deshaciendo", BOLD),
        paint(&format!("«{}»", records[idx].intent), DIM)
    );
    for line in crate::snapshot::restore(&snap)? {
        println!("  {line}");
    }

    records[idx].reverted = true;
    let undone = Record {
        id: crate::plan::new_id(),
        at: chrono::Local::now().to_rfc3339(),
        intent: format!("deshacer {}", records[idx].id),
        ticket_id: records[idx].ticket_id.clone(),
        planner: "antos".into(),
        plan: records[idx].plan.clone(),
        tier: records[idx].tier,
        reasons: vec![format!("revierte la instantánea {snap_id}")],
        outcome: Outcome::Reverted,
        detail: None,
        sandbox: "broker".into(),
        snapshot: Some(snap_id),
        reverted: false,
    };
    crate::journal::rewrite(&ctx.journal_path(), &records)?;
    crate::journal::append(&ctx.journal_path(), &undone)?;

    println!();
    println!("{}", paint("✓ revertido", GREEN));
    Ok(())
}

// ------------------------------------------------------------ otros comandos

