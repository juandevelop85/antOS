//! Ejecutor de comandos del instalador (T36.2).
//!
//! La instalación real y la simulación recorren **la misma tubería**
//! ([`super::DeployEngine::install`]); lo único que cambia es el
//! [`InstallRunner`] que reciben. [`SystemRunner`] lanza los procesos de
//! verdad (`parted`, `mkfs.*`, `mount`, `nixos-install`, …);
//! [`SimulatedRunner`] graba cada comando tal cual se habría ejecutado y
//! devuelve salidas sintéticas coherentes con el disco elegido. Así la
//! simulación enseña exactamente lo que la instalación hará, y los tests
//! fijan la secuencia completa sin tocar un disco ni necesitar `parted`.
//!
//! Ningún comando pasa por `sh -c` (T31.4): programa y argumentos van
//! siempre como vector.

use antos_protocol::DiskDevice;
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Líneas de salida que se conservan para el mensaje de error cuando un
/// comando largo (`nixos-install`) falla.
pub const ERROR_TAIL_LINES: usize = 50;

/// Lo que la tubería de instalación necesita del sistema.
pub trait InstallRunner {
    /// Ejecuta `program args…` con `stdin` opcional y devuelve su `stdout`.
    /// Un estado de salida distinto de cero es un error con el `stderr`.
    fn run(&mut self, program: &str, args: &[String], stdin: Option<&str>) -> Result<String>;

    /// Como [`InstallRunner::run`], pero entregando cada línea de salida
    /// (`stdout` y `stderr` mezclados) a `on_line` según llega.
    fn run_streaming(
        &mut self,
        program: &str,
        args: &[String],
        on_line: &mut dyn FnMut(&str),
    ) -> Result<()>;

    /// `true` si `path` es hoy un punto de montaje.
    fn is_mountpoint(&self, path: &Path) -> bool;

    /// `true` si los comandos se ejecutan de verdad (rellena
    /// `InstallStep.executed`).
    fn is_real(&self) -> bool;
}

/// Ejecutor real: `std::process::Command`.
pub struct SystemRunner;

impl SystemRunner {
    fn command_line(program: &str, args: &[String]) -> String {
        std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl InstallRunner for SystemRunner {
    fn run(&mut self, program: &str, args: &[String], stdin: Option<&str>) -> Result<String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().with_context(|| {
            format!("No se pudo lanzar `{}`", Self::command_line(program, args))
        })?;
        if let Some(input) = stdin {
            if let Some(mut pipe) = child.stdin.take() {
                pipe.write_all(input.as_bytes())
                    .with_context(|| format!("Escribiendo en la entrada de `{program}`"))?;
            }
        }
        let out = child
            .wait_with_output()
            .with_context(|| format!("Esperando a `{program}`"))?;
        if !out.status.success() {
            bail!(
                "`{}` terminó con {}:\n{}",
                Self::command_line(program, args),
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn run_streaming(
        &mut self,
        program: &str,
        args: &[String],
        on_line: &mut dyn FnMut(&str),
    ) -> Result<()> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!("No se pudo lanzar `{}`", Self::command_line(program, args))
            })?;

        // `stderr` se lee en un hilo aparte para que ninguno de los dos
        // pipes se llene y bloquee al hijo; las líneas se juntan por un
        // canal y salen por `on_line` en el orden en que llegan.
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let mut readers = Vec::new();
        if let Some(stdout) = child.stdout.take() {
            let tx = tx.clone();
            readers.push(std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            }));
        }
        if let Some(stderr) = child.stderr.take() {
            let tx = tx.clone();
            readers.push(std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            }));
        }
        drop(tx);

        let mut tail: std::collections::VecDeque<String> =
            std::collections::VecDeque::with_capacity(ERROR_TAIL_LINES);
        for line in rx {
            on_line(&line);
            if tail.len() == ERROR_TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        for r in readers {
            let _ = r.join();
        }
        let status = child
            .wait()
            .with_context(|| format!("Esperando a `{program}`"))?;
        if !status.success() {
            bail!(
                "`{}` terminó con {}. Últimas {} líneas:\n{}",
                Self::command_line(program, args),
                status,
                tail.len(),
                tail.iter().cloned().collect::<Vec<_>>().join("\n")
            );
        }
        Ok(())
    }

    fn is_mountpoint(&self, path: &Path) -> bool {
        let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
            return false;
        };
        let wanted = path.to_string_lossy();
        mounts
            .lines()
            .filter_map(|l| l.split_whitespace().nth(1))
            .any(|mp| mp == wanted)
    }

    fn is_real(&self) -> bool {
        true
    }
}

/// Región de una partición sintética, en MiB.
#[derive(Debug, Clone)]
struct SimulatedPartition {
    number: u32,
    start_mib: u64,
    end_mib: u64,
    fs_type: String,
    name: String,
    flags: String,
}

