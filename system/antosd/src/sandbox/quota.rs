//! Resource Quotas and Sandbox Isolation via cgroups v2 and Seatbelt/Watchdog (Ticket T7.2).
//!
//! Enforces CPU limits, memory quotas, process count caps, and execution timeouts
//! to prevent infinite loops, memory leaks, and fork-bombs during agent task executions.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

/// Default resource constraints for sandboxed agent tasks.
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
pub const DEFAULT_MEMORY_MB: u64 = 1024;
pub const DEFAULT_CPU_PERCENT: u32 = 100;
pub const DEFAULT_MAX_PIDS: u32 = 64;

/// Resource quotas configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceQuota {
    pub timeout_secs: u64,
    pub max_memory_mb: u64,
    pub cpu_quota_percent: u32,
    pub max_pids: u32,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            max_memory_mb: DEFAULT_MEMORY_MB,
            cpu_quota_percent: DEFAULT_CPU_PERCENT,
            max_pids: DEFAULT_MAX_PIDS,
        }
    }
}

/// Snapshot of resource consumption during execution.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub elapsed_millis: u64,
    pub peak_memory_mb: u64,
    pub cpu_time_millis: u64,
    pub terminated_by_quota: bool,
    pub violation_reason: Option<String>,
}

/// Linux cgroup v2 controller interface.
pub struct CgroupV2Manager;

impl CgroupV2Manager {
    pub const CGROUP_ROOT: &str = "/sys/fs/cgroup";

