//! Comandos de `antos` agrupados por familia de subcomando (T31.15): el
//! fichero único `tools.rs` (3592 líneas, sin ningún test — T31.13) se
//! partió en los submódulos de abajo. Cada uno conserva exactamente el
//! mismo código que tenía en `tools.rs`, solo movido — sin cambios de
//! lógica ni de API pública: todo `pub fn cmd_*` sigue siendo alcanzable
//! como `crate::cli::commands::tools::cmd_*` gracias a los `pub use *`
//! de abajo.

mod bench_forge;
mod doc_qa;
mod ebpf_profile;
mod llm;
mod memory_env;
mod ports_services_secrets;
mod project;
mod quota_diff;
mod terminal_notify;
mod testing;

pub use bench_forge::*;
pub use doc_qa::*;
pub use ebpf_profile::*;
pub use llm::*;
pub use memory_env::*;
pub use ports_services_secrets::*;
pub use project::*;
pub use quota_diff::*;
pub use terminal_notify::*;
pub use testing::*;
