//! `antos vm` (registro simulado de microVMs, T31.4).
#![allow(unused_imports, dead_code)]

extern crate antos_protocol;

use crate::capability::{Catalog, Tier};
use crate::cli::args::Opts;
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{Outcome, Record};
use crate::planner::{
    claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner,
    openai_compat::OpenAiCompatPlanner, Planner,
};
use crate::terminal::{ellipsis, paint, tier_color, BLUE, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Argumentos de `antos vm spawn` (T31.13). `vm_id` queda en `None` cuando
/// no se especifica «--id»: el identificador por defecto depende del reloj
/// (marca de tiempo), así que se resuelve fuera de esta función pura para
/// que el análisis en sí sea determinista y comprobable.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VmSpawnArgs {
    pub vm_id: Option<String>,
    pub vcpu_count: u8,
    pub memory_mb: u32,
    pub kernel_image: String,
    pub command: Option<String>,
}

/// Analiza los argumentos de `antos vm spawn|start|run` (T31.13).
fn parse_vm_spawn_args(args: &[String]) -> VmSpawnArgs {
    let mut vm_id = None;
    let mut vcpu_count = 2u8;
    let mut memory_mb = 512u32;
    let mut kernel_image = "/boot/antos-vmlinuz".to_string();
    let mut command = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--id" if i + 1 < args.len() => {
                vm_id = Some(args[i + 1].clone());
                i += 1;
            }
            "--cpus" | "-c" if i + 1 < args.len() => {
                if let Ok(c) = args[i + 1].parse::<u8>() {
                    vcpu_count = c;
                }
                i += 1;
            }
            "--memory" | "-m" if i + 1 < args.len() => {
                if let Ok(m) = args[i + 1].parse::<u32>() {
                    memory_mb = m;
                }
                i += 1;
            }
            "--kernel" | "-k" if i + 1 < args.len() => {
                kernel_image = args[i + 1].clone();
                i += 1;
            }
            "--cmd" if i + 1 < args.len() => {
                command = Some(args[i + 1].clone());
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    VmSpawnArgs {
        vm_id,
        vcpu_count,
        memory_mb,
        kernel_image,
        command,
    }
}

