//! Módulo de Instalación en Metal, Particionamiento y Bootloader (Fase 15).

pub mod disk;
pub mod deploy;
pub mod bootloader;
pub mod cli;
pub mod usb;

pub use disk::DiskManager;
pub use deploy::DeployEngine;
pub use bootloader::BootloaderEngine;
pub use cli::{cmd_install, run_installer_wizard, HardwareInfo, InstallTomlConfig};
pub use usb::{cmd_usb, UsbManager};

