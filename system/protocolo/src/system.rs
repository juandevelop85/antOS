//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};

// ============================================================================
// antOS Core System Services, Storage, VFS, Packages and Desktop Environment
// ============================================================================

// ----------------------------------------------------------- notifications (T8.2)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    TaskFinished,
    ApprovalRequired,
    QAFailed,
    SecurityAlert,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationAction {
    Approve,
    Reject,
    ViewDiff,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationItem {
    pub id: String,
    pub ticket_id: String,
    pub title: String,
    pub body: String,
    pub kind: NotificationKind,
    pub created_at: u64,
    pub read: bool,
    pub actions: Vec<NotificationAction>,
}

// ----------------------------------------------------------- semantic vfs (T10.1)

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfsEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: usize,
    pub node_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfsStatus {
    pub mount_point: Option<String>,
    pub is_mounted: bool,
    pub total_symbols: usize,
    pub total_modules: usize,
}

// --------------------------------------------------- vfs interceptor guard (T10.2)

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxValidationError {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub language: String,
    pub errors: Vec<SyntaxValidationError>,
    pub line_count: usize,
    pub file_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfsGuardStatus {
    pub enabled: bool,
    pub total_intercepted: usize,
    pub total_rejected: usize,
    pub rejected_paths: Vec<String>,
}

// --------------------------------------------------- supervisor kernel ebpf (T11.1)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EbpfHookKind {
    BprmCheckSecurity,
    FileOpen,
    SocketConnect,
    SyscallTrace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EbpfSecurityAction {
    Allowed,
    Blocked,
    Audited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbpfSecurityEvent {
    pub id: String,
    pub timestamp_ms: u64,
    pub pid: u32,
    pub comm: String,
    pub hook: EbpfHookKind,
    pub target_resource: String,
    pub action_taken: EbpfSecurityAction,
    pub violation_reason: Option<String>,
}

/// Backend real que produce los [`EbpfSecurityEvent`] del ring buffer
/// (T31.14). El panel de telemetría del sentinela existe y funciona hoy en
/// `Simulated`: el daemon nunca ha cargado un programa eBPF, así que
/// mostrarlo sin distinción de `LinuxBpf` (todavía no implementado en
/// ningún lugar del árbol) haría pasar auditoría de espacio de usuario por
/// vigilancia real del kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EbpfBackend {
    /// Ring buffer en memoria del proceso `antosd`, alimentado solo por
    /// llamadas explícitas (`record_event`/`simulate_violation`); ningún
    /// hook del kernel está cargado.
    Simulated,
    /// Programa eBPF real adjunto vía LSM. No implementado todavía.
    LinuxBpf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbpfStatus {
    pub available: bool,
    /// Si el *kernel* del host anuncia el LSM `bpf` en
    /// `/sys/kernel/security/lsm`. Es una propiedad de la plataforma, no una
    /// prueba de que este módulo esté usándolo — con el `backend` de hoy
    /// (`Simulated`) es cierto incluso aunque `antosd` nunca cargue un
    /// programa BPF. Ver `backend` para lo que de verdad produce los
    /// eventos.
    pub lsm_enabled: bool,
    /// Backend que produce los eventos del ring buffer (T31.14).
    #[serde(default = "default_ebpf_backend")]
    pub backend: EbpfBackend,
    pub active_probes: Vec<String>,
    pub total_events_captured: usize,
    pub total_violations_blocked: usize,
    pub ring_buffer_capacity: usize,
    pub ring_buffer_utilization: usize,
}

fn default_ebpf_backend() -> EbpfBackend {
    EbpfBackend::Simulated
}

// --------------------------------------------- continuous runtime profiler (T11.2)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSuggestionKind {
    MemoryOptimization,
    CpuOptimization,
    IoOptimization,
    ConcurrencyOptimization,
}

/// Un supuesto punto caliente de una ejecución perfilada (T31.14).
///
/// **Siempre heurístico, nunca muestreado.** `name`,
/// `percentage_cpu`/`percentage_memory` y `calls_or_samples` los elige
/// `ProfilerEngine::synthesize_hotspots` por coincidencia de subcadena en el
/// comando (`"test"`, `"build"`/`"check"`, u otro) — una tabla fija de tres
/// o cuatro entradas por caso, la misma siempre para el mismo tipo de
/// comando. No hay ningún muestreador de pila, `perf`, ni instrumentación
/// real detrás; los números no varían con la duración, CPU o memoria de la
/// ejecución real que sí mide [`ProfileReport`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileHotspot {
    pub name: String,
    pub percentage_cpu: f32,
    pub percentage_memory: f32,
    pub calls_or_samples: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSuggestion {
    pub kind: ProfileSuggestionKind,
    pub title: String,
    pub description: String,
    pub potential_impact: String,
    pub target_symbol_or_path: Option<String>,
}

/// Reporte de una ejecución perfilada (T31.14).
///
/// `duration_ms`, `cpu_user_ms`, `cpu_sys_ms`, `peak_memory_bytes` y
/// `page_faults` son reales cuando `metrics_are_real` es `true`: se leen de
/// `getrusage(2)` para el proceso hijo realmente lanzado. Si esa lectura
/// falla (plataforma sin `libc::getrusage`, o la llamada devuelve error),
/// `metrics_are_real` queda en `false` y esos cinco campos son un cálculo
/// aproximado a partir solo de `duration_ms` — no una medición. `hotspots`
/// es siempre heurístico, con independencia de `metrics_are_real`: ver
/// [`ProfileHotspot`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileReport {
    pub id: String,
    pub command: String,
    pub duration_ms: u64,
    pub cpu_user_ms: u64,
    pub cpu_sys_ms: u64,
    pub peak_memory_bytes: u64,
    pub page_faults: u64,
    pub exit_code: i32,
    pub hotspots: Vec<ProfileHotspot>,
    pub suggestions: Vec<ProfileSuggestion>,
    /// Si las cinco métricas de arriba vienen de `getrusage(2)` real (`true`)
    /// o de una estimación de respaldo cuando esa lectura falló (`false`).
    #[serde(default = "default_profile_metrics_are_real")]
    pub metrics_are_real: bool,
}

fn default_profile_metrics_are_real() -> bool {
    // Los reportes persistidos antes de T31.14 no tenían este campo; el
    // comportamiento de esa época siempre intentaba `getrusage` primero,
    // así que asumir `true` para datos antiguos es la lectura más fiel.
    true
}

// ------------------------------------------------ Desktop Session (T13.0)

/// Global desktop hotkey in antOS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopHotkey {
    pub key: String,
    pub action: String,
    pub description: String,
}

