//! IPC handlers for the active project (T38.1).
//!
//! They live apart from [`super`] on purpose: the dispatch module is well
//! past the 800-line mark this repository uses as a signal to split, so new
//! families of requests get their own file and `mod.rs` only delegates.
//!
//! The three of them are thin: all the thinking is in
//! [`crate::projects`], which is also what the CLI uses, so `antos use` in
//! a terminal and `UseProject` from the bar cannot end up meaning different
//! things.

use crate::ctx::Ctx;
use antos_protocol::{Event, ProjectStatus};
use anyhow::Result;
use std::io::Write;

/// `ListProjects` → `Event::ProjectList`.
pub fn handle_list<W: Write>(ctx: &Ctx, writer: &mut W) -> Result<()> {
    let projects = crate::projects::list(&ctx.workspace, &ctx.state);
    super::transport::send(writer, &Event::ProjectList(projects))
}

/// `QueryProjectStatus` → `Event::ProjectStatus`.
///
/// Resolved on the spot, never cached: a selection made after the daemon
/// started has to be visible without restarting it.
pub fn handle_status<W: Write>(ctx: &Ctx, writer: &mut W) -> Result<()> {
    let status = crate::projects::resolve(&ctx.workspace, &ctx.state, None);
    super::transport::send(writer, &Event::ProjectStatus(status))
}

/// `UseProject` → `Event::ProjectChanged`, or `Event::Error` with the real
/// reason (a name that is not a directory name, a project that does not
/// exist, a state directory that cannot be written).
pub fn handle_use<W: Write>(ctx: &Ctx, writer: &mut W, name: Option<String>) -> Result<()> {
    match crate::projects::select(&ctx.workspace, &ctx.state, name.as_deref()) {
        Ok(status) => send_changed(writer, status),
        Err(err) => super::transport::send(writer, &Event::Error(format!("{err:#}"))),
    }
}

fn send_changed<W: Write>(writer: &mut W, status: ProjectStatus) -> Result<()> {
    super::transport::send(writer, &Event::ProjectChanged(status))
}