/// Ejecutor de simulación: graba cada comando y contesta con salidas
/// sintéticas derivadas del [`DiskDevice`] elegido. Las escrituras a disco
/// no ocurren; lo único que «cambia» es la tabla de particiones que el
/// propio runner mantiene en memoria para que un `print free` posterior a
/// un `mkpart` vea la partición nueva, igual que haría `parted`.
pub struct SimulatedRunner {
    /// Cada comando tal cual se habría lanzado (`programa arg1 arg2 …`).
    pub commands: Vec<String>,
    disk_path: String,
    disk_size_mib: u64,
    model: String,
    partitions: Vec<SimulatedPartition>,
    mounted: HashSet<PathBuf>,
    /// Comandos (por programa) que deben fallar, para probar la limpieza.
    failing_programs: HashSet<String>,
}

impl SimulatedRunner {
    /// Construye la simulación sobre el disco tal y como lo describe
    /// `DiskManager` (sus particiones se conservan; el resto es hueco libre
    /// al final).
    pub fn for_disk(disk: &DiskDevice) -> Self {
        let mut next_start = 1u64;
        let partitions = disk
            .partitions
            .iter()
            .map(|p| {
                let size = (p.size_bytes / (1024 * 1024)).max(1);
                let part = SimulatedPartition {
                    number: p.number,
                    start_mib: next_start,
                    end_mib: next_start + size,
                    fs_type: p.fs_type.clone().unwrap_or_default(),
                    name: if p.is_efi {
                        "EFI System Partition".into()
                    } else {
                        String::new()
                    },
                    flags: if p.is_efi {
                        "boot, esp".into()
                    } else {
                        String::new()
                    },
                };
                next_start = part.end_mib;
                part
            })
            .collect();
        Self {
            commands: Vec::new(),
            disk_path: disk.path.clone(),
            disk_size_mib: (disk.size_bytes / (1024 * 1024)).max(2),
            model: disk.model.clone(),
            partitions,
            mounted: HashSet::new(),
            failing_programs: HashSet::new(),
        }
    }

    /// Hace que cualquier invocación de `program` falle (tests).
    pub fn fail_on(mut self, program: &str) -> Self {
        self.failing_programs.insert(program.to_string());
        self
    }

    fn record(&mut self, program: &str, args: &[String]) -> String {
        let line = std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        self.commands.push(line.clone());
        line
    }

    fn parse_mib(value: &str) -> Option<u64> {
        value
            .trim_end_matches("MiB")
            .trim_end_matches('%')
            .parse::<f64>()
            .ok()
            .map(|v| v.round() as u64)
    }

    /// Salida de `parted -s -m <dev> unit MiB print free`, con los huecos
    /// libres intercalados como hace `parted`.
    fn print_free(&self) -> String {
        let mut lines = vec![
            "BYT;".to_string(),
            format!(
                "{}:{}MiB:scsi:512:512:gpt:{}:;",
                self.disk_path, self.disk_size_mib, self.model
            ),
        ];
        let mut parts = self.partitions.clone();
        parts.sort_by_key(|p| p.start_mib);
        let mut cursor = 1u64;
        for p in &parts {
            if p.start_mib > cursor + 1 {
                lines.push(format!(
                    "1:{}MiB:{}MiB:{}MiB:free;",
                    cursor,
                    p.start_mib,
                    p.start_mib - cursor
                ));
            }
            lines.push(format!(
                "{}:{}MiB:{}MiB:{}MiB:{}:{}:{};",
                p.number,
                p.start_mib,
                p.end_mib,
                p.end_mib - p.start_mib,
                p.fs_type,
                p.name,
                p.flags
            ));
            cursor = p.end_mib;
        }
        if self.disk_size_mib > cursor + 1 {
            lines.push(format!(
                "1:{}MiB:{}MiB:{}MiB:free;",
                cursor,
                self.disk_size_mib - 1,
                self.disk_size_mib - 1 - cursor
            ));
        }
        lines.join("\n") + "\n"
    }