/// Status of the Wayland graphical desktop environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopSessionStatus {
    pub running: bool,
    pub compositor_name: String,
    pub wayland_display: Option<String>,
    pub active_clients_count: usize,
    pub registered_hotkeys: Vec<DesktopHotkey>,
}

// ------------------------------------------------ Barra Telemetry (T13.1)

/// Visual alert or notification emitted to the desktop bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarraAlert {
    pub category: String,
    pub message: String,
    pub urgent: bool,
}

/// Real-time consolidated telemetry for the antOS desktop bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarraTelemetry {
    pub ebpf_lsm_active: bool,
    pub ebpf_violations_count: usize,
    pub profiler_rss_bytes: u64,
    pub profiler_cpu_percent: f32,
    pub active_pair_session: Option<String>,
    pub active_ghost_text_count: usize,
    pub mesh_peers_count: usize,
    pub active_notifications_count: usize,
}

// ------------------------------------------------ Boot Pipeline (T13.2)

/// Status of the bare-metal boot pipeline and QEMU emulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootPipelineStatus {
    pub kernel_elf_exists: bool,
    pub kernel_elf_size_bytes: u64,
    pub bios_image_exists: bool,
    pub bios_image_size_bytes: u64,
    pub qemu_installed: bool,
    pub target_arch: String,
}

// ------------------------------------------------ Visual QA & Screencopy (T14.2)

/// Specific finding detected during multimodal visual inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualFinding {
    pub category: String,
    pub severity: String,
    pub description: String,
    pub coordinates: Option<String>,
    pub recommendation: String,
}

/// Consolidated graphical interface audit report generated by VisualQA.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualQAReport {
    pub target: String,
    pub image_width: u32,
    pub image_height: u32,
    pub image_size_bytes: usize,
    pub findings: Vec<VisualFinding>,
    pub pass: bool,
    pub summary: String,
}

/// Result of a Wayland screencopy capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotResult {
    pub target: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub base64_data: String,
    pub size_bytes: usize,
    pub saved_path: Option<String>,
}

// ------------------------------------------------ Storage & Installer (T15.1)