pub fn cmd_vm(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "spawn" | "start" | "run" => {
            let VmSpawnArgs {
                vm_id,
                vcpu_count,
                memory_mb,
                kernel_image,
                command,
            } = parse_vm_spawn_args(args);
            let vm_id = vm_id
                .unwrap_or_else(|| format!("vm-{}", chrono::Local::now().format("%Y%m%d%H%M%S")));

            println!(
                "\n{} Instanciando microVM con aislamiento por hipervisor...",
                paint("antOS MicroVM ·", BOLD)
            );
            let cfg = antos_protocol::MicrovmConfig {
                vm_id: vm_id.clone(),
                vcpu_count,
                memory_mb,
                kernel_image: kernel_image.clone(),
                initrd_image: None,
                overlay_disk: None,
                vsock_port: 5252,
                command,
            };

            let instance = crate::vm::MicrovmManager::spawn_vm(&ctx.state, &cfg)?;
            println!(
                "  {} MicroVM «{}» arrancada exitosamente",
                paint("✓", GREEN),
                paint(&instance.id, BOLD)
            );
            println!("    • PID:          {}", instance.pid);
            println!("    • vCPUs:        {}", instance.vcpus);
            println!("    • Memoria:      {} MB", instance.memory_mb);
            println!("    • Puerto vsock: {}", instance.vsock_port);
            println!("    • Estado:       {}\n", paint(&instance.status, GREEN));
        }
        "exec" => {
            let vm_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Uso: antos vm exec <VM_ID> <comando>"))?;
            let command = if args.len() > 2 {
                args[2..].join(" ")
            } else {
                bail!("Uso: antos vm exec <VM_ID> <comando>");
            };

            println!(
                "\n{} Ejecutando comando en microVM «{}»...",
                paint("antOS MicroVM ·", BOLD),
                paint(vm_id, CYAN)
            );
            let res = crate::vm::MicrovmManager::exec_vm(&ctx.state, vm_id, &command)?;
            let status_badge = if res.success {
                paint("EXITOSO", GREEN)
            } else {
                paint("FALLIDO", RED)
            };
            println!("  Resultado:  {} (código {})", status_badge, res.exit_code);
            println!("  Duración:   {} ms", res.duration_ms);
            if !res.stdout.is_empty() {
                println!("\n  Salida:\n{}", res.stdout.trim());
            }
            if !res.stderr.is_empty() {
                println!("\n  Errores:\n{}", paint(&res.stderr, RED));
            }
            println!();
        }
        "list" | "ls" => {
            println!(
                "\n{} MicroVMs Activas en el Sistema:",
                paint("antOS MicroVM ·", BOLD)
            );
            let vms = crate::vm::MicrovmManager::list_vms(&ctx.state)?;
            if vms.is_empty() {
                println!("  (no hay microVMs activas en este momento)\n");
            } else {
                for v in &vms {
                    println!(
                        "  • [{}] {} (PID {}, {} vCPUs, {} MB RAM, vsock {})",
                        paint(&v.id, BOLD),
                        paint(&v.status, GREEN),
                        v.pid,
                        v.vcpus,
                        v.memory_mb,
                        v.vsock_port
                    );
                }
                println!();
            }
        }
        "kill" | "stop" | "destroy" => {
            let vm_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Uso: antos vm kill <VM_ID>"))?;
            crate::vm::MicrovmManager::kill_vm(&ctx.state, vm_id)?;
            println!(
                "\n{} MicroVM «{}» detenida y eliminada.\n",
                paint("✓", GREEN),
                paint(vm_id, BOLD)
            );
        }
        _ => {
            println!(
                "\n{} Diagnóstico de Hipervisor y MicroVMs:",
                paint("antOS MicroVM ·", BOLD)
            );
            let st = crate::vm::MicrovmManager::get_status(&ctx.state)?;
            let kvm_badge = if st.kvm_available {
                paint("Disponible (/dev/kvm)", GREEN)
            } else {
                paint("No detectado (Emulación)", YELLOW)
            };
            println!("  • Soporte KVM:             {}", kvm_badge);
            println!(
                "  • Motor de Hipervisor:     {}",
                paint(&st.hypervisor_engine, CYAN)
            );
            println!("  • Kernel del Host:         {}", st.kernel_version);
            println!("  • MicroVMs activas:        {}", st.active_vms_count);
            println!(
                "  • Memoria asignada a VMs:  {} MB",
                st.total_memory_allocated_mb
            );
            println!(
                "  • Canales vsock:           {}",
                if st.vsock_supported {
                    paint("Soportado", GREEN)
                } else {
                    paint("No disponible", RED)
                }
            );
            println!("\n  Uso:");
            println!("    antos vm spawn [--cpus N] [--memory MB]   Arranca una microVM efímera");
            println!(
                "    antos vm exec <VM_ID> <comando>          Ejecuta un comando en la microVM"
            );
            println!("    antos vm list                             Lista microVMs en ejecución");
            println!(
                "    antos vm kill <VM_ID>                     Detiene y libera una microVM\n"
            );
        }
    }
    Ok(())
}

// ----------------------------------------------------------- antpkg (T16.2)

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn args_of(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -- antos vm spawn -----------------------------------------------------

    #[test]
    fn test_parse_vm_spawn_args_reads_every_flag() {
        let args = args_of(&[
            "spawn",
            "--id",
            "vm-test",
            "--cpus",
            "4",
            "--memory",
            "1024",
            "--kernel",
            "/boot/other-vmlinuz",
            "--cmd",
            "echo hi",
        ]);
        let parsed = parse_vm_spawn_args(&args);
        assert_eq!(
            parsed,
            VmSpawnArgs {
                vm_id: Some("vm-test".into()),
                vcpu_count: 4,
                memory_mb: 1024,
                kernel_image: "/boot/other-vmlinuz".into(),
                command: Some("echo hi".into()),
            }
        );
    }

    #[test]
    fn test_parse_vm_spawn_args_keeps_defaults_on_an_unparseable_cpu_count() {
        // «--cpus abc» no es un u8 válido: se conserva el valor por defecto
        // en vez de entrar en pánico o de propagar un error (T31.13).
        let args = args_of(&["spawn", "--cpus", "abc"]);
        let parsed = parse_vm_spawn_args(&args);
        assert_eq!(parsed.vcpu_count, 2);
        assert_eq!(parsed.vm_id, None);
    }

    #[test]
    fn test_parse_vm_spawn_args_ignores_a_trailing_flag_with_no_value() {
        let args = args_of(&["spawn", "--memory"]);
        let parsed = parse_vm_spawn_args(&args);
        assert_eq!(parsed.memory_mb, 512);
    }
}
