//! Módulo de Instalación en Metal, Particionamiento y Bootloader (Fase 15).

pub mod bootloader;
pub mod cli;
pub mod deploy;
pub mod disk;
pub mod runner;
pub mod usb;

pub use bootloader::BootloaderEngine;
pub use cli::{cmd_install, run_installer_wizard, HardwareInfo, InstallTomlConfig};
pub use deploy::DeployEngine;
pub use disk::DiskManager;
pub use runner::{InstallRunner, SimulatedRunner, SystemRunner};
pub use usb::{cmd_usb, UsbManager};