/// Individual partition within a storage drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskPartition {
    pub number: u32,
    pub name: String,
    pub size_bytes: u64,
    pub fs_type: Option<String>,
    pub mountpoint: Option<String>,
    pub is_efi: bool,
    pub is_bootable: bool,
    pub uuid: Option<String>,
}

/// Physical or virtual block storage device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskDevice {
    pub path: String,
    pub model: String,
    pub size_bytes: u64,
    pub sector_size: u32,
    pub bus_type: String,
    pub partition_table: String,
    pub partitions: Vec<DiskPartition>,
    pub is_read_only: bool,
}

/// Proposed partitioning plan for installation to disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionPlan {
    pub target_device: String,
    pub clean_install: bool,
    pub efi_partition_bytes: u64,
    pub root_partition_bytes: u64,
    pub swap_partition_bytes: u64,
    pub aligned_start_sector: u64,
    pub warnings: Vec<String>,
}

/// Operating system installation configuration (T15.2 / T30.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallConfig {
    pub target_device: String,
    pub clean_install: bool,
    pub target_mount: String,
    pub hostname: String,
    pub username: String,
    pub timezone: String,
    /// Console keymap (`console.keyMap` in the generated NixOS config).
    #[serde(default = "default_keymap")]
    pub keymap: String,
    pub dry_run: bool,
}

fn default_keymap() -> String {
    "us".to_string()
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            target_device: "/dev/nvme0n1".into(),
            clean_install: false,
            target_mount: "/mnt/antos".into(),
            hostname: "antos-box".into(),
            username: "antos".into(),
            timezone: "UTC".into(),
            keymap: default_keymap(),
            dry_run: true,
        }
    }
}

/// Individual step in the installation pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallStep {
    pub name: String,
    pub description: String,
    pub completed: bool,
}

/// Final completion or simulation report for system deployment (T15.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallReport {
    pub target_device: String,
    pub mode: String,
    pub success: bool,
    pub steps: Vec<InstallStep>,
    pub efi_partition: String,
    pub root_partition: String,
    pub fstab_entries: Vec<String>,
    pub summary: String,
}

/// Detected operating system entry for dual-boot configurations (T15.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsEntry {
    pub name: String,
    pub os_type: String,
    pub efi_path: String,
    pub disk_device: String,
    pub partition_number: u32,
}

/// Configuration for UEFI bootloader deployment (T15.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootloaderConfig {
    pub esp_mount: String,
    pub target_device: String,
    pub efi_partition: u32,
    pub default_os: String,
    pub timeout_seconds: u32,
    pub detected_os: Vec<OsEntry>,
    pub dry_run: bool,
}

impl Default for BootloaderConfig {
    fn default() -> Self {
        Self {
            esp_mount: "/boot/efi".into(),
            target_device: "/dev/nvme0n1".into(),
            efi_partition: 1,
            default_os: "antos".into(),
            timeout_seconds: 5,
            detected_os: Vec::new(),
            dry_run: true,
        }
    }
}

/// Deployment and configuration report for UEFI bootloader (T15.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootloaderReport {
    pub success: bool,
    pub esp_path: String,
    pub efibootmgr_command: String,
    pub entries_configured: Vec<String>,
    pub loader_conf_content: String,
    pub summary: String,
}

/// Declarative Limine bootloader configuration and hybrid image parameters (T24.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimineBootConfig {
    pub timeout_seconds: u32,
    pub default_entry: u32,
    pub graphics: bool,
    pub title: String,
    pub protocol: String,
    pub kernel_path: String,
    pub resolution: String,
    pub comment: String,
}

impl Default for LimineBootConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 3,
            default_entry: 1,
            graphics: true,
            title: "/antOS (Desarrollo y Orquestación Multi-Agente)".into(),
            protocol: "limine".into(),
            kernel_path: "boot():/KERNEL.ELF".into(),
            resolution: "1280x720x32".into(),
            comment: "Sistema operativo antOS en modo bare-metal nativo".into(),
        }
    }
}

/// Deployment and verification report for hybrid UEFI/BIOS boot image (T24.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridBootImageReport {
    pub success: bool,
    pub image_path: String,
    pub architecture: String,
    pub format: String,
    pub efi_bootloader: String,
    pub pe_signature_valid: bool,
    pub limine_conf_generated: bool,
    pub esp_size_bytes: u64,
    pub summary: String,
}

/// Configuration for the Live Ramdisk (Initramfs) packaging and deployment (T24.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveRamdiskConfig {
    pub format: String,
    pub compression: String,
    pub target_path: String,
    pub include_tools: Vec<String>,
    pub include_configs: Vec<String>,
    pub mount_points: Vec<String>,
    pub auto_mount_rootfs: bool,
}

