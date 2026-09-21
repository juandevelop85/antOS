//! Comandos de `antos` agrupados por familia de subcomando (T31.15): el
//! fichero único `system.rs` (2065 líneas) se partió en los submódulos de
//! abajo, siguiendo el mismo patrón que `cli/commands/tools/`. Cada uno
//! conserva exactamente el mismo código que tenía en `system.rs`, solo
//! movido — sin cambios de lógica ni de API pública: todo `pub fn cmd_*`
//! sigue siendo alcanzable como `crate::cli::commands::system::cmd_*`
//! gracias a los `pub use *` de abajo.

mod autopilot;
mod barra_desktop;
mod boot_plugin_disk;
mod caps_and_grants;
mod install_usb_bootloader;
mod setup;
mod vm;
mod web;

pub use autopilot::*;
pub use barra_desktop::*;
pub use boot_plugin_disk::*;
pub use caps_and_grants::*;
pub use install_usb_bootloader::*;
pub use setup::*;
pub use vm::*;
pub use web::*;