    /// Checks if the host has cgroups v2 enabled and accessible.
    pub fn is_available() -> bool {
        #[cfg(target_os = "linux")]
        {
            Path::new(Self::CGROUP_ROOT).join("cgroup.controllers").exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Creates an antOS isolation cgroup and configures memory and cpu controllers.
    pub fn setup_cgroup(group_name: &str, quota: &ResourceQuota) -> Result<PathBuf> {
        let base_path = Path::new(Self::CGROUP_ROOT).join("antos").join(group_name);
        fs::create_dir_all(&base_path)
            .with_context(|| format!("failed to create cgroup at {}", base_path.display()))?;

        // 1. Set memory limit (in bytes)
        let memory_bytes = quota.max_memory_mb * 1024 * 1024;
        let memory_max_file = base_path.join("memory.max");
        let _ = fs::write(&memory_max_file, memory_bytes.to_string());

        // 2. Set CPU quota: "quota_us period_us" (e.g. 100000 100000 for 100%)
        let cpu_max_file = base_path.join("cpu.max");
        let period_us = 100_000u64;
        let quota_us = (period_us * quota.cpu_quota_percent as u64) / 100;
        let _ = fs::write(&cpu_max_file, format!("{quota_us} {period_us}"));

        // 3. Set maximum processes (fork-bomb guard)
        let pids_max_file = base_path.join("pids.max");
        let _ = fs::write(&pids_max_file, quota.max_pids.to_string());

        Ok(base_path)
    }

    /// Moves a child PID into the cgroup.
    pub fn attach_pid(cgroup_path: &Path, pid: u32) -> Result<()> {
        let procs_file = cgroup_path.join("cgroup.procs");
        fs::write(&procs_file, pid.to_string())
            .with_context(|| format!("failed to attach PID {pid} to cgroup {}", procs_file.display()))?;
        Ok(())
    }

    /// Cleans up the cgroup directory when execution completes.
    pub fn cleanup(cgroup_path: &Path) -> Result<()> {
        if cgroup_path.exists() {
            let _ = fs::remove_dir(cgroup_path);
        }
        Ok(())
    }
}

/// Process watchdog that monitors execution time and memory limits.
pub struct ProcessWatchdog {
    quota: ResourceQuota,
    cgroup_path: Option<PathBuf>,
}

impl ProcessWatchdog {
    pub fn new(quota: ResourceQuota) -> Self {
        Self {
            quota,
            cgroup_path: None,
        }
    }

    /// Sets up cgroup v2 on Linux if available.
    pub fn attach_child(&mut self, child_pid: u32) -> Result<()> {
        if CgroupV2Manager::is_available() {
            let cgroup_name = format!("task-{}", child_pid);
            if let Ok(path) = CgroupV2Manager::setup_cgroup(&cgroup_name, &self.quota) {
                let _ = CgroupV2Manager::attach_pid(&path, child_pid);
                self.cgroup_path = Some(path);
            }
        }
        Ok(())
    }

    /// Supervises the child process until exit, enforcing timeouts and memory limits.
    #[allow(dead_code)]
    pub fn supervise(&self, mut child: Child) -> Result<(std::process::ExitStatus, ResourceUsage)> {
        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.quota.timeout_secs);
        let pid = child.id();
        let mut peak_memory_mb = 0u64;

        loop {
            // 1. Check if process already exited
            if let Some(status) = child.try_wait()? {
                let elapsed = start_time.elapsed();
                self.cleanup();
                return Ok((
                    status,
                    ResourceUsage {
                        elapsed_millis: elapsed.as_millis() as u64,
                        peak_memory_mb,
                        cpu_time_millis: elapsed.as_millis() as u64,
                        terminated_by_quota: false,
                        violation_reason: None,
                    },
                ));
            }

            let elapsed = start_time.elapsed();

            // 2. Check timeout limit
            if elapsed >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                self.cleanup();
                bail!(
                    "límite de recursos excedido: tiempo de ejecución agotado (timeout de {}s superado)",
                    self.quota.timeout_secs
                );
            }

            // 3. Check memory limit via process inspector
            if let Some(rss_mb) = read_process_rss_mb(pid) {
                if rss_mb > peak_memory_mb {
                    peak_memory_mb = rss_mb;
                }
                if rss_mb > self.quota.max_memory_mb {
                    let _ = child.kill();
                    let _ = child.wait();
                    self.cleanup();
                    bail!(
                        "límite de recursos excedido: cuota de memoria superada (consumo {} MB > límite {} MB)",
                        rss_mb,
                        self.quota.max_memory_mb
                    );
                }
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Supervises child and captures stdio output safely with timeout and quota enforcement.
    pub fn supervise_output(&self, mut child: Child) -> Result<std::process::Output> {
        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.quota.timeout_secs);
        let pid = child.id();

        loop {
            if let Some(_) = child.try_wait()? {
                self.cleanup();
                return child.wait_with_output().context("failed to collect child output");
            }

            let elapsed = start_time.elapsed();
            if elapsed >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                self.cleanup();
                bail!(
                    "límite de recursos excedido: tiempo de ejecución agotado (timeout de {}s superado)",
                    self.quota.timeout_secs
                );
            }

            if let Some(rss_mb) = read_process_rss_mb(pid) {
                if rss_mb > self.quota.max_memory_mb {
                    let _ = child.kill();
                    let _ = child.wait();
                    self.cleanup();
                    bail!(
                        "límite de recursos excedido: cuota de memoria superada (consumo {} MB > límite {} MB)",
                        rss_mb,
                        self.quota.max_memory_mb
                    );
                }
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn cleanup(&self) {
        if let Some(ref path) = self.cgroup_path {
            let _ = CgroupV2Manager::cleanup(path);
        }
    }
}

/// Reads resident set size (RSS) in megabytes for a process.
pub fn read_process_rss_mb(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let statm_path = format!("/proc/{pid}/statm");
        if let Ok(content) = fs::read_to_string(&statm_path) {
            let parts: Vec<&str> = content.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(resident_pages) = parts[1].parse::<u64>() {
                    // Page size is typically 4096 bytes
                    let bytes = resident_pages * 4096;
                    return Some(bytes / (1024 * 1024));
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, probe using ps -o rss= -p <pid>
        let output = std::process::Command::new("ps")
            .arg("-o")
            .arg("rss=")
            .arg("-p")
            .arg(pid.to_string())
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if let Ok(kb) = text.parse::<u64>() {
                    return Some(kb / 1024);
                }
            }
        }
    }

    let _ = pid;
    None
}

/// Loads quota configuration from workspace or state.
pub fn load_quota(workspace: &Path) -> Result<ResourceQuota> {
    let path = workspace.join(".antos").join("quota.toml");
    if path.exists() {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read quota config from {}", path.display()))?;
        let quota: ResourceQuota = toml::from_str(&content)
            .with_context(|| "failed to parse quota.toml")?;
        Ok(quota)
    } else {
        Ok(ResourceQuota::default())
    }
}

/// Saves quota configuration to workspace atomically.
pub fn save_quota(workspace: &Path, quota: &ResourceQuota) -> Result<()> {
    let dir = workspace.join(".antos");
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create directory {}", dir.display()))?;

    let path = dir.join("quota.toml");
    let content = toml::to_string_pretty(quota)
        .context("failed to serialize quota configuration")?;

    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_defaults() {
        let q = ResourceQuota::default();
        assert_eq!(q.timeout_secs, DEFAULT_TIMEOUT_SECS);
        assert_eq!(q.max_memory_mb, DEFAULT_MEMORY_MB);
        assert_eq!(q.cpu_quota_percent, DEFAULT_CPU_PERCENT);
        assert_eq!(q.max_pids, DEFAULT_MAX_PIDS);
    }

    #[test]
    fn test_save_and_load_quota() {
        let temp_dir = std::env::temp_dir().join(format!("antos-quota-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("create tempdir");

        let mut custom = ResourceQuota::default();
        custom.timeout_secs = 45;
        custom.max_memory_mb = 512;

        save_quota(&temp_dir, &custom).expect("save");
        let loaded = load_quota(&temp_dir).expect("load");

        assert_eq!(loaded.timeout_secs, 45);
        assert_eq!(loaded.max_memory_mb, 512);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_watchdog_timeout_enforcement() {
        let quota = ResourceQuota {
            timeout_secs: 1,
            max_memory_mb: 2048,
            cpu_quota_percent: 100,
            max_pids: 32,
        };
        let watchdog = ProcessWatchdog::new(quota);

        // Spawn a process that sleeps for 5 seconds
        let child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("spawn sleep");

        let result = watchdog.supervise(child);
        assert!(result.is_err(), "debe fallar por timeout de 1 segundo");
        let err_str = result.err().unwrap().to_string();
        assert!(err_str.contains("tiempo de ejecución agotado"), "mensaje de error: {err_str}");
    }
}