impl Default for LiveRamdiskConfig {
    fn default() -> Self {
        Self {
            format: "ustar".into(),
            compression: "none".into(),
            target_path: "boot():/initrd.img".into(),
            include_tools: vec![
                "/bin/antos".into(),
                "/bin/antosd".into(),
                "/bin/sh".into(),
                "/bin/parted".into(),
                "/bin/mkfs.ext4".into(),
                "/sbin/init".into(),
            ],
            include_configs: vec![
                "/etc/hostname".into(),
                "/etc/os-release".into(),
                "/etc/fstab".into(),
                "/etc/antos.conf".into(),
            ],
            mount_points: vec![
                "/dev".into(),
                "/proc".into(),
                "/sys".into(),
                "/mnt".into(),
                "/tmp".into(),
            ],
            auto_mount_rootfs: true,
        }
    }
}

/// Metadata manifest describing a packaged Live Ramdisk (T24.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveRamdiskManifest {
    pub hostname: String,
    pub os_name: String,
    pub version: String,
    pub total_entries: usize,
    pub total_size_bytes: usize,
    pub has_init: bool,
    pub tools_count: usize,
    pub summary: String,
}

/// Removable USB mass storage device representation for Live USB creation (T24.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbDeviceInfo {
    pub path: String,
    pub vendor: String,
    pub model: String,
    pub size_bytes: u64,
    pub bus_type: String,
    pub is_removable: bool,
    pub is_system_disk: bool,
    pub mount_points: Vec<String>,
}

/// Report of a Live USB ISO image build operation (T24.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbBuildReport {
    pub success: bool,
    pub iso_path: String,
    pub sha256_path: String,
    pub sha256_checksum: String,
    pub architecture: String,
    pub size_bytes: u64,
    pub summary: String,
}

/// Report of a USB flash operation with sector verification (T24.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsbFlashReport {
    pub success: bool,
    pub target_device: String,
    pub image_path: String,
    pub bytes_written: u64,
    pub sha256_checksum: String,
    pub duration_seconds: f64,
    pub average_speed_mbps: f64,
    pub verified: bool,
    pub summary: String,
}

// ------------------------------------------------------------- packages (antpkg)

/// Application type classification for an antOS package (T25.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PackageAppType {
    #[default]
    Cli,
    Gui,
}

/// Freedesktop XDG desktop entry specification metadata (T25.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopEntryManifest {
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub exec: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub mime_types: Vec<String>,
    #[serde(default)]
    pub terminal: bool,
    pub startup_wm_class: Option<String>,
}

/// Icon asset definition for desktop applications (T25.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IconAsset {
    pub resolution: String,
    pub format: String,
    pub path: String,
}

/// Declarative package recipe specification (T16.2 / T25.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub source_url: Option<String>,
    pub sha256: Option<String>,
    pub signature: Option<String>,
    pub signer_public_key: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub build_script: Option<String>,
    #[serde(default)]
    pub binaries: Vec<String>,
    #[serde(default)]
    pub app_type: PackageAppType,
    #[serde(default)]
    pub desktop_entry: Option<DesktopEntryManifest>,
    #[serde(default)]
    pub icons: Vec<IconAsset>,
}

/// Summary of an installed package in the immutable store (T16.2 / T25.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSummary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub store_hash: String,
    pub installed_size_bytes: u64,
    pub installed_at: String,
    pub binaries: Vec<String>,
    pub generation: u64,
    #[serde(default)]
    pub app_type: PackageAppType,
    #[serde(default)]
    pub desktop_entry: Option<DesktopEntryManifest>,
    #[serde(default)]
    pub desktop_file: Option<String>,
    #[serde(default)]
    pub icons_linked: Vec<String>,
}

/// Installation or compilation report for an immutable package (T16.2 / T25.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageInstallReport {
    pub name: String,
    pub version: String,
    pub store_path: String,
    pub generation: u64,
    pub binaries_linked: Vec<String>,
    #[serde(default)]
    pub desktop_entries_linked: Vec<String>,
    #[serde(default)]
    pub icons_linked: Vec<String>,
    pub checksum_verified: bool,
    pub signature_verified: bool,
    pub success: bool,
    pub message: String,
}

