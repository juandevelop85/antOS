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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbpfStatus {
    pub available: bool,
    pub lsm_enabled: bool,
    pub active_probes: Vec<String>,
    pub total_events_captured: usize,
    pub total_violations_blocked: usize,
    pub ring_buffer_capacity: usize,
    pub ring_buffer_utilization: usize,
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

/// Operating system installation configuration (T15.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallConfig {
    pub target_device: String,
    pub clean_install: bool,
    pub target_mount: String,
    pub hostname: String,
    pub username: String,
    pub timezone: String,
    pub dry_run: bool,
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



// ------------------------------------------------------------- packages (antpkg)

/// Declarative package recipe specification (T16.2).
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
}

/// Summary of an installed package in the immutable store (T16.2).
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
}

/// Installation or compilation report for an immutable package (T16.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageInstallReport {
    pub name: String,
    pub version: String,
    pub store_path: String,
    pub generation: u64,
    pub binaries_linked: Vec<String>,
    pub checksum_verified: bool,
    pub signature_verified: bool,
    pub success: bool,
    pub message: String,
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

