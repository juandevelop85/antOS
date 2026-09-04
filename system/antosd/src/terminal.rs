//! The terminal session handler: prints and asks via stdin.
//!
//! This is one of the possible implementations of `SessionHandler`, not the
//! only nor the privileged one. All it knows how to do is draw what the
//! session hands it and return a decision.

use crate::capability::Tier;
use crate::preview::Line;
use crate::protocol::{SessionHandler, Proposal, ExecutionResult};
use anyhow::Result;
use std::io::Write;

pub const BOLD: &str = "1";
pub const DIM: &str = "2";
pub const RED: &str = "31";
pub const GREEN: &str = "32";
pub const YELLOW: &str = "33";
pub const BLUE: &str = "34";
pub const CYAN: &str = "36";

pub fn tier_color(tier: Tier) -> &'static str {
    match tier {
        Tier::Auto => GREEN,
        Tier::Confirm => YELLOW,
        Tier::Grant => RED,
    }
}

pub fn paint(text: &str, code: &str) -> String {
    if std::env::var_os("NO_COLOR").is_some() {
        return text.to_string();
    }
    format!("\x1b[{code}m{text}\x1b[0m")
}

pub fn ellipsis(s: &str, max: usize) -> String {
    let flat = s.replace('\n', "⏎");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max - 1).collect::<String>())
    }
}

pub struct Terminal {
    /// Answer yes without asking. Still passes through `on_proposal`, so the
    /// proposal is shown: what changes is who answers, not whether there was
    /// a gate.
    pub assume_yes: bool,
}

impl Terminal {
    pub fn new(assume_yes: bool) -> Self {
        Terminal { assume_yes }
    }
}

impl SessionHandler for Terminal {
    fn on_start(&mut self, intent: &str, planner: &str) -> Result<()> {
        println!();
        println!("{}", paint("antOS", BOLD));
        println!("  {}", paint(&format!("«{intent}»"), DIM));
        println!("  {}", paint(&format!("planner: {planner}"), DIM));
        Ok(())
    }

    fn on_note(&mut self, text: &str) -> Result<()> {
        println!("  {}", paint(&format!("note: {text}"), DIM));
        Ok(())
    }

    fn on_proposal(&mut self, proposal: &Proposal) -> Result<bool> {
        println!();
        println!("{}", paint("plan", BOLD));
        for (i, step) in proposal.plan.steps.iter().enumerate() {
            let args = step
                .args
                .iter()
                .map(|(k, v)| format!("{k}={}", ellipsis(v, 40)))
                .collect::<Vec<_>>()
                .join(" ");
            println!("  {}. {}  {}", i + 1, step.capability, paint(&args, DIM));
        }

        println!();
        println!("{}", paint("changes", BOLD));
        for line in &proposal.changes {
            match line {
                Line::Info(t) => println!("  {t}"),
                Line::Add(t) => println!("  {}", paint(&format!("+{t}"), GREEN)),
                Line::Del(t) => println!("  {}", paint(&format!("-{t}"), RED)),
            }
        }

        println!();
        println!("{}", paint("blast radius", BOLD));
        let radius = &proposal.blast_radius;
        for (label, paths) in [
            ("writes  ", &radius.writes),
            ("deletes ", &radius.deletes),
            ("reads   ", &radius.reads),
        ] {
            if !paths.is_empty() {
                println!("  {label}  {}", ellipsis(&paths.join(", "), 68));
            }
        }
        if !radius.system.is_empty() {
            println!(
                "  {}  {}",
                paint("SYSTEM  ", RED),
                ellipsis(&radius.system.join(", "), 68)
            );
        }
        if !radius.network.is_empty() {
            println!("  network   {}", radius.network.join(", "));
        }
        println!(
            "  level     {} {}",
            paint(proposal.tier.label(), tier_color(proposal.tier)),
            paint(&format!("— {}", proposal.reasons.join("; ")), DIM)
        );
        let enclosure_color = if proposal.enclosure.engine == "none" { RED } else { GREEN };
        println!(
            "  enclosure {} {}",
            paint(&proposal.enclosure.engine, enclosure_color),
            paint(&format!("— {}", proposal.enclosure.guarantees), DIM)
        );

        if proposal.dry_run {
            println!();
            println!("{}", paint("· dry run, nothing will be executed", DIM));
            return Ok(false);
        }

        // Auto tier: nothing to ask.
        if proposal.tier == Tier::Auto {
            return Ok(true);
        }

        if self.assume_yes {
            println!();
            println!("{}", paint("· approved without asking (--yes)", DIM));
            return Ok(true);
        }

        print!("\n{} ", paint("execute? [y/N]", BOLD));
        std::io::stdout().flush()?;
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            return Ok(false);
        }
        Ok(matches!(
            line.trim().to_lowercase().as_str(),
            "s" | "si" | "sí" | "y" | "yes"
        ))
    }

    fn on_output(&mut self, text: &str) -> Result<()> {
        println!();
        println!("{}", text.trim_end());
        Ok(())
    }

    fn on_result(&mut self, result: &ExecutionResult) -> Result<()> {
        println!();
        if !result.ok {
            println!("{} {}", paint("✗", RED), result.message);
            return Ok(());
        }
        match &result.snapshot {
            Some(id) => println!(
                "{} {}",
                paint(&result.message, GREEN),
                paint(&format!("· snapshot {id} · «antos undo» to revert"), DIM)
            ),
            None => println!("{}", paint(&result.message, GREEN)),
        }
        Ok(())
    }
}