/// Summary of a registered desktop graphical application (T25.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopAppSummary {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub exec: String,
    pub icon: Option<String>,
    pub icon_path: Option<String>,
    pub categories: Vec<String>,
    pub mime_types: Vec<String>,
    pub desktop_file_path: String,
    pub package_name: String,
    pub package_version: String,
}

/// Verification report for Freedesktop .desktop files (T25.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopValidationReport {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Global status of the immutable package store and profile generations (T16.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageStoreStatus {
    pub store_path: String,
    pub current_profile_path: String,
    pub current_generation: u64,
    pub total_packages: usize,
    pub total_store_bytes: u64,
    pub generations_count: usize,
}

/// Snapshot of an immutable profile generation (T16.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageGeneration {
    pub generation: u64,
    pub timestamp: String,
    pub packages: Vec<String>,
    pub active: bool,
}

// ----------------------------------------------------------- autopilot (T16.3)

/// Configuration for continuous autonomous agent monitoring (T16.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotConfig {
    pub enabled: bool,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub watch_paths: Vec<String>,
    #[serde(default)]
    pub auto_merge: bool,
    #[serde(default = "default_target_branch")]
    pub target_branch: String,
}

fn default_poll_interval() -> u64 {
    5
}

fn default_target_branch() -> String {
    "master".to_string()
}

impl Default for AutopilotConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_interval_secs: default_poll_interval(),
            watch_paths: Vec::new(),
            auto_merge: false,
            target_branch: default_target_branch(),
        }
    }
}

/// Real-time health and telemetry of the Autopilot daemon (T16.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotStatus {
    pub active: bool,
    pub workspace_path: String,
    pub poll_interval_secs: u64,
    pub active_incidents_count: usize,
    pub resolved_incidents_count: usize,
    pub last_scan_timestamp: Option<String>,
}

/// Proposed automated fix generated by the antFlow agent roles (T16.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotFixProposal {
    pub incident_id: String,
    pub branch: String,
    pub title: String,
    pub diff: String,
    pub test_output: String,
    pub reviewed_by_auditor: bool,
}

/// Incident tracked by Autopilot when detecting broken builds or syntax flaws (T16.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotIncident {
    pub id: String,
    pub timestamp: String,
    pub incident_type: String,
    pub severity: String,
    pub file_path: String,
    pub error_message: String,
    pub status: String,
    pub worktree_branch: Option<String>,
    pub fix_proposal: Option<AutopilotFixProposal>,
}

// -------------------------------------------------------- web console (T16.4)

/// Configuration for the embedded real-time web console and WebSocket bridge (T16.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebConsoleConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_web_port")]
    pub port: u16,
    #[serde(default = "default_auth_required")]
    pub auth_required: bool,
    #[serde(default = "default_ws_ping_interval")]
    pub ws_ping_interval_secs: u64,
}

fn default_bind_addr() -> String {
    "127.0.0.1".to_string()
}

fn default_web_port() -> u16 {
    8088
}

fn default_auth_required() -> bool {
    true
}

fn default_ws_ping_interval() -> u64 {
    30
}

impl Default for WebConsoleConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            port: default_web_port(),
            auth_required: default_auth_required(),
            ws_ping_interval_secs: default_ws_ping_interval(),
        }
    }
}

/// Operational status and telemetry of the embedded web console server (T16.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebConsoleStatus {
    pub running: bool,
    pub bind_addr: String,
    pub port: u16,
    pub connected_clients: usize,
    pub active_sessions_count: usize,
    pub url: String,
}

/// Structured frame exchanged over the WebConsole WebSocket stream (T16.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSocketMessage {
    pub topic: String,
    pub payload: String,
    pub timestamp: u64,
}

/// Authenticated session token for remote web clients (T16.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebAuthSession {
    pub token: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub client_label: Option<String>,
}

// ----------------------------------------------------------- preemptive scheduler & task control (T23.2)

/// Operating state of a process in the antOS preemptive scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

/// Execution state of a thread inside a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

/// Information and inspection model for an active or terminated process (PCB snapshot).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u64,
    pub name: String,
    pub page_table_root: u64,
    pub state: ProcessState,
    pub threads: Vec<u64>,
    pub exit_code: Option<u64>,
    pub memory_bytes: usize,
}

/// Information and inspection model for a thread (TCB snapshot).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub tid: u64,
    pub pid: u64,
    pub state: ThreadState,
    pub priority: u8,
    pub user_sp: u64,
    pub kernel_sp: u64,
    pub total_ticks: u64,
    pub time_slice_remaining: u32,
}

