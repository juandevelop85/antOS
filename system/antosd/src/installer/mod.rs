//! Módulo de Instalación en Metal, Particionamiento y Bootloader (Fase 15).

pub mod disk;
pub mod deploy;

pub use disk::DiskManager;
pub use deploy::DeployEngine;
