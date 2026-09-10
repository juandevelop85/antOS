//! Tests de serialización/deserialización (round-trip) de los tipos del
//! protocolo IPC de antOS (T31.15): el fichero único `tests.rs` (2211
//! líneas, 43 tests independientes sin ninguna referencia cruzada entre
//! sí) se partió en los cuatro submódulos de abajo, agrupados por bloques
//! consecutivos — no hace falta ninguna re-exportación: son tests, no una
//! API que otro código del crate consuma.

mod dev_tools_and_kernel;
mod git_ticket_diff_notify_mesh;
mod storage_pkg_autopilot_web;
mod vfs_ebpf_profiler_dap_desktop;
