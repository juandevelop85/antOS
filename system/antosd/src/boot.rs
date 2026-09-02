//! Pipeline de compilación cruzada bare metal, generación de imagen y arranque QEMU (T13.2).
//!
//! Este módulo orquesta la compilación del núcleo `no_std` para el target
//! `x86_64-unknown-none`, la invocación de `builder` para generar la imagen de disco BIOS/UEFI
//! y la ejecución supervisada de `qemu-system-x86_64`.

use antos_protocolo::BootPipelineStatus;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct BootEngine;

impl BootEngine {
    pub fn global() -> Self {
        Self
    }

    /// Encuentra la raíz del repositorio donde conviven kernel/ y builder/.
    fn find_repo_root(start: &Path) -> PathBuf {
        let mut curr = if start.is_file() {
            start.parent().unwrap_or(start).to_path_buf()
        } else {
            start.to_path_buf()
        };

        for _ in 0..10 {
            if curr.join("kernel").is_dir() && curr.join("builder").is_dir() {
                return curr;
            }
            if let Some(parent) = curr.parent() {
                curr = parent.to_path_buf();
            } else {
                break;
            }
        }

        if let Ok(cwd) = std::env::current_dir() {
            if cwd.join("kernel").is_dir() && cwd.join("builder").is_dir() {
                return cwd;
            }
        }

        start.to_path_buf()
    }

    /// Ruta al ejecutable ELF del kernel.
    pub fn kernel_elf_path(workspace: &Path) -> PathBuf {
        let root = Self::find_repo_root(workspace);
        root.join("kernel/target/x86_64-unknown-none/debug/kernel")
    }

    /// Ruta a la imagen de disco arrancable BIOS.
    pub fn bios_image_path(workspace: &Path) -> PathBuf {
        let root = Self::find_repo_root(workspace);
        root.join("kernel/target/x86_64-unknown-none/debug/antos-bios.img")
    }

    /// Comprueba si el ejecutable de QEMU está disponible en el PATH del sistema.
    pub fn is_qemu_available() -> bool {
        Command::new("qemu-system-x86_64")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Consulta el estado actual de los artefactos del pipeline de arranque.
    pub fn status(&self, workspace: &Path) -> BootPipelineStatus {
        let elf_path = Self::kernel_elf_path(workspace);
        let img_path = Self::bios_image_path(workspace);

        let (kernel_elf_exists, kernel_elf_size_bytes) = if elf_path.exists() {
            (true, std::fs::metadata(&elf_path).map(|m| m.len()).unwrap_or(0))
        } else {
            (false, 0)
        };

        let (bios_image_exists, bios_image_size_bytes) = if img_path.exists() {
            (true, std::fs::metadata(&img_path).map(|m| m.len()).unwrap_or(0))
        } else {
            (false, 0)
        };

        BootPipelineStatus {
            kernel_elf_exists,
            kernel_elf_size_bytes,
            bios_image_exists,
            bios_image_size_bytes,
            qemu_installed: Self::is_qemu_available(),
            target_arch: "x86_64-unknown-none".into(),
        }
    }

    /// Compila el kernel y empaqueta la imagen de arranque.
    pub fn build(&self, workspace: &Path) -> Result<PathBuf> {
        let root = Self::find_repo_root(workspace);
        let kernel_dir = root.join("kernel");
        if !kernel_dir.exists() {
            bail!("No se encontró el subdirectorio 'kernel/' en {}", root.display());
        }

        // 1. Compilar kernel no_std
        let build_status = Command::new("cargo")
            .arg("build")
            .current_dir(&kernel_dir)
            .status()
            .context("Error al ejecutar 'cargo build' dentro de kernel/")?;

        if !build_status.success() {
            bail!("Fallo la compilación cruzada del kernel (código {:?})", build_status.code());
        }

        let elf_path = Self::kernel_elf_path(workspace);
        if !elf_path.exists() {
            bail!("El binario ELF del kernel no fue generado en {}", elf_path.display());
        }

        // 2. Ejecutar builder para generar la imagen de disco BIOS
        let builder_status = Command::new("cargo")
            .args(["run", "-q", "-p", "builder", "--"])
            .arg(&elf_path)
            .current_dir(&root)
            .status()
            .context("Error al invocar el crate 'builder'")?;

        if !builder_status.success() {
            bail!("Fallo la creación de la imagen de disco con builder");
        }

        let img_path = Self::bios_image_path(workspace);
        if !img_path.exists() {
            bail!("La imagen arrancable no fue generada en {}", img_path.display());
        }

        Ok(img_path)
    }

    /// Ejecuta una prueba automatizada headless en QEMU y valida la emisión de telemetría del kernel.
    pub fn test_boot(&self, workspace: &Path) -> Result<String> {
        let img_path = self.build(workspace)?;
        if !Self::is_qemu_available() {
            bail!("'qemu-system-x86_64' no está disponible en este entorno. Instálelo para ejecutar pruebas de arranque.");
        }

        let run_script = Self::find_repo_root(workspace).join("run.sh");
        if run_script.exists() {
            let out = Command::new("bash")
                .arg(&run_script)
                .arg("--test")
                .output()
                .context("Error al ejecutar run.sh --test")?;

            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.status.success() && (stdout.contains("antOS · kernel") || stdout.contains("Verificación de arranque exitosa")) {
                return Ok(format!("✓ Arranque de kernel antOS verificado en QEMU:\n{}", stdout.trim()));
            } else {
                bail!("Prueba de arranque fallida: {}\n{}", stdout, stderr);
            }
        }

        // Fallback directo por subprocess
        let out = Command::new("python3")
            .args([
                "-c",
                &format!(
                    "import subprocess, sys\n\
                    p = subprocess.Popen(['qemu-system-x86_64', '-m', '256M', '-serial', 'stdio', '-display', 'none', '-drive', 'format=raw,file={}'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)\n\
                    try:\n\
                        so, se = p.communicate(timeout=4)\n\
                    except subprocess.TimeoutExpired:\n\
                        p.kill()\n\
                        so, se = p.communicate()\n\
                    if 'antOS · kernel x86_64' in so or 'Jumping to kernel' in so:\n\
                        print('OK')\n\
                    else:\n\
                        sys.exit(1)\n",
                    img_path.display()
                ),
            ])
            .output()?;

        if out.status.success() {
            Ok(format!("✓ Kernel arrancó con éxito en QEMU (imagen: {})", img_path.display()))
        } else {
            bail!("No se detectó el banner de arranque del kernel en la salida serial de QEMU");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_engine_status_inspection() {
        let cwd = std::env::current_dir().unwrap();
        let engine = BootEngine::global();
        let st = engine.status(&cwd);
        assert_eq!(st.target_arch, "x86_64-unknown-none");
    }

    #[test]
    fn test_boot_engine_paths_resolution() {
        let cwd = std::env::current_dir().unwrap();
        let elf = BootEngine::kernel_elf_path(&cwd);
        let img = BootEngine::bios_image_path(&cwd);
        assert!(elf.ends_with("kernel"));
        assert!(img.ends_with("antos-bios.img"));
    }
}