    /// Aplica a la tabla en memoria lo que `parted -s <dev> …` haría.
    fn apply_parted(&mut self, args: &[String]) {
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "mklabel" => {
                    self.partitions.clear();
                    i += 2;
                }
                "mkpart" if i + 4 < args.len() => {
                    let name = args[i + 1].clone();
                    let fs_type = args[i + 2].clone();
                    let start = Self::parse_mib(&args[i + 3]).unwrap_or(1);
                    let end = if args[i + 4].ends_with('%') {
                        self.disk_size_mib - 1
                    } else {
                        Self::parse_mib(&args[i + 4]).unwrap_or(start + 1)
                    };
                    let number = self.partitions.iter().map(|p| p.number).max().unwrap_or(0) + 1;
                    self.partitions.push(SimulatedPartition {
                        number,
                        start_mib: start,
                        end_mib: end,
                        fs_type,
                        name,
                        flags: String::new(),
                    });
                    i += 5;
                }
                "set" if i + 3 < args.len() => {
                    if let Ok(n) = args[i + 1].parse::<u32>() {
                        if let Some(p) = self.partitions.iter_mut().find(|p| p.number == n) {
                            p.flags = format!("boot, {}", args[i + 2]);
                        }
                    }
                    i += 4;
                }
                _ => i += 1,
            }
        }
    }
}

impl InstallRunner for SimulatedRunner {
    fn run(&mut self, program: &str, args: &[String], _stdin: Option<&str>) -> Result<String> {
        let line = self.record(program, args);
        if self.failing_programs.contains(program) {
            bail!("`{line}` terminó con exit status: 1 (fallo simulado)");
        }
        match program {
            "parted" => {
                if args.iter().any(|a| a == "print") {
                    return Ok(self.print_free());
                }
                // `parted -s <dev> <órdenes…>`: las órdenes empiezan tras el
                // dispositivo.
                let dev_pos = args
                    .iter()
                    .position(|a| a.starts_with("/dev/"))
                    .unwrap_or(0);
                let orders = args[dev_pos + 1..].to_vec();
                self.apply_parted(&orders);
                Ok(String::new())
            }
            "blkid" => {
                // `blkid -s UUID -o value <part>`: UUID sintético estable por
                // partición, con el formato de cada sistema de ficheros.
                let part = args.last().cloned().unwrap_or_default();
                let n: u32 = part
                    .chars()
                    .rev()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0);
                let is_esp = self
                    .partitions
                    .iter()
                    .any(|p| p.number == n && p.flags.contains("esp"));
                Ok(if is_esp {
                    format!("SIM{n}-ESP0\n")
                } else {
                    format!("00000000-0000-4000-8000-{n:012x}\n")
                })
            }
            "mount" => {
                if let Some(target) = args.last() {
                    self.mounted.insert(PathBuf::from(target));
                }
                Ok(String::new())
            }
            "umount" => {
                if let Some(target) = args.last() {
                    let target = PathBuf::from(target);
                    self.mounted.retain(|m| !m.starts_with(&target));
                }
                Ok(String::new())
            }
            _ => Ok(String::new()),
        }
    }

    fn run_streaming(
        &mut self,
        program: &str,
        args: &[String],
        on_line: &mut dyn FnMut(&str),
    ) -> Result<()> {
        let line = self.record(program, args);
        if self.failing_programs.contains(program) {
            bail!("`{line}` terminó con exit status: 1 (fallo simulado)");
        }
        on_line(&format!("(simulado) {line}"));
        Ok(())
    }

    fn is_mountpoint(&self, path: &Path) -> bool {
        self.mounted.contains(path)
    }

    fn is_real(&self) -> bool {
        false
    }
}

/// Analiza la salida de `parted -s -m <dev> unit MiB print free`
/// (formato «machine-readable»: campos separados por `:` y `;` final).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartedRegion {
    /// Número de partición; `None` en un hueco libre.
    pub number: Option<u32>,
    pub start_mib: u64,
    pub end_mib: u64,
    pub size_mib: u64,
    pub fs_type: String,
    pub name: String,
    pub flags: String,
}

impl PartedRegion {
    pub fn is_free(&self) -> bool {
        self.number.is_none()
    }

    pub fn is_esp(&self) -> bool {
        self.flags.split(',').any(|f| f.trim() == "esp")
    }
}

