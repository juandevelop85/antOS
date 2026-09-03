//! A plan is what the planner produces: an ordered list
//! of capability invocations. Never a shell line.

pub use antos_protocol::{Plan, Step};

/// Human-readable and sortable identifier: the log is read in chronological order
/// without having to interpret anything. Milliseconds avoid collisions between
/// two plans in the same second.
pub fn new_id() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string()
}