/// Telemetry and metrics for the preemptive scheduler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerStats {
    pub total_processes: usize,
    pub active_processes: usize,
    pub total_threads: usize,
    pub ready_threads: usize,
    pub context_switches: u64,
    pub quantum_ticks: u32,
    pub preemption_enabled: bool,
}

// ----------------------------------------------------------- graphics console & framebuffer (T23.3)

/// Pixel color format supported by the linear framebuffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FramebufferFormat {
    Rgb,
    Bgr,
    Grayscale,
    Unknown,
}

/// Metadata and dimensions of the kernel graphics framebuffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FramebufferInfoModel {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub bytes_per_pixel: usize,
    pub format: FramebufferFormat,
}

/// State and geometry of the kernel text console.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleInfoModel {
    pub cols: usize,
    pub rows: usize,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub font_width: usize,
    pub font_height: usize,
    pub ansi_enabled: bool,
}

// ----------------------------------------------------------- secondary storage & kernel vfs (T23.4)

/// Storage device transport or virtual interface type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageDeviceType {
    VirtioBlk,
    Ramdisk,
    Nvme,
    Ahci,
    Unknown,
}

/// Statistics and configuration of a storage block device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockDeviceStats {
    pub device_type: StorageDeviceType,
    pub capacity_sectors: u64,
    pub sector_size: usize,
    pub capacity_bytes: u64,
    pub pci_address: Option<String>,
}

/// Information about a filesystem mounted in the kernel VFS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelVfsMount {
    pub mount_point: String,
    pub fs_type: String,
    pub total_files: usize,
    pub read_only: bool,
}

/// Details of a specific node or entry inside the kernel VFS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelVfsNode {
    pub path: String,
    pub size: usize,
    pub is_dir: bool,
}

// ----------------------------------------------------------- syscall ABI & microkernel IPC (T23.5)

/// Metadata and metrics of the kernel system call interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyscallTableInfo {
    pub total_syscalls: u32,
    pub active_architecture: String,
    pub user_address_limit: u64,
    pub smap_efault_protection: bool,
}

/// Status and metrics of an active microkernel IPC channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcChannelInfo {
    pub channel_id: u64,
    pub pending_messages: usize,
    pub max_messages: usize,
    pub max_message_size: usize,
    pub waiting_receiver: bool,
}

/// Summary of an IPC message sent through a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcMessageSummary {
    pub channel_id: u64,
    pub sender_pid: u64,
    pub payload_size: usize,
}

// ----------------------------------------------------------- physical storage subsystem (T24.3)

/// Hardware interface type of a mass storage device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageInterfaceKind {
    VirtioBlk,
    AhciSata,
    Nvme,
}

/// Metadata and state describing a physical or virtual mass storage device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageDeviceInfo {
    pub device_node: String,
    pub interface: StorageInterfaceKind,
    pub model: String,
    pub sector_size: usize,
    pub total_sectors: u64,
    pub capacity_bytes: u64,
    pub read_only: bool,
}

/// Status and inventory of the kernel mass storage subsystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageSubsystemStatus {
    pub total_devices: usize,
    pub ahci_controllers_found: usize,
    pub nvme_controllers_found: usize,
    pub virtio_devices_found: usize,
    pub devices: Vec<StorageDeviceInfo>,
}

// ----------------------------------------------------------- application management subsystem (T25.2)

/// Origin or packaging system of an application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSource {
    Flatpak,
    NativePkg,
    Nix,
}

impl AppSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppSource::Flatpak => "flatpak",
            AppSource::NativePkg => "antpkg",
            AppSource::Nix => "nix",
        }
    }
}

/// Description of a desktop application managed by antOS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: AppSource,
    pub description: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub installed: bool,
    pub exec_cmd: String,
}

/// Remote search result for available applications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSearchResult {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: AppSource,
    pub description: String,
    pub installed: bool,
}

/// Progress report during application installation or downloading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppProgress {
    pub app_id: String,
    pub percentage: f32,
    pub status: String,
    pub done: bool,
}

/// Result of launching an application with desktop and workspace context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppLaunchResult {
    pub app_id: String,
    pub pid: Option<u32>,
    pub workspace: Option<String>,
    pub success: bool,
    pub message: String,
}

/// Result of an application action (install, uninstall, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppActionResult {
    pub app_id: String,
    pub action: String,
    pub success: bool,
    pub message: String,
}