/// Devuelve las regiones (particiones y huecos) en el orden del disco.
pub fn parse_parted_print_free(output: &str) -> Vec<PartedRegion> {
    let mib = |v: &str| -> u64 {
        v.trim_end_matches("MiB")
            .parse::<f64>()
            .map(|f| f.round() as u64)
            .unwrap_or(0)
    };
    output
        .lines()
        .skip(2) // `BYT;` y la línea del disco
        .filter_map(|line| {
            let line = line.trim().trim_end_matches(';');
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 5 {
                return None;
            }
            if fields[4] == "free" {
                return Some(PartedRegion {
                    number: None,
                    start_mib: mib(fields[1]),
                    end_mib: mib(fields[2]),
                    size_mib: mib(fields[3]),
                    fs_type: "free".into(),
                    name: String::new(),
                    flags: String::new(),
                });
            }
            Some(PartedRegion {
                number: fields[0].parse().ok(),
                start_mib: mib(fields[1]),
                end_mib: mib(fields[2]),
                size_mib: mib(fields[3]),
                fs_type: fields[4].to_string(),
                name: fields.get(5).unwrap_or(&"").to_string(),
                flags: fields.get(6).unwrap_or(&"").to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    const REAL_PARTED_OUTPUT: &str = "BYT;\n\
/dev/nvme0n1:953870MiB:nvme:512:512:gpt:Samsung SSD 980 PRO 1TB:;\n\
1:1.00MiB:101MiB:100MiB:fat32:EFI system partition:boot, esp;\n\
2:101MiB:117MiB:16.0MiB::Microsoft reserved partition:msftres;\n\
3:117MiB:500000MiB:499883MiB:ntfs:Basic data partition:msftdata;\n\
1:500000MiB:953870MiB:453870MiB:free;\n";

    #[test]
    fn test_parse_parted_print_free_real_layout() {
        let regions = parse_parted_print_free(REAL_PARTED_OUTPUT);
        assert_eq!(regions.len(), 4);
        assert!(regions[0].is_esp());
        assert_eq!(regions[0].number, Some(1));
        assert_eq!(regions[0].fs_type, "fat32");
        assert!(!regions[2].is_esp());
        let free = regions.iter().find(|r| r.is_free()).unwrap();
        assert_eq!(free.start_mib, 500000);
        assert_eq!(free.size_mib, 453870);
    }

    #[test]
    fn test_simulated_runner_print_free_reflects_mkpart() {
        let disk = &crate::installer::DiskManager::synthetic_disk_devices()[0];
        let mut runner = SimulatedRunner::for_disk(disk);
        let args = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
        let before = runner
            .run(
                "parted",
                &args("-s -m /dev/nvme0n1 unit MiB print free"),
                None,
            )
            .unwrap();
        let regions = parse_parted_print_free(&before);
        assert!(regions[0].is_esp());
        let free = regions
            .iter()
            .find(|r| r.is_free())
            .expect("hueco libre al final");

        runner
            .run(
                "parted",
                &args(&format!(
                    "-s /dev/nvme0n1 mkpart antos-root ext4 {}MiB {}MiB",
                    free.start_mib, free.end_mib
                )),
                None,
            )
            .unwrap();
        let after = parse_parted_print_free(
            &runner
                .run(
                    "parted",
                    &args("-s -m /dev/nvme0n1 unit MiB print free"),
                    None,
                )
                .unwrap(),
        );
        let new = after
            .iter()
            .find(|r| r.name == "antos-root")
            .expect("la partición nueva aparece");
        assert_eq!(new.number, Some(3));
        assert_eq!(new.start_mib, free.start_mib);
        assert_eq!(runner.commands.len(), 3);
    }

    #[test]
    fn test_simulated_runner_mklabel_clears_and_set_marks_esp() {
        let disk = &crate::installer::DiskManager::synthetic_disk_devices()[0];
        let mut runner = SimulatedRunner::for_disk(disk);
        let args = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
        runner
            .run(
                "parted",
                &args("-s /dev/nvme0n1 mklabel gpt mkpart ESP fat32 1MiB 513MiB set 1 esp on mkpart antos-root ext4 513MiB 100%"),
                None,
            )
            .unwrap();
        let regions = parse_parted_print_free(
            &runner
                .run(
                    "parted",
                    &args("-s -m /dev/nvme0n1 unit MiB print free"),
                    None,
                )
                .unwrap(),
        );
        let parts: Vec<_> = regions.iter().filter(|r| !r.is_free()).collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].is_esp());
        assert_eq!(parts[1].name, "antos-root");
        let esp_uuid = runner
            .run("blkid", &args("-s UUID -o value /dev/nvme0n1p1"), None)
            .unwrap();
        assert!(esp_uuid.trim().ends_with("-ESP0"));
    }

    #[test]
    fn test_system_runner_reports_failure_with_stderr_and_no_shell() {
        let mut runner = SystemRunner;
        // `false` existe en todo POSIX y no necesita intérprete.
        let err = runner.run("false", &[], None).unwrap_err();
        assert!(err.to_string().contains("`false` terminó con"));
        let out = runner
            .run("printf", &["%s".to_string(), "hola".to_string()], None)
            .unwrap();
        assert_eq!(out, "hola");
    }

    #[test]
    fn test_system_runner_streaming_delivers_lines_and_keeps_tail_on_failure() {
        let mut runner = SystemRunner;
        let mut lines = Vec::new();
        runner
            .run_streaming("printf", &["a\\nb\\n".to_string()], &mut |l| {
                lines.push(l.to_string())
            })
            .unwrap();
        assert_eq!(lines, vec!["a", "b"]);
        let err = runner.run_streaming("false", &[], &mut |_| {}).unwrap_err();
        assert!(err.to_string().contains("Últimas 0 líneas"));
    }
}
