#![allow(unused_imports, dead_code)]

extern crate antos_protocol;

use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use crate::capability::{Catalog, Tier};
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{Outcome, Record};
use crate::planner::{
    claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner,
    openai_compat::OpenAiCompatPlanner, Planner,
};
use crate::terminal::{ellipsis, paint, tier_color, BLUE, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use crate::cli::args::Opts;

pub fn cmd_caps(catalog: &Catalog, ctx: &Ctx) -> Result<()> {
    let grants = Grants::load(&ctx.grants_path())?;
    println!();
    println!("{}", paint("capacidades", BOLD));
    for cap in catalog.caps.values() {
        let mut nivel = cap.policy.tier.label().to_string();
        if cap.policy.tier == Tier::Grant {
            nivel.push_str(if grants.is_granted(&cap.name) {
                " · concedida"
            } else {
                " · denegada"
            });
        }
        println!();
        println!(
            "  {}  {}",
            paint(&cap.name, BOLD),
            paint(&nivel, tier_color(cap.policy.tier))
        );
        println!("    {}", paint(&cap.summary, DIM));
        let params = cap
            .params
            .iter()
            .map(|(n, s)| {
                if s.optional || s.default.is_some() {
                    format!("[{n}]")
                } else {
                    n.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        if !params.is_empty() {
            println!("    {}", paint(&format!("parámetros: {params}"), DIM));
        }
    }
    println!();
    Ok(())
}

/// Comprueba que el recinto es real, atacándolo.
///
/// No basta con generar una política y confiar: `doctor` intenta de verdad
/// escribir fuera de lo declarado y salir a la red, y solo da por buena la
/// garantía si el kernel lo impide.

pub fn cmd_doctor(ctx: &Ctx) -> Result<()> {
    let jail = crate::sandbox::for_host();
    println!();
    println!("{}", paint("recinto de ejecución", BOLD));
    println!("  motor     {}", jail.name());
    println!("  garantiza {}", paint(jail.guarantees(), DIM));
    println!();

    let mut fallos = 0;

    // Política de prueba: solo se declara el espacio de trabajo, sin red.
    let solo_workspace = crate::sandbox::Policy {
        writes: vec![ctx.workspace.clone()],
        reads: vec![],
        dirs: vec![ctx.workspace.clone()],
        network: false,
        allowed_secrets: vec![],
        quota: None,
    };

    // 1) escribir fuera de lo declarado
    let fuga = ctx.state.join("doctor-fuga.txt");
    let _ = std::fs::remove_file(&fuga);
    let intento = crate::sandbox::run(
        &*jail,
        &[crate::exec::Change::Write {
            path: fuga.clone(),
            content: "esto no debería existir".into(),
        }],
        &solo_workspace,
    );
    let quedo_escrito = fuga.exists();
    let _ = std::fs::remove_file(&fuga);

    if intento.is_err() && !quedo_escrito {
        marca(
            true,
            "escritura fuera de lo declarado: la deniega el kernel",
        );
    } else {
        fallos += 1;
        marca(false, "escritura fuera de lo declarado: SE COMPLETÓ");
    }

    // 2) escribir dentro de lo declarado debe seguir funcionando: un recinto
    //    que lo bloquea todo no es seguro, es inútil.
    let dentro = ctx.workspace.join(".doctor-prueba");
    let _ = std::fs::remove_file(&dentro);
    let permitido = crate::sandbox::run(
        &*jail,
        &[crate::exec::Change::Write {
            path: dentro.clone(),
            content: "ok".into(),
        }],
        &solo_workspace,
    );
    let creado = dentro.exists();
    let _ = std::fs::remove_file(&dentro);
    if permitido.is_ok() && creado {
        marca(true, "escritura dentro de lo declarado: permitida");
    } else {
        fallos += 1;
        marca(
            false,
            "escritura dentro de lo declarado: BLOQUEADA (el recinto es demasiado estrecho)",
        );
    }

    // 3) leer fuera de lo declarado. Es la garantía que separa a Landlock de
    //    Seatbelt, así que se pregunta al motor qué promete antes de juzgar.
    let secreto = ctx.state.join("doctor-secreto.txt");
    std::fs::write(&secreto, "credencial de mentira")?;
    let lectura = crate::sandbox::run(
        &*jail,
        &[crate::exec::Change::Read {
            path: secreto.clone(),
        }],
        &solo_workspace,
    );
    let _ = std::fs::remove_file(&secreto);

    match (jail.confines_reads(), lectura.is_err()) {
        (true, true) => marca(true, "lectura fuera de lo declarado: la deniega el kernel"),
        (true, false) => {
            fallos += 1;
            marca(false, "lectura fuera de lo declarado: SE COMPLETÓ");
        }
        (false, _) => println!(
            "  {} {}",
            paint("·", YELLOW),
            paint(
                "lectura fuera de lo declarado: este motor no confina lecturas",
                DIM
            )
        ),
    }

    // 4) red. Se prueba en los dos sentidos para no confundir «bloqueada»
    //    con «esta máquina no tiene internet».
    let con_red = crate::sandbox::Policy {
        writes: vec![],
        reads: vec![],
        dirs: vec![],
        network: true,
        allowed_secrets: vec![],
        quota: None,
    };
    let alcanzable_declarando = crate::sandbox::probe_network(&*jail, &con_red).unwrap_or(false);
    let alcanzable_sin_declarar = crate::sandbox::probe_network(&*jail, &solo_workspace).unwrap_or(false);

    match (alcanzable_declarando, alcanzable_sin_declarar) {
        (true, false) => marca(true, "red: alcanzable al declararla, bloqueada si no"),
        (true, true) => {
            fallos += 1;
            marca(false, "red: alcanzable SIN declararla");
        }
        (false, _) => println!(
            "  {} {}",
            paint("?", YELLOW),
            paint(
                "red: no concluyente — esta máquina no llega a internet",
                DIM
            )
        ),
    }

    println!();
    if fallos == 0 {
        println!("{}", paint("✓ el recinto se comporta como dice", GREEN));
        Ok(())
    } else {
        bail!("{fallos} comprobación(es) del recinto han fallado")
    }
}

fn marca(ok: bool, texto: &str) {
    let (simbolo, color) = if ok { ("✓", GREEN) } else { ("✗", RED) };
    println!("  {} {texto}", paint(simbolo, color));
}

pub fn cmd_log(ctx: &Ctx) -> Result<()> {
    let records = crate::journal::read_all(&ctx.journal_path())?;
    if records.is_empty() {
        println!("la bitácora está vacía");
        return Ok(());
    }
    println!();
    println!("{}", paint("bitácora", BOLD));
    for r in records
        .iter()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let mark = match r.outcome {
            Outcome::Executed if r.reverted => paint("↩", DIM),
            Outcome::Executed => paint("✓", GREEN),
            Outcome::Reverted => paint("↩", DIM),
            Outcome::Denied => paint("✗", RED),
            Outcome::Failed => paint("!", RED),
            Outcome::Cancelled => paint("·", DIM),
        };
        let ticket_badge = if let Some(tid) = &r.ticket_id {
            format!("{} ", paint(&format!("[{tid}]"), BOLD))
        } else {
            String::new()
        };
        println!(
            "  {mark} {}  {} {}{}",
            paint(&r.id, DIM),
            paint(&format!("[{}]", r.tier.label()), tier_color(r.tier)),
            ticket_badge,
            ellipsis(&r.intent, 60)
        );
        if let Some(d) = &r.detail {
            println!("      {}", paint(d, DIM));
        }
    }
    println!();
    Ok(())
}

pub fn cmd_grant(ctx: &Ctx, catalog: &Catalog, args: &[String]) -> Result<()> {
    let Some(cap_name) = args.first() else {
        bail!("uso: antos grant <capacidad|secreto> [--minutos N] [--para \"motivo\"]");
    };

    // Si no está en el catálogo directamente, comprobar si es un permiso de secreto o ruta
    if let Ok(cap) = catalog.get(cap_name) {
        if cap.policy.tier != Tier::Grant {
            bail!(
                "{cap_name} es de nivel «{}»: no necesita concesión",
                cap.policy.tier.label()
            );
        }
    }

    let minutes = args
        .iter()
        .position(|a| a == "--minutos" || a == "-m")
        .and_then(|i| args.get(i + 1))
        .and_then(|m| m.parse::<i64>().ok())
        .unwrap_or(10);

    let reason = args
        .iter()
        .position(|a| a == "--para" || a == "--reason" || a == "-p")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let mut grants = Grants::load(&ctx.grants_path())?;
    grants.grant_with_reason(cap_name, minutes, reason.clone());
    grants.save(&ctx.grants_path())?;

    let motivo_str = reason.map(|r| format!(" para «{r}»")).unwrap_or_default();
    println!(
        "\n{} Concesión explícita otorgada a «{}» durante {} minutos{motivo_str}.\n",
        paint("🔑 CONCEDIDA", GREEN),
        paint(cap_name, BOLD),
        paint(&minutes.to_string(), YELLOW)
    );
    Ok(())
}

pub fn cmd_revoke(ctx: &Ctx, args: &[String]) -> Result<()> {
    let Some(cap_name) = args.first() else {
        bail!("uso: antos revoke <capacidad|secreto>");
    };
    let mut grants = Grants::load(&ctx.grants_path())?;
    grants.revoke(cap_name);
    grants.save(&ctx.grants_path())?;
    println!(
        "\n{} Concesión revocada: «{}».\n",
        paint("🔒 REVOCADA", DIM),
        paint(cap_name, BOLD)
    );
    Ok(())
}

pub fn cmd_boot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let engine = crate::boot::BootEngine::global();

    match sub {
        "build" | "compile" => {
            println!(
                "\n{} Compilando kernel no_std y empaquetando imagen BIOS/UEFI...",
                paint("antOS Boot ·", BOLD)
            );
            let img = engine.build(&ctx.workspace)?;
            println!(
                "  {} {}\n",
                paint("✓ Imagen generada:", GREEN),
                img.display()
            );
        }
        "test" | "check" => {
            println!(
                "\n{} Ejecutando prueba automatizada de arranque en QEMU (headless)...",
                paint("antOS Boot ·", BOLD)
            );
            let res = engine.test_boot(&ctx.workspace)?;
            println!("{}\n", res);
        }
        "qemu" | "run" => {
            println!("\n{} Iniciando QEMU...", paint("antOS Boot ·", BOLD));
            let img = engine.build(&ctx.workspace)?;
            let run_script = ctx.workspace.join("run.sh");
            if run_script.exists() {
                let status = std::process::Command::new("bash")
                    .arg(&run_script)
                    .status()?;
                if !status.success() {
                    bail!("QEMU finalizó con código {:?}", status.code());
                }
            } else {
                let status = std::process::Command::new("qemu-system-x86_64")
                    .args(["-m", "256M", "-serial", "stdio", "-drive"])
                    .arg(format!("format=raw,file={}", img.display()))
                    .status()?;
                if !status.success() {
                    bail!("QEMU finalizó con código {:?}", status.code());
                }
            }
        }
        "iso" => {
            let mut arch = "x86_64";
            let mut iter = args.iter().skip(1);
            while let Some(a) = iter.next() {
                if a == "--arch" {
                    if let Some(val) = iter.next() {
                        arch = val.as_str();
                    }
                } else if a == "aarch64" || a == "arm64" {
                    arch = "aarch64";
                }
            }
            println!(
                "\n{} Construyendo imagen Live ISO autoarrancable ({arch})...",
                paint("antOS Boot ·", BOLD)
            );
            let iso = engine.build_iso_arch(&ctx.workspace, arch)?;
            println!(
                "  {} {}\n",
                paint("✓ Live ISO generada:", GREEN),
                iso.display()
            );
        }
        "release" | "dist" => {
            println!(
                "\n{} Ejecutando pipeline oficial de empaquetado release...",
                paint("antOS Release ·", BOLD)
            );
            let res = engine.build_release(&ctx.workspace)?;
            println!("{}\n", res);
        }
        "status" | _ => {
            let st = engine.status(&ctx.workspace);
            println!(
                "\n{} Estado del Pipeline de Arranque Bare Metal:",
                paint("antOS Boot ·", BOLD)
            );
            println!("  • Arquitectura:         {}", paint(&st.target_arch, CYAN));
            println!(
                "  • Binario Kernel ELF:   {} ({})",
                if st.kernel_elf_exists {
                    paint("Presente", GREEN)
                } else {
                    paint("No compilado", YELLOW)
                },
                if st.kernel_elf_exists {
                    format!("{} KiB", st.kernel_elf_size_bytes / 1024)
                } else {
                    "0 B".into()
                }
            );
            println!(
                "  • Imagen BIOS/MBR:      {} ({})",
                if st.bios_image_exists {
                    paint("Presente", GREEN)
                } else {
                    paint("No generada", YELLOW)
                },
                if st.bios_image_exists {
                    format!(
                        "{:.1} MB",
                        st.bios_image_size_bytes as f64 / (1024.0 * 1024.0)
                    )
                } else {
                    "0 B".into()
                }
            );
            println!(
                "  • Emulador QEMU:        {}",
                if st.qemu_installed {
                    paint("Disponible (qemu-system-x86_64)", GREEN)
                } else {
                    paint("No instalado", RED)
                }
            );
            println!("\n  Uso:");
            println!(
                "    antos boot build      Compila el kernel no_std y crea la imagen de disco"
            );
            println!("    antos boot test       Prueba automatizada de arranque en QEMU headless");
            println!("    antos boot qemu       Lanza la máquina virtual interactiva en QEMU");
            println!("    antos boot iso        Genera la imagen Live ISO autoarrancable");
            println!("    antos boot release    Ejecuta el pipeline de empaquetado y checksums\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------- plugins

pub fn cmd_plugin(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let plugins_dir = crate::wasm::PluginManager::get_plugins_dir(&ctx.workspace);

    match sub {
        "list" | "ls" => {
            let list = crate::wasm::PluginManager::list_plugins(&plugins_dir);
            println!(
                "\n{} Plugins WebAssembly (WASM) Registrados:",
                paint("antOS ·", BOLD)
            );
            if list.is_empty() {
                println!(
                    "  (no hay plugins instalados en {})\n",
                    plugins_dir.display()
                );
            } else {
                for p in list {
                    let kb = (p.wasm_size_bytes + 1023) / 1024;
                    println!(
                        "  • {} v{} ({} KiB) - {}",
                        paint(&p.name, GREEN),
                        paint(&p.version, CYAN),
                        kb,
                        p.description
                    );
                    println!("    Acciones disponibles: {}", p.capabilities.join(", "));
                }
                println!();
            }
        }
        "install" => {
            let path_str = args.get(1).map(String::as_str).unwrap_or(".");
            let src = ctx.workspace.join(path_str);
            println!(
                "\n{} Instalando plugin desde {}...",
                paint("antOS ·", BOLD),
                src.display()
            );
            let summary = crate::wasm::PluginManager::install_plugin(&plugins_dir, &src)?;
            println!(
                "  {} {} v{} (acciones: {})\n",
                paint("✓ Plugin instalado:", GREEN),
                summary.name,
                summary.version,
                summary.capabilities.join(", ")
            );
        }
        "run" => {
            let name = args.get(1).map(String::as_str).unwrap_or("");
            if name.is_empty() {
                bail!("Uso: antos plugin run <nombre> [accion] [clave=valor...]");
            }
            let action = args.get(2).map(String::as_str).unwrap_or("run");
            let mut params = std::collections::BTreeMap::new();
            for arg in args.iter().skip(3) {
                if let Some((k, v)) = arg.split_once('=') {
                    params.insert(k.to_string(), v.to_string());
                }
            }

            println!(
                "\n{} Ejecutando plugin [{}:{}] en sandbox aislado WASM...",
                paint("antOS ·", BOLD),
                name,
                action
            );
            let res = crate::wasm::PluginManager::run_plugin(&plugins_dir, name, action, &params);
            if res.success {
                println!("  {} {}", paint("✓ Resultado:", GREEN), res.output);
                println!("    Ciclos de instrucción:  {}", res.fuel_consumed);
                println!(
                    "    Memoria lineal:         {} KiB (cuota máx: 64 MB)\n",
                    res.memory_allocated_bytes / 1024
                );
            } else {
                let err = res.error.unwrap_or_else(|| "Error desconocido".into());
                println!("  {} {}\n", paint("✗ Error:", RED), err);
                bail!("Fallo durante ejecución en sandbox WASM");
            }
        }
        _ => {
            println!(
                "\n{} Gestor de Plugins WebAssembly (WASM):",
                paint("antOS Plugins ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos plugin list                   Enumera plugins instalados");
            println!("    antos plugin install <directorio>   Instala un plugin con plugin.toml");
            println!(
                "    antos plugin run <nombre> [accion]  Ejecuta una acción en sandbox WASM\n"
            );
        }
    }
    Ok(())
}

// -------------------------------------------------------- screenshot & visual qa

pub fn cmd_disk(_ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");

    match sub {
        "list" | "ls" => {
            println!(
                "\n{} Unidades de Almacenamiento Detectadas:",
                paint("antOS Almacenamiento ·", BOLD)
            );
            let disks = crate::installer::DiskManager::list_disks()?;
            if disks.is_empty() {
                println!("  (no se detectaron unidades de bloque en el sistema)\n");
                return Ok(());
            }

            for d in &disks {
                let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                println!(
                    "  • {} ({:.1} GB, Bus: {}, Tabla: {})",
                    paint(&d.path, CYAN),
                    gb,
                    d.bus_type,
                    d.partition_table
                );
                println!("    Modelo:      {}", d.model);
                println!(
                    "    Sectores:    {} bytes / sector (RO: {})",
                    d.sector_size, d.is_read_only
                );
                if d.partitions.is_empty() {
                    println!("    Particiones: (disco sin particiones)");
                } else {
                    println!("    Particiones: {} detectadas", d.partitions.len());
                    for p in &d.partitions {
                        let p_mb = p.size_bytes / (1024 * 1024);
                        let efi_badge = if p.is_efi {
                            paint(" [EFI ESP]", GREEN)
                        } else {
                            "".into()
                        };
                        let fs = p.fs_type.as_deref().unwrap_or("desconocido");
                        let mount = p.mountpoint.as_deref().unwrap_or("no montada");
                        println!(
                            "      - {} ({:.0} MB, {}) → {}{}",
                            paint(&p.name, BOLD),
                            p_mb,
                            fs,
                            mount,
                            efi_badge
                        );
                    }
                }
                println!();
            }
        }
        "inspect" | "info" => {
            let target = args.get(1).map(String::as_str).unwrap_or("");
            if target.is_empty() {
                bail!("Uso: antos disk inspect <dispositivo>");
            }

            println!(
                "\n{} Inspeccionando Dispositivo {}:",
                paint("antOS Almacenamiento ·", BOLD),
                paint(target, CYAN)
            );
            let disk = crate::installer::DiskManager::inspect_disk(target)?;
            match disk {
                Some(d) => {
                    let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    println!("  • Ruta física:       {}", paint(&d.path, BOLD));
                    println!("  • Modelo / Vendor:   {}", d.model);
                    println!(
                        "  • Tamaño total:      {:.2} GB ({} bytes)",
                        gb, d.size_bytes
                    );
                    println!("  • Tamaño de sector:  {} bytes (LBA)", d.sector_size);
                    println!("  • Tipo de Bus:       {}", d.bus_type);
                    println!("  • Tabla:             {}", d.partition_table);
                    println!("  • Solo Lectura:      {}\n", d.is_read_only);

                    println!("  {}:", paint("Mapa de Particiones", BOLD));
                    if d.partitions.is_empty() {
                        println!("    (sin particiones registradas)");
                    } else {
                        for p in &d.partitions {
                            let efi_str = if p.is_efi {
                                paint(" [SISTEMA EFI]", GREEN)
                            } else {
                                "".into()
                            };
                            println!(
                                "    #{}: {} | {:.1} MB | FS: {} | UUID: {}{}",
                                p.number,
                                paint(&p.name, CYAN),
                                p.size_bytes as f64 / (1024.0 * 1024.0),
                                p.fs_type.as_deref().unwrap_or("none"),
                                p.uuid.as_deref().unwrap_or("N/A"),
                                efi_str
                            );
                        }
                    }
                    println!();
                }
                None => {
                    bail!("No se encontró el dispositivo «{}»", target);
                }
            }
        }
        "partition" | "part" => {
            let target = args.get(1).map(String::as_str).unwrap_or("");
            if target.is_empty() {
                bail!("Uso: antos disk partition <dispositivo> [--clean | --dual-boot] [--apply]");
            }

            let clean = args.iter().any(|a| a == "--clean");
            let apply = args.iter().any(|a| a == "--apply");
            let dry_run = !apply;

            println!(
                "\n{} Calculando esquema de particionado para {}:",
                paint("antOS Particionador ·", BOLD),
                paint(target, CYAN)
            );

            let plan = crate::installer::DiskManager::plan_partitioning(target, clean)?;
            let report = crate::installer::DiskManager::apply_partitioning(target, &plan, dry_run)?;
            println!("{}\n", report);
            if dry_run {
                println!("  {} Para aplicar estos cambios en el disco use «--apply» (acción destructiva).\n", paint("Nota:", YELLOW));
            }
        }
        _ => {
            println!(
                "\n{} Gestor de Discos y Particiones GPT:",
                paint("antOS Almacenamiento ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos disk list                           Enumera discos físicos y particiones");
            println!("    antos disk inspect <dispositivo>          Muestra el mapa de particiones y metadatos");
            println!("    antos disk partition <dispositivo> [modo] Calcula o aplica tabla GPT (--clean / --dual-boot)\n");
        }
    }
    Ok(())
}

// ---------------------------------------------------- installer & deploy (T15.2)

pub fn cmd_install(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");

    match sub {
        "list" | "disks" => {
            println!(
                "\n{} Discos Compatibles para Instalación de antOS:",
                paint("antOS Instalador ·", BOLD)
            );
            let disks = crate::installer::DiskManager::list_disks()?;
            if disks.is_empty() {
                println!("  (no se detectaron unidades de almacenamiento compatibles)\n");
                return Ok(());
            }

            for (idx, d) in disks.iter().enumerate() {
                let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let suitable = d.size_bytes >= 8 * 1024 * 1024 * 1024;
                let status_str = if suitable {
                    paint("COMPATIBLE (≥ 8 GB)", GREEN)
                } else {
                    paint("INSUFICIENTE (< 8 GB)", RED)
                };

                let has_efi = d.partitions.iter().any(|p| p.is_efi);
                let mode_rec = if has_efi {
                    paint("Recomendado: Dual Boot", CYAN)
                } else {
                    paint("Recomendado: Sistema Principal Limpio", YELLOW)
                };

                println!(
                    "  [{}] {} ({:.1} GB, Bus: {})",
                    idx + 1,
                    paint(&d.path, BOLD),
                    gb,
                    d.bus_type
                );
                println!("      Modelo:       {}", d.model);
                println!("      Estado:       {} | {}", status_str, mode_rec);
                println!("      Particiones:  {} existentes\n", d.partitions.len());
            }
        }
        "run" | "deploy" => {
            let mut target_device = "/dev/nvme0n1".to_string();
            let mut clean_install = false;
            let mut username = "antos".to_string();
            let mut hostname = "antos-box".to_string();
            let mut dry_run = true;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--target" | "-t" => {
                        if i + 1 < args.len() {
                            target_device = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--clean" => clean_install = true,
                    "--dual-boot" => clean_install = false,
                    "--user" | "-u" => {
                        if i + 1 < args.len() {
                            username = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--host" => {
                        if i + 1 < args.len() {
                            hostname = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--apply" => dry_run = false,
                    _ => {}
                }
                i += 1;
            }

            let config = antos_protocol::InstallConfig {
                target_device: target_device.clone(),
                clean_install,
                target_mount: "/mnt/antos".into(),
                hostname,
                username,
                timezone: "UTC".into(),
                dry_run,
            };

            let mode_str = if clean_install {
                "Sistema Principal (Limpio)"
            } else {
                "Sistema Secundario (Dual Boot)"
            };
            println!(
                "\n{} Iniciando Despliegue de antOS:",
                paint("antOS Instalador ·", BOLD)
            );
            println!("  • Dispositivo:      {}", paint(&target_device, CYAN));
            println!("  • Modo de instalación: {}", paint(mode_str, BOLD));
            println!(
                "  • Modo de ejecución:   {}\n",
                if dry_run {
                    paint("SIMULACIÓN SEGURA (Dry-Run)", YELLOW)
                } else {
                    paint("INSTALACIÓN EN DISCO REAL", RED)
                }
            );

            let report = crate::installer::DeployEngine::deploy_system(&config, &ctx.workspace)?;
            println!(
                "  {}: {}",
                paint("Resultado", BOLD),
                if report.success {
                    paint("EXITOSO", GREEN)
                } else {
                    paint("FALLIDO", RED)
                }
            );
            println!("  {}\n", report.summary);
            println!("  {}:", paint("Pasos Ejecutados", BOLD));
            for s in &report.steps {
                println!("    ✓ {}: {}", paint(&s.name, CYAN), s.description);
            }
            println!("\n  {}:", paint("Entradas /etc/fstab Generadas", BOLD));
            for f in &report.fstab_entries {
                println!("    {}", f);
            }
            println!();

            if dry_run {
                println!("  {} Para aplicar esta instalación de forma definitiva en el hardware ejecute con «--apply».\n", paint("Nota:", YELLOW));
            }
        }
        "wizard" | "gui" => {
            println!(
                "\n{}",
                paint(
                    "╔════════════════════════════════════════════════════════════════╗",
                    CYAN
                )
            );
            println!(
                "{}",
                paint(
                    "║           antOS · Asistente de Instalación Guiada             ║",
                    BOLD
                )
            );
            println!(
                "{}\n",
                paint(
                    "╚════════════════════════════════════════════════════════════════╝",
                    CYAN
                )
            );

            let disks = crate::installer::DiskManager::list_disks()?;
            if disks.is_empty() {
                bail!("No se detectaron discos de almacenamiento disponibles para instalar");
            }

            let chosen_disk = &disks[0];
            let has_efi = chosen_disk.partitions.iter().any(|p| p.is_efi);
            let mode_str = if has_efi {
                "Dual Boot (preservando partición EFI y SO vecino)"
            } else {
                "Sistema Principal Completo"
            };

            println!(
                "  Disco detectado para instalación: {} ({:.1} GB)",
                paint(&chosen_disk.path, CYAN),
                chosen_disk.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            );
            println!(
                "  Modo seleccionado automáticamente: {}",
                paint(mode_str, GREEN)
            );
            println!(
                "  Usuario predeterminado:           {}",
                paint("antos", BOLD)
            );
            println!(
                "  Hostname:                         {}",
                paint("antos-box", BOLD)
            );
            println!("\n  Ejecutando simulación de instalación guiada...");

            let cfg = antos_protocol::InstallConfig {
                target_device: chosen_disk.path.clone(),
                clean_install: !has_efi,
                target_mount: "/mnt/antos".into(),
                hostname: "antos-box".into(),
                username: "antos".into(),
                timezone: "UTC".into(),
                dry_run: true,
            };

            let report = crate::installer::DeployEngine::deploy_system(&cfg, &ctx.workspace)?;
            println!(
                "\n  {} {}",
                paint("✓ Verificación de instalación completada:", GREEN),
                report.summary
            );
            println!("    • Partición ESP:   {}", report.efi_partition);
            println!("    • Partición Raíz:  {}", report.root_partition);
            println!(
                "    • Pasos validados: {}/{}",
                report.steps.len(),
                report.steps.len()
            );
            println!("\n  Para proceder a instalar en vivo sobre este equipo, ejecute:");
            println!(
                "    {}\n",
                paint(
                    &format!("antos install run --target {} --apply", chosen_disk.path),
                    CYAN
                )
            );
        }
        _ => {
            println!(
                "\n{} Asistente de Instalación en Disco Duro y Dual Boot:",
                paint("antOS Instalador ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos install list                           Enumera discos compatibles y sugerencias de modo");
            println!("    antos install wizard                         Asistente interactivo guiado de instalación");
            println!("    antos install run --target <dev> [--clean]   Ejecuta el despliegue del sistema base");
            println!("    antos install run --target <dev> --apply     Aplica los cambios irreversibles al disco\n");
        }
    }
    Ok(())
}

// ---------------------------------------------------- bootloader & uefi (T15.3)

pub fn cmd_bootloader(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");

    match sub {
        "probe" | "detect" | "os" => {
            let mut esp_path = "/boot/efi".to_string();
            let mut i = 1;
            while i < args.len() {
                if args[i] == "--esp" && i + 1 < args.len() {
                    esp_path = args[i + 1].clone();
                    i += 1;
                }
                i += 1;
            }

            println!(
                "\n{} Sondeando sistemas operativos en «{}»:",
                paint("antOS Bootloader ·", BOLD),
                esp_path
            );
            let entries = crate::installer::BootloaderEngine::probe_operating_systems(
                std::path::Path::new(&esp_path),
            )?;
            if entries.is_empty() {
                println!(
                    "  (no se detectaron sistemas operativos en el directorio especificado)\n"
                );
                return Ok(());
            }

            for (idx, os) in entries.iter().enumerate() {
                let badge = match os.os_type.as_str() {
                    "windows" => paint("[WINDOWS]", CYAN),
                    "linux" => paint("[LINUX]", GREEN),
                    "macos" => paint("[MACOS]", YELLOW),
                    _ => paint("[ANTOS]", BOLD),
                };
                println!("  [{}] {} {}", idx + 1, badge, paint(&os.name, BOLD));
                println!("      Ruta binario EFI:  {}", os.efi_path);
                println!(
                    "      Dispositivo/Part:  {} (Partición #{})\n",
                    os.disk_device, os.partition_number
                );
            }
        }
        "install" | "deploy" => {
            let mut esp_path = "/boot/efi".to_string();
            let mut target_device = "/dev/nvme0n1".to_string();
            let mut efi_partition = 1u32;
            let mut timeout_seconds = 5u32;
            let mut dry_run = true;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--esp" => {
                        if i + 1 < args.len() {
                            esp_path = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--target" | "-t" => {
                        if i + 1 < args.len() {
                            target_device = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--partition" | "-p" => {
                        if i + 1 < args.len() {
                            if let Ok(num) = args[i + 1].parse::<u32>() {
                                efi_partition = num;
                            }
                            i += 1;
                        }
                    }
                    "--timeout" => {
                        if i + 1 < args.len() {
                            if let Ok(num) = args[i + 1].parse::<u32>() {
                                timeout_seconds = num;
                            }
                            i += 1;
                        }
                    }
                    "--apply" => dry_run = false,
                    _ => {}
                }
                i += 1;
            }

            let esp = std::path::PathBuf::from(if dry_run {
                ctx.workspace
                    .join("target/esp-staging")
                    .display()
                    .to_string()
            } else {
                esp_path.clone()
            });

            let config = antos_protocol::BootloaderConfig {
                esp_mount: esp.display().to_string(),
                target_device: target_device.clone(),
                efi_partition,
                default_os: "antos".into(),
                timeout_seconds,
                detected_os: Vec::new(),
                dry_run,
            };

            println!(
                "\n{} Instalando Gestor de Arranque UEFI (systemd-boot):",
                paint("antOS Bootloader ·", BOLD)
            );
            println!("  • Directorio ESP:     {}", paint(&config.esp_mount, CYAN));
            println!("  • Dispositivo destino: {}", paint(&target_device, CYAN));
            println!("  • Partición EFI:      #{}", efi_partition);
            println!("  • Timeout menú:       {} segundos", timeout_seconds);
            println!(
                "  • Modo de ejecución:  {}\n",
                if dry_run {
                    paint("SIMULACIÓN SEGURA (Dry-Run)", YELLOW)
                } else {
                    paint("ESCRITURA EN ESP Y NVRAM", RED)
                }
            );

            let report = crate::installer::BootloaderEngine::install_bootloader(&config)?;
            println!(
                "  {}: {}",
                paint("Resultado", BOLD),
                if report.success {
                    paint("EXITOSO", GREEN)
                } else {
                    paint("FALLIDO", RED)
                }
            );
            println!("  {}\n", report.summary);
            println!("  {}:", paint("Entradas de Arranque Generadas", BOLD));
            for e in &report.entries_configured {
                println!("    ✓ {}", e);
            }
            println!("\n  {}:", paint("Comando de Registro NVRAM", BOLD));
            println!("    {}\n", paint(&report.efibootmgr_command, CYAN));

            if dry_run {
                println!(
                    "  {} Para aplicar estos cambios en el firmware UEFI use «--apply».\n",
                    paint("Nota:", YELLOW)
                );
            }
        }
        _ => {
            println!(
                "\n{} Gestor de Arranque UEFI y Dual Boot:",
                paint("antOS Bootloader ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos bootloader probe [--esp <ruta>]        Sondea sistemas operativos instalados");
            println!("    antos bootloader install [--esp <ruta>]      Genera y valida la configuración de systemd-boot");
            println!("    antos bootloader install --apply             Registra antOS en la NVRAM UEFI con efibootmgr\n");
        }
    }
    Ok(())
}

// ----------------------------------------------------------------- microvms

pub fn cmd_vm(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "spawn" | "start" | "run" => {
            let mut vm_id = format!("vm-{}", chrono::Local::now().format("%Y%m%d%H%M%S"));
            let mut vcpu_count = 2u8;
            let mut memory_mb = 512u32;
            let mut kernel_image = "/boot/antos-vmlinuz".to_string();
            let mut command = None;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--id" if i + 1 < args.len() => {
                        vm_id = args[i + 1].clone();
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

            println!("\n{} Instanciando microVM con aislamiento por hipervisor...", paint("antOS MicroVM ·", BOLD));
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
            println!("  {} MicroVM «{}» arrancada exitosamente", paint("✓", GREEN), paint(&instance.id, BOLD));
            println!("    • PID:          {}", instance.pid);
            println!("    • vCPUs:        {}", instance.vcpus);
            println!("    • Memoria:      {} MB", instance.memory_mb);
            println!("    • Puerto vsock: {}", instance.vsock_port);
            println!("    • Estado:       {}\n", paint(&instance.status, GREEN));
        }
        "exec" => {
            let vm_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos vm exec <VM_ID> <comando>"))?;
            let command = if args.len() > 2 {
                args[2..].join(" ")
            } else {
                bail!("Uso: antos vm exec <VM_ID> <comando>");
            };

            println!("\n{} Ejecutando comando en microVM «{}»...", paint("antOS MicroVM ·", BOLD), paint(vm_id, CYAN));
            let res = crate::vm::MicrovmManager::exec_vm(&ctx.state, vm_id, &command)?;
            let status_badge = if res.success { paint("EXITOSO", GREEN) } else { paint("FALLIDO", RED) };
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
            println!("\n{} MicroVMs Activas en el Sistema:", paint("antOS MicroVM ·", BOLD));
            let vms = crate::vm::MicrovmManager::list_vms(&ctx.state)?;
            if vms.is_empty() {
                println!("  (no hay microVMs activas en este momento)\n");
            } else {
                for v in &vms {
                    println!("  • [{}] {} (PID {}, {} vCPUs, {} MB RAM, vsock {})",
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
            let vm_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos vm kill <VM_ID>"))?;
            crate::vm::MicrovmManager::kill_vm(&ctx.state, vm_id)?;
            println!("\n{} MicroVM «{}» detenida y eliminada.\n", paint("✓", GREEN), paint(vm_id, BOLD));
        }
        "status" | _ => {
            println!("\n{} Diagnóstico de Hipervisor y MicroVMs:", paint("antOS MicroVM ·", BOLD));
            let st = crate::vm::MicrovmManager::get_status(&ctx.state)?;
            let kvm_badge = if st.kvm_available { paint("Disponible (/dev/kvm)", GREEN) } else { paint("No detectado (Emulación)", YELLOW) };
            println!("  • Soporte KVM:             {}", kvm_badge);
            println!("  • Motor de Hipervisor:     {}", paint(&st.hypervisor_engine, CYAN));
            println!("  • Kernel del Host:         {}", st.kernel_version);
            println!("  • MicroVMs activas:        {}", st.active_vms_count);
            println!("  • Memoria asignada a VMs:  {} MB", st.total_memory_allocated_mb);
            println!("  • Canales vsock:           {}", if st.vsock_supported { paint("Soportado", GREEN) } else { paint("No disponible", RED) });
            println!("\n  Uso:");
            println!("    antos vm spawn [--cpus N] [--memory MB]   Arranca una microVM efímera");
            println!("    antos vm exec <VM_ID> <comando>          Ejecuta un comando en la microVM");
            println!("    antos vm list                             Lista microVMs en ejecución");
            println!("    antos vm kill <VM_ID>                     Detiene y libera una microVM\n");
        }
    }
    Ok(())
}

// ----------------------------------------------------------- antpkg (T16.2)

pub fn cmd_autopilot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "start" => {
            let mut interval = 5u64;
            let mut auto_merge = false;
            let mut i = 1;
            while i < args.len() {
                if (args[i] == "--interval" || args[i] == "-i") && i + 1 < args.len() {
                    if let Ok(v) = args[i + 1].parse::<u64>() {
                        interval = v;
                    }
                    i += 2;
                } else if args[i] == "--auto-merge" || args[i] == "-m" {
                    auto_merge = true;
                    i += 1;
                } else {
                    i += 1;
                }
            }

            let config = antos_protocol::AutopilotConfig {
                enabled: true,
                poll_interval_secs: interval,
                watch_paths: Vec::new(),
                auto_merge,
                target_branch: "master".to_string(),
            };

            println!("\n{} Iniciando centinela continuo en segundo plano...", paint("antOS Autopilot ·", BOLD));
            let st = crate::autopilot::AutopilotEngine::start(&ctx.state, &ctx.workspace, config)?;
            println!("  Estado:                {}", paint("ACTIVO (Vigilando)", GREEN));
            println!("  Intervalo de sondeo:   {}s", st.poll_interval_secs);
            println!("  Espacio de trabajo:    {}", st.workspace_path);
            println!("  Incidentes detectados: {}\n", paint(&st.active_incidents_count.to_string(), if st.active_incidents_count > 0 { YELLOW } else { CYAN }));
        }

        "stop" => {
            println!("\n{} Deteniendo centinela...", paint("antOS Autopilot ·", BOLD));
            let st = crate::autopilot::AutopilotEngine::stop(&ctx.state, &ctx.workspace)?;
            println!("  Estado:               {}", paint("DETENIDO", RED));
            println!("  Incidentes resueltos: {}\n", st.resolved_incidents_count);
        }

        "scan" => {
            println!("\n{} Escaneando el workspace en busca de errores y fallos...", paint("antOS Autopilot ·", BOLD));
            let incs = crate::autopilot::AutopilotEngine::scan_workspace(&ctx.state, &ctx.workspace)?;
            if incs.is_empty() {
                println!("  ✓ {} Repositorio limpio, cero incidencias.", paint("OK", GREEN));
            } else {
                println!("  ⚠ Detectadas {} incidencias con propuestas generadas:", paint(&incs.len().to_string(), YELLOW));
                for inc in incs {
                    println!("    • [{}] {} en «{}» — {}", paint(&inc.id, BOLD), paint(&inc.incident_type, CYAN), inc.file_path, inc.error_message);
                    if let Some(ref prop) = inc.fix_proposal {
                        println!("      Rama: {} | QA: {}", prop.branch, prop.test_output.lines().next().unwrap_or(""));
                    }
                }
            }
            println!();
        }

        "list" | "log" | "incidents" => {
            let incs = crate::autopilot::AutopilotEngine::list_incidents(&ctx.state)?;
            if incs.is_empty() {
                println!("\n{} No hay incidencias registradas.", paint("antOS Autopilot ·", BOLD));
            } else {
                println!("\n{} Historial de Incidencias ({}):", paint("antOS Autopilot ·", BOLD), incs.len());
                for inc in incs {
                    let st_badge = match inc.status.as_str() {
                        "resolved" => paint("RESUELTO", GREEN),
                        "ready_for_approval" => paint("PENDIENTE", YELLOW),
                        "dismissed" => paint("DESCARTADO", DIM),
                        other => paint(other, CYAN),
                    };
                    println!("  • [{}] {} en «{}» [{}]", paint(&inc.id, BOLD), paint(&inc.incident_type, CYAN), inc.file_path, st_badge);
                    println!("    Detalle: {}", inc.error_message);
                    if let Some(ref prop) = inc.fix_proposal {
                        println!("    Fix:     {}", prop.title);
                    }
                }
            }
            println!();
        }

        "approve" | "merge" => {
            let incident_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos autopilot approve <incident_id>"))?;
            println!("\n{} Aprobando propuesta para incidente «{}»...", paint("antOS Autopilot ·", BOLD), incident_id);
            let inc = crate::autopilot::AutopilotEngine::resolve_incident(&ctx.state, &ctx.workspace, incident_id, true)?;
            println!("  ✓ {} Corrección aplicada en {}", paint("APROBADO", GREEN), inc.file_path);
            println!("  Estado: {}\n", paint(&inc.status, BOLD));
        }

        "reject" | "dismiss" => {
            let incident_id = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos autopilot reject <incident_id>"))?;
            println!("\n{} Descartando propuesta para incidente «{}»...", paint("antOS Autopilot ·", BOLD), incident_id);
            let _inc = crate::autopilot::AutopilotEngine::resolve_incident(&ctx.state, &ctx.workspace, incident_id, false)?;
            println!("  ✓ {} Incidente descartado.\n", paint("DESCARTADO", DIM));
        }

        "status" | _ => {
            let st = crate::autopilot::AutopilotEngine::status(&ctx.state, &ctx.workspace)?;
            let status_badge = if st.active { paint("ACTIVO (Vigilando)", GREEN) } else { paint("DETENIDO", RED) };
            println!("\n{} Estado del Centinela Autónomo:", paint("antOS Autopilot ·", BOLD));
            println!("  • Estado:                 {}", status_badge);
            println!("  • Espacio de Trabajo:     {}", st.workspace_path);
            println!("  • Intervalo de Sondeo:    {}s", st.poll_interval_secs);
            println!("  • Incidencias Activas:    {}", paint(&st.active_incidents_count.to_string(), if st.active_incidents_count > 0 { YELLOW } else { GREEN }));
            println!("  • Incidencias Resueltas:  {}", st.resolved_incidents_count);
            if let Some(ts) = st.last_scan_timestamp {
                println!("  • Último Escaneo:         {}", ts);
            }
            println!("\n  Uso:");
            println!("    antos autopilot start [--interval <secs>] [--auto-merge]  Arranca el centinela");
            println!("    antos autopilot stop                                      Detiene el centinela");
            println!("    antos autopilot status                                    Muestra métricas y estado");
            println!("    antos autopilot scan                                      Escaneo manual reactivo");
            println!("    antos autopilot list                                      Historial de incidentes");
            println!("    antos autopilot approve <incident_id>                     Aprueba y aplica fix");
            println!("    antos autopilot reject <incident_id>                      Descarta la propuesta\n");
        }
    }
    Ok(())
}

// ----------------------------------------------------- web console (T16.4)

pub fn cmd_web(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "start" => {
            let mut bind = "127.0.0.1".to_string();
            let mut port = 8088u16;
            let mut i = 1;
            while i < args.len() {
                if (args[i] == "--bind" || args[i] == "-b") && i + 1 < args.len() {
                    bind = args[i + 1].clone();
                    i += 2;
                } else if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
                    if let Ok(p) = args[i + 1].parse::<u16>() {
                        port = p;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }

            let config = antos_protocol::WebConsoleConfig {
                bind_addr: bind,
                port,
                auth_required: true,
                ws_ping_interval_secs: 30,
            };

            println!("\n{} Iniciando consola web remota y bridge WebSocket...", paint("antOS Web Console ·", BOLD));
            let st = crate::web::WebEngine::start(&ctx.state, &ctx.workspace, config)?;
            let token_sess = crate::web::WebEngine::generate_token(&ctx.state, Some("admin".into()), Some(86400))?;
            println!("  Estado:               {}", paint("ACTIVO (En línea)", GREEN));
            println!("  URL de Acceso:        {}", paint(&st.url, CYAN));
            println!("  URL con Token:        {}", paint(&format!("{}?token={}", st.url, token_sess.token), BOLD));
            println!("  WebSocket Bridge:     {}/ws/events\n", st.url);
        }

        "stop" => {
            println!("\n{} Deteniendo servidor de consola web...", paint("antOS Web Console ·", BOLD));
            let st = crate::web::WebEngine::stop(&ctx.state)?;
            println!("  Estado:               {}\n", paint(if st.running { "ACTIVO" } else { "DETENIDO" }, RED));
        }

        "token" => {
            let mut label = None;
            let mut ttl = None;
            let mut i = 1;
            while i < args.len() {
                if (args[i] == "--label" || args[i] == "-l") && i + 1 < args.len() {
                    label = Some(args[i + 1].clone());
                    i += 2;
                } else if (args[i] == "--ttl" || args[i] == "-t") && i + 1 < args.len() {
                    if let Ok(v) = args[i + 1].parse::<u64>() {
                        ttl = Some(v);
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }

            println!("\n{} Generando token de autenticación...", paint("antOS Web Console ·", BOLD));
            let session = crate::web::WebEngine::generate_token(&ctx.state, label, ttl)?;
            let st = crate::web::WebEngine::status(&ctx.state)?;
            println!("  Token:                {}", paint(&session.token, BOLD));
            println!("  Expira en:            {}s", session.expires_at.saturating_sub(session.created_at));
            if let Some(ref l) = session.client_label {
                println!("  Cliente / Dispositivo: {}", l);
            }
            println!("  Enlace de Conexión:   {}\n", paint(&format!("{}?token={}", st.url, session.token), CYAN));
        }

        "status" | _ => {
            let st = crate::web::WebEngine::status(&ctx.state)?;
            let status_badge = if st.running { paint("ACTIVO (En línea)", GREEN) } else { paint("DETENIDO", RED) };
            println!("\n{} Estado del Servidor Web y Bridge WebSocket:", paint("antOS Web Console ·", BOLD));
            println!("  • Estado:                 {}", status_badge);
            println!("  • Dirección y Puerto:     {}:{}", st.bind_addr, st.port);
            println!("  • URL de Acceso:          {}", paint(&st.url, CYAN));
            println!("  • Clientes Conectados:    {}", st.connected_clients);
            println!("  • Sesiones Token Activas: {}", st.active_sessions_count);
            println!("\n  Uso:");
            println!("    antos web start [--port <puerto>] [--bind <ip>]  Inicia el servidor web");
            println!("    antos web stop                                  Detiene el servidor");
            println!("    antos web status                                Muestra estado y métricas");
            println!("    antos web token [--label <nombre>] [--ttl <s]>  Genera enlace seguro\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ tickets

pub fn cmd_barra(_ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let manager = crate::barra::BarraManager::global();

    match sub {
        "alert" | "notif" | "notify" | "alerta" => {
            let clean_parts: Vec<&str> = args[1..]
                .iter()
                .filter(|a| *a != "--urgent" && *a != "-u")
                .map(|s| s.as_str())
                .collect();
            let msg = if clean_parts.is_empty() {
                "Prueba de alerta visual".to_string()
            } else {
                clean_parts.join(" ")
            };
            let alert = antos_protocol::BarraAlert {
                category: "cli".into(),
                message: msg.clone(),
                urgent: args.iter().any(|a| a == "--urgent" || a == "-u"),
            };
            manager.emit_alert(alert)?;
            println!(
                "\n{} Alerta visual emitida a la barra de escritorio: «{}»\n",
                paint("antOS Barra ·", BOLD),
                paint(&msg, GREEN)
            );
        }
        "status" | "telemetry" | "telemetria" | _ => {
            let t = manager.get_telemetry();
            let mb = t.profiler_rss_bytes as f64 / (1024.0 * 1024.0);
            println!(
                "\n{} Telemetría en Tiempo Real de la Barra de Escritorio:",
                paint("antOS Barra ·", BOLD)
            );
            println!(
                "  • eBPF LSM Guard:       {}",
                if t.ebpf_lsm_active {
                    paint("Activo", GREEN)
                } else {
                    paint("Auditoría", YELLOW)
                }
            );
            println!(
                "  • Violaciones LSM:      {}",
                if t.ebpf_violations_count > 0 {
                    paint(&t.ebpf_violations_count.to_string(), RED)
                } else {
                    paint("0", GREEN)
                }
            );
            println!("  • Consumo RSS Pico:     {:.2} MB", mb);
            println!("  • CPU Estimada:         {:.1}%", t.profiler_cpu_percent);
            println!(
                "  • Sesión de Pair:       {}",
                paint(t.active_pair_session.as_deref().unwrap_or("inactiva"), CYAN)
            );
            println!(
                "  • Nodos antMesh:        {} vecinos descubiertos",
                t.mesh_peers_count
            );
            println!(
                "  • Notificaciones:       {} pendientes",
                t.active_notifications_count
            );
            println!("\n  Alertas recientes en cola:");
            let alerts = manager.get_alerts(3);
            if alerts.is_empty() {
                println!("    (sin alertas recientes)");
            } else {
                for a in alerts {
                    let u = if a.urgent {
                        paint("[URGENTE]", RED)
                    } else {
                        paint("[INFO]", CYAN)
                    };
                    println!("    • {u} {}: {}", a.category, a.message);
                }
            }
            println!();
        }
    }
    Ok(())
}

// --------------------------------------------------------------------- boot

pub fn cmd_desktop(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "start" | "iniciar" | "run" => {
            let nested = args.iter().any(|a| a == "--nested" || a == "-n");
            println!(
                "\n{} Inicializando entorno gráfico Wayland de antOS...",
                paint("antOS Desktop ·", BOLD)
            );
            let _ = crate::desktop::DesktopManager::sync_configuration(&ctx.workspace)?;
            let out = crate::desktop::DesktopManager::start_session(&ctx.workspace, nested)?;
            println!("{out}");
        }
        "keys" | "hotkeys" | "atajos" => {
            let keys = crate::desktop::DesktopManager::get_hotkeys();
            println!(
                "\n{} Atajos de Teclado Globales del Entorno de Escritorio:",
                paint("antOS Desktop ·", BOLD)
            );
            println!(
                "  {:<16} {:<24} {}",
                paint("ATAJO", BOLD),
                paint("ACCIÓN", BOLD),
                paint("DESCRIPCIÓN", BOLD)
            );
            println!("  {}", "─".repeat(78));
            for k in keys {
                println!(
                    "  {:<16} {:<24} {}",
                    paint(&k.key, CYAN),
                    paint(&k.action, YELLOW),
                    k.description
                );
            }
            println!();
        }
        "status" | "estado" | _ => {
            let status = crate::desktop::DesktopManager::get_status();
            let st = if status.running {
                paint("En ejecución", GREEN)
            } else {
                paint("Inactivo / Headless", DIM)
            };
            println!(
                "\n{} Diagnóstico de Sesión Gráfica Wayland:",
                paint("antOS Desktop ·", BOLD)
            );
            println!("  Estado:                {}", st);
            println!(
                "  Compositor:            {}",
                paint(&status.compositor_name, CYAN)
            );
            println!(
                "  WAYLAND_DISPLAY:       {}",
                paint(
                    status.wayland_display.as_deref().unwrap_or("ninguno"),
                    YELLOW
                )
            );
            println!("  Clientes de capa:      {}", status.active_clients_count);
            println!(
                "  Atajos registrados:    {} combinaciones globales\n",
                status.registered_hotkeys.len()
            );
            println!("  Uso:");
            println!("    antos desktop start       Arranca la sesión de escritorio");
            println!("    antos desktop keys        Muestra todos los atajos de teclado globales");
            println!("    antos desktop status      Diagnostica la sesión activa\n");
        }
    }
    Ok(())
}

// -------------------------------------------------------------------- barra

