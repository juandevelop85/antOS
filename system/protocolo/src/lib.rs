//! The IPC contract between the antOS daemon and its clients.
//!
//! Originally lived inside `antosd` when the only client was its own terminal.
//! Extracted into a standalone crate as soon as a second client appeared—the
//! intention bar—because the alternative would be each having its own copy of
//! these types. A duplicated protocol is a diverging protocol.
//!
//! There is NO execution logic here: permission tiers are not decided, diffs
//! are not calculated, and nothing is validated. That belongs in the daemon by
//! design—a client capable of computing its own tier could simply choose it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ------------------------------------------------------------ tier and plan

/// The order of variants IS the scale: Auto < Confirm < Grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Auto,
    Confirm,
    Grant,
}

/// The base of the scale. The default value being the most permissive level
/// is safe precisely because derivation only elevates privileges upward.
impl Default for Tier {
    fn default() -> Self {
        Tier::Auto
    }
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Auto => "auto",
            Tier::Confirm => "confirm",
            Tier::Grant => "grant",
        }
    }

    pub fn label_es(self) -> &'static str {
        match self {
            Tier::Auto => "automático",
            Tier::Confirm => "confirmación",
            Tier::Grant => "concesión",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub capability: String,
    pub args: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub intent: String,
    pub planner: String,
    pub steps: Vec<Step>,
}

// ------------------------------------------------------------------- diff

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Line {
    Info(String),
    Add(String),
    Del(String),
}

// ----------------------------------------------------------- interactive diff (T8.1)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    HunkHeader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxTokenType {
    Keyword,
    Type,
    StringLit,
    Comment,
    Number,
    Added,
    Deleted,
    Normal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxToken {
    pub text: String,
    pub token_type: SyntaxTokenType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line_num: Option<usize>,
    pub new_line_num: Option<usize>,
    pub content: String,
    pub tokens: Vec<SyntaxToken>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFile {
    pub old_path: String,
    pub new_path: String,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
}

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

// ----------------------------------------------------------- p2p network / antMesh (T9.1)

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeResources {
    pub cpu_cores: usize,
    pub memory_mb: u64,
    pub vram_mb: Option<u64>,
    pub available_models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerNode {
    pub id: String,
    pub hostname: String,
    pub address: String,
    pub latency_ms: u64,
    pub connected: bool,
    pub resources: NodeResources,
    pub last_seen_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingToken {
    pub token: String,
    pub node_id: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshStatus {
    pub local_node: PeerNode,
    pub peers: Vec<PeerNode>,
}

// ----------------------------------------------------------- distributed swarm (T9.2)

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmTaskAssignment {
    pub task_id: String,
    pub ticket_id: String,
    pub role: AgentRole,
    pub assigned_node_id: String,
    pub target_model: Option<String>,
    pub worktree_branch: String,
    pub status: String,
    pub started_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmNodeStatus {
    pub node_id: String,
    pub hostname: String,
    pub address: String,
    pub is_local: bool,
    pub vram_available_mb: Option<u64>,
    pub cpu_cores: usize,
    pub running_tasks: Vec<SwarmTaskAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmStatus {
    pub nodes: Vec<SwarmNodeStatus>,
    pub total_tasks: usize,
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

// ---------------------------------------------------------------- LSP (T12.1)

/// Type of editor or developer client compatible with the antOS LSP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LspEditorKind {
    VsCode,
    Neovim,
    Helix,
    Emacs,
    Generic,
}

impl LspEditorKind {
    pub fn name(&self) -> &'static str {
        match self {
            LspEditorKind::VsCode => "VS Code",
            LspEditorKind::Neovim => "Neovim",
            LspEditorKind::Helix => "Helix",
            LspEditorKind::Emacs => "Emacs",
            LspEditorKind::Generic => "Generic LSP Client",
        }
    }

    pub fn config_filename(&self) -> &'static str {
        match self {
            LspEditorKind::VsCode => ".vscode/settings.json",
            LspEditorKind::Neovim => "init.lua / nvim-lspconfig",
            LspEditorKind::Helix => "~/.config/helix/languages.toml",
            LspEditorKind::Emacs => ".dir-locals.el / init.el",
            LspEditorKind::Generic => "lsp-client.json",
        }
    }
}

/// Status of the embedded antOS LSP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspServerStatus {
    pub running: bool,
    pub transport: String,
    pub socket_path: Option<String>,
    pub connected_clients: usize,
    pub active_workspace: String,
    pub indexed_symbols_count: usize,
    pub capabilities: Vec<String>,
}

// ---------------------------------------------------- Collab & DAP (T12.2)

/// Virtual cursor position in a collaborative co-editing session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollabCursor {
    pub client_id: String,
    pub line: usize,
    pub character: usize,
    pub ghost_text: Option<String>,
}

/// Status of a real-time collaborative editing session (CRDT).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollabSessionStatus {
    pub session_id: String,
    pub file_path: String,
    pub collaborators: Vec<String>,
    pub cursors: Vec<CollabCursor>,
    pub buffer_length: usize,
    pub active_ticket_id: Option<String>,
}

/// Breakpoint in an isolated DAP debugging session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapBreakpoint {
    pub id: usize,
    pub file_path: String,
    pub line: usize,
    pub verified: bool,
}

/// Inspected variable during debugging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapVariable {
    pub name: String,
    pub value: String,
    pub type_name: String,
}

/// Status of a sandbox-isolated DAP debugging session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapSessionStatus {
    pub session_id: String,
    pub target_command: String,
    pub state: String,
    pub breakpoints: Vec<DapBreakpoint>,
    pub current_line: Option<usize>,
    pub call_stack: Vec<String>,
    pub variables: Vec<DapVariable>,
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

// ------------------------------------------------ WASM Plugins (T14.1)

/// Summary of an installed WebAssembly plugin in antOS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSummary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub wasm_size_bytes: u64,
}

/// Result of executing an action on a WASM plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginResult {
    pub plugin: String,
    pub action: String,
    pub output: String,
    pub fuel_consumed: u64,
    pub memory_allocated_bytes: usize,
    pub success: bool,
    pub error: Option<String>,
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

// ----------------------------------------------------------- microvms (T16.1)

/// Configuration to instantiate an ephemeral microVM with hardware isolation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrovmConfig {
    pub vm_id: String,
    pub vcpu_count: u8,
    pub memory_mb: u32,
    pub kernel_image: String,
    pub initrd_image: Option<String>,
    pub overlay_disk: Option<String>,
    pub vsock_port: u32,
    pub command: Option<String>,
}

impl Default for MicrovmConfig {
    fn default() -> Self {
        Self {
            vm_id: "vm-default".into(),
            vcpu_count: 2,
            memory_mb: 512,
            kernel_image: "/boot/antos-vmlinuz".into(),
            initrd_image: None,
            overlay_disk: None,
            vsock_port: 5252,
            command: None,
        }
    }
}

/// Health and support status of KVM / Cloud-Hypervisor hypervisor engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrovmStatus {
    pub kvm_available: bool,
    pub hypervisor_engine: String,
    pub active_vms_count: usize,
    pub total_memory_allocated_mb: u32,
    pub vsock_supported: bool,
    pub kernel_version: String,
}

/// Active or registered ephemeral microVM instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrovmInstance {
    pub id: String,
    pub pid: u32,
    pub vcpus: u8,
    pub memory_mb: u32,
    pub vsock_port: u32,
    pub status: String,
    pub created_at: String,
    pub command: Option<String>,
}

/// Result of executing a command inside an ephemeral microVM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrovmExecResult {
    pub vm_id: String,
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub success: bool,
}

// ---------------------------------------------------------------- proposal

/// What is presented to a user before touching anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub plan: Plan,
    #[serde(alias = "cambios")]
    pub changes: Vec<Line>,
    #[serde(alias = "radio")]
    pub blast_radius: BlastRadius,
    #[serde(alias = "nivel")]
    pub tier: Tier,
    #[serde(alias = "razones")]
    pub reasons: Vec<String>,
    #[serde(alias = "recinto")]
    pub enclosure: Enclosure,
    /// If `true`, nothing will be executed: it is a dry run / preview.
    #[serde(alias = "seco")]
    pub dry_run: bool,
}

/// The declared effects, resolved to readable paths.
///
/// Resolved in the daemon and not the client on purpose: a client
/// should not need filesystem access to show a plan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlastRadius {
    #[serde(alias = "escribe")]
    pub writes: Vec<String>,
    #[serde(alias = "borra")]
    pub deletes: Vec<String>,
    #[serde(alias = "lee")]
    pub reads: Vec<String>,
    #[serde(alias = "sistema")]
    pub system: Vec<String>,
    #[serde(alias = "red")]
    pub network: Vec<String>,
}

/// Execution sandbox enclosure parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enclosure {
    #[serde(alias = "motor")]
    pub engine: String,
    #[serde(alias = "garantiza")]
    pub guarantees: String,
}

/// Outcome of executing a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub ok: bool,
    #[serde(alias = "mensaje")]
    pub message: String,
    #[serde(alias = "instantanea")]
    pub snapshot: Option<String>,
}

/// Type aliases for backwards compatibility.
pub type Propuesta = Proposal;
pub type Radio = BlastRadius;
pub type Recinto = Enclosure;
pub type Resultado = ExecutionResult;

// ----------------------------------------------------- git introspection (T1.1)

/// Modification status of a tracked or untracked file in Git.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitFileStatus {
    Modified,
    Created,
    Deleted,
    Renamed,
    TypeChanged,
    Conflicted,
}

/// Granular change summary for a file inside the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileDiffSummary {
    #[serde(alias = "ruta")]
    pub path: String,
    #[serde(alias = "lineas_anadidas")]
    pub added_lines: usize,
    #[serde(alias = "lineas_borradas")]
    pub deleted_lines: usize,
    #[serde(alias = "estado")]
    pub status: GitFileStatus,
}

/// Global status of a Git repository in the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitRepoStatus {
    /// Current active branch (e.g. `main`, `feature/x`) or `None` if detached HEAD.
    #[serde(alias = "rama")]
    pub branch: Option<String>,
    /// Short or full commit hash of HEAD.
    pub head_commit: Option<String>,
    /// Number of local commits ahead of remote upstream.
    #[serde(alias = "delante")]
    pub ahead: usize,
    /// Number of local commits behind remote upstream.
    #[serde(alias = "detras")]
    pub behind: usize,
    /// Files with unstaged modifications in working tree.
    #[serde(alias = "modificados")]
    pub modified: Vec<GitFileDiffSummary>,
    /// Files staged in the index.
    pub staged: Vec<GitFileDiffSummary>,
    /// Untracked files in the repository.
    #[serde(alias = "sin_seguimiento")]
    pub untracked: Vec<String>,
    /// Indicates if both working tree and index are clean.
    #[serde(alias = "limpio")]
    pub clean: bool,
}

impl GitRepoStatus {
    pub fn is_clean(&self) -> bool {
        self.modified.is_empty() && self.staged.is_empty() && self.untracked.is_empty()
    }

    /// Helper for backwards compatibility.
    pub fn es_limpio(&self) -> bool {
        self.is_clean()
    }
}

// ------------------------------------------------- spec engine and tickets (T1.3)

/// Status of a specification or development ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    #[serde(alias = "Pendiente")]
    Pending,
    #[serde(alias = "EnProgreso")]
    InProgress,
    #[serde(alias = "EnRevision")]
    InReview,
    #[serde(alias = "Completado")]
    Completed,
}

#[allow(non_upper_case_globals)]
impl TicketStatus {
    pub const Pendiente: Self = Self::Pending;
    pub const EnProgreso: Self = Self::InProgress;
    pub const EnRevision: Self = Self::InReview;
    pub const Completado: Self = Self::Completed;

    pub fn label(&self) -> &'static str {
        match self {
            TicketStatus::Pending => "Pending",
            TicketStatus::InProgress => "In Progress",
            TicketStatus::InReview => "In Review",
            TicketStatus::Completed => "Completed",
        }
    }

    pub fn tag(&self) -> &'static str {
        match self {
            TicketStatus::Pending => "⏳ Pending",
            TicketStatus::InProgress => "🔄 In Progress",
            TicketStatus::InReview => "🔍 In Review",
            TicketStatus::Completed => "✅ Completed",
        }
    }

    pub fn etiqueta(&self) -> &'static str {
        match self {
            TicketStatus::Pending => "⏳ Pendiente",
            TicketStatus::InProgress => "🔄 En Progreso",
            TicketStatus::InReview => "🔍 En Revisión",
            TicketStatus::Completed => "✅ Completado",
        }
    }
}

/// Summary of a ticket for listings and Kanban boards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketSummary {
    pub id: String,
    #[serde(alias = "fase")]
    pub phase: String,
    #[serde(alias = "titulo")]
    pub title: String,
    #[serde(alias = "estado")]
    pub status: TicketStatus,
    #[serde(alias = "ruta_archivo")]
    pub file_path: String,
}

/// Full details of a ticket parsed from Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketDetail {
    pub id: String,
    #[serde(alias = "fase")]
    pub phase: String,
    #[serde(alias = "titulo")]
    pub title: String,
    #[serde(alias = "estado")]
    pub status: TicketStatus,
    #[serde(alias = "ruta_archivo")]
    pub file_path: String,
    #[serde(alias = "descripcion")]
    pub description: String,
    #[serde(alias = "alcance_tecnico")]
    pub technical_scope: Vec<String>,
    #[serde(alias = "criterios_aceptacion")]
    pub acceptance_criteria: Vec<String>,
}

/// Diagnostic info for a listening TCP port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortDiagnosticInfo {
    pub port: u16,
    pub pid: u32,
    pub process_name: String,
    pub command: String,
    pub working_dir: Option<String>,
}

// ---------------------------------------------------- antFlow: multi-agent (T3.1)

/// Specialized agent role within the antFlow lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    #[serde(alias = "Arquitecto")]
    Architect,
    Coder,
    QA,
    Auditor,
    VisualQA,
}

#[allow(non_upper_case_globals)]
impl AgentRole {
    pub const Arquitecto: Self = Self::Architect;

    pub fn name(&self) -> &'static str {
        match self {
            AgentRole::Architect => "Architect",
            AgentRole::Coder => "Coder",
            AgentRole::QA => "QA / Tester",
            AgentRole::Auditor => "Security Auditor",
            AgentRole::VisualQA => "Visual QA",
        }
    }

    pub fn nombre(&self) -> &'static str {
        match self {
            AgentRole::Architect => "Arquitecto",
            AgentRole::Coder => "Coder",
            AgentRole::QA => "QA / Tester",
            AgentRole::Auditor => "Auditor de Seguridad",
            AgentRole::VisualQA => "QA Visual",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            AgentRole::Architect => {
                "Technical planning, ticket breakdown and architecture design."
            }
            AgentRole::Coder => {
                "Modular implementation of changes and refactoring in the worktree."
            }
            AgentRole::QA => "Automated test suite generation and execution in sandbox.",
            AgentRole::Auditor => "Review of diffs, security, style and blast radius.",
            AgentRole::VisualQA => {
                "Multimodal inspection of GUI windows, screenshots and visual regression testing."
            }
        }
    }

    pub fn system_prompt(&self) -> &'static str {
        match self {
            AgentRole::Architect => {
                "You are the Architect Agent of antOS. Your goal is to break down technical tickets \
                 into atomic steps, validate dependencies, and design the architecture adhering \
                 to crate boundaries and zero unwraps in production."
            }
            AgentRole::Coder => {
                "You are the Coder Agent of antOS. Your goal is to implement changes in files \
                 within the assigned ephemeral worktree, maintaining robustness, idiomatic \
                 error handling, and project conventions."
            }
            AgentRole::QA => {
                "You are the QA Agent of antOS. Your goal is to build and run test suites \
                 inside the confined sandbox, detecting failures or regressions and reporting \
                 detailed error output for correction."
            }
            AgentRole::Auditor => {
                "You are the Auditor Agent of antOS. Your goal is to audit generated diffs, \
                 verify that the blast radius does not exceed limits, and ensure all \
                 acceptance criteria are met before merging."
            }
            AgentRole::VisualQA => {
                "You are the Visual QA Agent of antOS. Your goal is to inspect user interface \
                 screenshots, verify layout fidelity, color contrast, typography alignment, \
                 and detect visual glitches or errors in Wayland graphical applications."
            }
        }
    }

    #[deprecated(note = "use description")]
    pub fn descripcion(&self) -> &'static str {
        self.description()
    }

    #[deprecated(note = "use system_prompt")]
    pub fn prompt_sistema(&self) -> &'static str {
        self.system_prompt()
    }
}

/// Lifecycle state machine for an antFlow task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowState {
    #[serde(alias = "Pendiente")]
    Pending,
    #[serde(alias = "Planificando")]
    Planning,
    #[serde(alias = "Implementando")]
    Implementing,
    #[serde(alias = "VerificandoTests")]
    Testing,
    #[serde(alias = "RevisionAuditor")]
    Reviewing,
    #[serde(alias = "ListoParaAprobacion")]
    ReadyForApproval,
    #[serde(alias = "Fusionado")]
    Merged,
    #[serde(alias = "Fallido")]
    Failed,
}

#[allow(non_upper_case_globals)]
impl FlowState {
    pub const Pendiente: Self = Self::Pending;
    pub const Planificando: Self = Self::Planning;
    pub const Implementando: Self = Self::Implementing;
    pub const VerificandoTests: Self = Self::Testing;
    pub const RevisionAuditor: Self = Self::Reviewing;
    pub const ListoParaAprobacion: Self = Self::ReadyForApproval;
    pub const Fusionado: Self = Self::Merged;
    pub const Fallido: Self = Self::Failed;
    pub fn label(&self) -> &'static str {
        match self {
            FlowState::Pending => "Pending",
            FlowState::Planning => "Planning (Architect)",
            FlowState::Implementing => "Implementing (Coder)",
            FlowState::Testing => "Running Tests (QA)",
            FlowState::Reviewing => "Reviewing (Auditor)",
            FlowState::ReadyForApproval => "Ready for Approval",
            FlowState::Merged => "Merged",
            FlowState::Failed => "Failed",
        }
    }

    pub fn tag(&self) -> &'static str {
        match self {
            FlowState::Pending => "⏳ Pending",
            FlowState::Planning => "📐 Planning (Architect)",
            FlowState::Implementing => "💻 Implementing (Coder)",
            FlowState::Testing => "🧪 Testing (QA)",
            FlowState::Reviewing => "🛡️ Reviewing (Auditor)",
            FlowState::ReadyForApproval => "✨ Ready for Approval",
            FlowState::Merged => "✅ Merged",
            FlowState::Failed => "❌ Failed",
        }
    }

    pub fn active_role(&self) -> Option<AgentRole> {
        match self {
            FlowState::Planning => Some(AgentRole::Architect),
            FlowState::Implementing => Some(AgentRole::Coder),
            FlowState::Testing => Some(AgentRole::QA),
            FlowState::Reviewing => Some(AgentRole::Auditor),
            _ => None,
        }
    }
}

/// Record of a lifecycle state transition in antFlow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowTransition {
    #[serde(alias = "timestamp_segundos")]
    pub timestamp_seconds: u64,
    #[serde(alias = "estado_anterior")]
    pub old_state: FlowState,
    #[serde(alias = "estado_nuevo")]
    pub new_state: FlowState,
    #[serde(alias = "rol")]
    pub role: Option<AgentRole>,
    #[serde(alias = "detalle")]
    pub detail: String,
}

/// Active or historical task orchestrated by antFlow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowTask {
    pub id: String,
    pub ticket_id: String,
    #[serde(alias = "estado")]
    pub state: FlowState,
    #[serde(alias = "rol_actual")]
    pub current_role: Option<AgentRole>,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    #[serde(alias = "reintentos_qa")]
    pub qa_retries: u32,
    #[serde(alias = "max_reintentos_qa")]
    pub max_qa_retries: u32,
    pub diff_preview: Option<String>,
    #[serde(alias = "resumen_auditoria")]
    pub audit_summary: Option<String>,
    #[serde(alias = "historial")]
    pub history: Vec<FlowTransition>,
}

// ---------------------------------------------------------------- mensajes

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// Intent to plan and execute a natural language task.
    #[serde(alias = "Intencion")]
    Intent {
        #[serde(alias = "texto")]
        text: String,
        #[serde(alias = "planificador")]
        planner: Option<String>,
        #[serde(alias = "seco")]
        dry_run: bool,
    },
    /// Response to a proposal (approve or discard).
    #[serde(alias = "Aprobacion")]
    Approval(bool),
    /// Query Git repository status for a workspace path.
    #[serde(alias = "ConsultarEstadoGit")]
    QueryGitStatus { workspace_path: String },
    /// List all available tickets in the workspace.
    #[serde(alias = "ListarTickets")]
    ListTickets { workspace_path: String },
    /// Get details of a specific ticket in the workspace.
    #[serde(alias = "ObtenerTicket")]
    GetTicket {
        workspace_path: String,
        ticket_id: String,
    },
    /// Diagnose listening TCP ports and associated processes.
    #[serde(alias = "DiagnosticarPuertos")]
    DiagnosePorts { port: Option<u16> },
    /// Start multi-agent antFlow execution for a ticket.
    #[serde(alias = "IniciarFlow")]
    StartFlow {
        workspace_path: String,
        ticket_id: String,
    },
    /// Query status of an antFlow task for a ticket.
    #[serde(alias = "ConsultarFlow")]
    QueryFlow { ticket_id: String },
    /// List all active agent flow tasks.
    #[serde(alias = "ListarFlows")]
    ListFlows { workspace_path: String },
    /// Approve or reject final changes of an antFlow task.
    #[serde(alias = "AprobarFlow")]
    ApproveFlow { ticket_id: String, decision: bool },
    /// Query structured syntax diff for ticket, file, or commit (T8.1).
    #[serde(alias = "ConsultarDiff")]
    QueryDiff {
        workspace_path: String,
        target: Option<String>,
    },
    /// List pending agent and system notifications (T8.2).
    #[serde(alias = "ListarNotificaciones")]
    ListNotifications { workspace_path: String },
    /// Execute action on a notification (approval, rejection, dismissal) (T8.2).
    #[serde(alias = "AccionNotificacion")]
    HandleNotificationAction {
        workspace_path: String,
        notification_id: String,
        action: NotificationAction,
    },
    /// Query P2P antMesh network status and known peers (T9.1).
    #[serde(alias = "ConsultarMesh")]
    QueryMesh { workspace_path: String },
    /// Connect to a peer node by IP/port or multiaddr (T9.1).
    #[serde(alias = "ConectarPeer")]
    ConnectPeer {
        workspace_path: String,
        address: String,
    },
    /// Generate a secure pairing token with expiration (T9.1).
    #[serde(alias = "GenerarTokenEmparejamiento")]
    GeneratePairingToken { workspace_path: String },
    /// Query distributed task distribution status of the multi-agent Swarm (T9.2).
    #[serde(alias = "ConsultarSwarm")]
    QuerySwarm { workspace_path: String },
    /// Dispatch a specific role for a ticket to a remote or auto-selected node (T9.2).
    #[serde(alias = "DespacharRolRemoto")]
    DispatchRemoteRole {
        workspace_path: String,
        ticket_id: String,
        role: AgentRole,
        node_id: Option<String>,
    },
    /// Query a virtual path or list a directory in /antfs (T10.1).
    #[serde(alias = "ConsultarVfs")]
    QueryVfs {
        workspace_path: String,
        virtual_path: String,
    },
    /// Mount virtual projection of /antfs on mount point (T10.1).
    #[serde(alias = "MontarVfs")]
    MountVfs {
        workspace_path: String,
        mount_point: Option<String>,
    },
    /// Unmount virtual projection of /antfs (T10.1).
    #[serde(alias = "DesmontarVfs")]
    UnmountVfs {
        workspace_path: String,
        mount_point: Option<String>,
    },
    /// Intercept and validate code buffer syntax before persisting (T10.2).
    #[serde(alias = "ValidarEscrituraVfs")]
    ValidateVfsWrite { file_path: String, content: String },
    /// Query status of semantic write guard interceptor (T10.2).
    #[serde(alias = "ConsultarGuardVfs")]
    QueryVfsGuard { workspace_path: String },
    /// Query status and compatibility of kernel eBPF LSM probes (T11.1).
    #[serde(alias = "ConsultarEbpfStatus")]
    QueryEbpfStatus { workspace_path: String },
    /// Query syscall and security audit log from eBPF (T11.1).
    #[serde(alias = "ConsultarEbpfAuditLog")]
    QueryEbpfAuditLog {
        workspace_path: String,
        limit: usize,
    },
    /// Simulate sandbox escape attempt to verify detection (T11.1).
    #[serde(alias = "SimularViolacionEbpf")]
    SimulateEbpfViolation {
        workspace_path: String,
        hook: EbpfHookKind,
        target_resource: String,
    },
    /// Execute command under continuous performance profiler (T11.2).
    #[serde(alias = "EjecutarProfiler")]
    RunProfiler {
        workspace_path: String,
        command: String,
    },
    /// Query historical profiler reports (T11.2).
    #[serde(alias = "ConsultarProfilerReportes")]
    QueryProfilerReports {
        workspace_path: String,
        limit: usize,
    },
    /// Analyze hot spots and generate recommendations for agents (T11.2).
    #[serde(alias = "AnalizarProfilerHotspots")]
    AnalyzeProfilerHotspots { workspace_path: String },
    /// Query status and capabilities of embedded LSP server (T12.1).
    #[serde(alias = "ConsultarLspStatus")]
    QueryLspStatus { workspace_path: String },
    /// Generate editor configuration to connect to antOS LSP (T12.1).
    #[serde(alias = "ObtenerLspConfig")]
    GetLspConfig {
        editor: LspEditorKind,
        workspace_path: String,
    },
    /// Start interactive pair programming session with Coder agent (T12.2).
    #[serde(alias = "IniciarCollabSession")]
    StartCollabSession {
        file_path: String,
        ticket_id: Option<String>,
        workspace_path: String,
    },
    /// Query status and cursors of active co-editing session (T12.2).
    #[serde(alias = "ConsultarCollabStatus")]
    QueryCollabStatus {
        session_id: String,
        workspace_path: String,
    },
    /// Start isolated debug session under DAP protocol in sandbox (T12.2).
    #[serde(alias = "IniciarDapSession")]
    StartDapSession {
        command: String,
        workspace_path: String,
    },
    /// Query state, variables, and call stack of DAP session (T12.2).
    #[serde(alias = "ConsultarDapStatus")]
    QueryDapStatus {
        session_id: String,
        workspace_path: String,
    },
    /// Query compositor and desktop environment status (T13.0).
    #[serde(alias = "ConsultarDesktopStatus")]
    QueryDesktopStatus,
    /// List global hotkeys registered in desktop (T13.0).
    #[serde(alias = "ListarDesktopHotkeys")]
    ListDesktopHotkeys,
    /// Start Wayland desktop session (T13.0).
    #[serde(alias = "IniciarDesktopSession")]
    StartDesktopSession { nested: bool },
    /// Query consolidated background telemetry for status bar (T13.1).
    #[serde(alias = "ConsultarBarraTelemetry")]
    QueryBarraTelemetry,
    /// Emit an alert or visual update to status bar (T13.1).
    #[serde(alias = "EmitirBarraAlert")]
    EmitBarraAlert(BarraAlert),
    /// Query bare metal boot pipeline status (T13.2).
    #[serde(alias = "ConsultarBootStatus")]
    QueryBootStatus,
    /// Execute boot pipeline action (build, qemu, test) (T13.2).
    #[serde(alias = "EjecutarBootPipeline")]
    RunBootPipeline { action: String, headless: bool },
    /// List installed WebAssembly plugins (T14.1).
    #[serde(alias = "ListarPlugins")]
    ListPlugins,
    /// Execute action inside a WASM plugin (T14.1).
    #[serde(alias = "EjecutarPlugin")]
    RunPlugin {
        plugin_name: String,
        action: String,
        params: std::collections::BTreeMap<String, String>,
    },
    /// Install WASM plugin from directory or manifest (T14.1).
    #[serde(alias = "InstalarPlugin")]
    InstallPlugin { source_path: String },
    /// Capture Wayland screen or window (T14.2).
    #[serde(alias = "CapturarPantalla")]
    CaptureScreen {
        target: Option<String>,
        save_path: Option<String>,
    },
    /// Execute multimodal visual QA inspection (T14.2).
    #[serde(alias = "InspeccionarVisualQA")]
    InspectVisualQa {
        target: String,
        criteria: Vec<String>,
    },
    /// List detected storage devices (T15.1).
    #[serde(alias = "ListarDiscos")]
    ListDisks,
    /// Inspect specific storage device (T15.1).
    #[serde(alias = "InspeccionarDisco")]
    InspectDisk { device: String },
    /// Calculate or apply GPT partition scheme on disk (T15.1).
    #[serde(alias = "ParticionarDisco")]
    PartitionDisk {
        device: String,
        clean_install: bool,
        dry_run: bool,
    },
    /// Install antOS base system on disk (T15.2).
    #[serde(alias = "InstalarSistema")]
    InstallSystem(InstallConfig),
    /// Probe operating systems on EFI / disk (T15.3).
    #[serde(alias = "SondearSistemasOperativos")]
    ProbeOperatingSystems { esp_mount: Option<String> },
    /// Install and configure UEFI bootloader (T15.3).
    #[serde(alias = "InstalarBootloader")]
    InstallBootloader(BootloaderConfig),
    /// Spawn ephemeral microVM with hardware isolation (T16.1).
    #[serde(alias = "SpawnMicrovm")]
    SpawnMicrovm(MicrovmConfig),
    /// Execute command inside microVM via vsock (T16.1).
    #[serde(alias = "ExecMicrovm")]
    ExecMicrovm { vm_id: String, command: String },
    /// Destroy and release microVM resources (T16.1).
    #[serde(alias = "DestroyMicrovm")]
    DestroyMicrovm { vm_id: String },
    /// List active microVMs (T16.1).
    #[serde(alias = "ListMicrovms")]
    ListMicrovms,
    /// Query hypervisor status (T16.1).
    #[serde(alias = "QueryMicrovmStatus")]
    QueryMicrovmStatus,
}

/// Type aliases for backwards compatibility.
pub type Peticion = Request;

/// The IPC event stream emitted by the antOS daemon to connected clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// Initial notification when an intent planning process starts.
    #[serde(alias = "Inicio")]
    Start {
        #[serde(alias = "intencion")]
        intent: String,
        #[serde(alias = "planificador")]
        planner: String,
    },
    /// Informational note or execution milestone.
    #[serde(alias = "Nota")]
    Note(String),
    /// Proposed plan and blast radius for review before execution.
    #[serde(alias = "Propuesta")]
    Proposal(Box<Proposal>),
    /// Output line from command or tool execution.
    #[serde(alias = "Salida")]
    Output(String),
    /// Final execution outcome of a plan.
    #[serde(alias = "Resultado")]
    Result(ExecutionResult),
    /// Detailed status of the Git repository in the workspace.
    #[serde(alias = "EstadoGit")]
    GitStatus(GitRepoStatus),
    /// Emitted when queried path is not a valid Git repository.
    #[serde(alias = "NoEsRepoGit")]
    NotGitRepo,
    /// List of technical tickets parsed from the workspace.
    #[serde(alias = "ListaTickets")]
    TicketList(Vec<TicketSummary>),
    /// Full detail of a specific ticket.
    #[serde(alias = "DetalleTicket")]
    TicketDetail(Option<TicketDetail>),
    /// Diagnostic info for listening TCP ports and associated processes.
    #[serde(alias = "EstadoPuertos")]
    PortsStatus(Vec<PortDiagnosticInfo>),
    /// Detailed status of an antFlow agent workflow task.
    #[serde(alias = "EstadoFlow")]
    FlowStatus(Option<FlowTask>),
    /// Listing of all antFlow tasks.
    #[serde(alias = "ListaFlows")]
    FlowList(Vec<FlowTask>),
    /// Real-time state transition event in antFlow.
    #[serde(alias = "TransicionFlow")]
    FlowTransition {
        ticket_id: String,
        #[serde(alias = "estado_anterior")]
        old_state: FlowState,
        #[serde(alias = "estado_nuevo")]
        new_state: FlowState,
        #[serde(alias = "rol")]
        role: Option<AgentRole>,
        #[serde(alias = "detalle")]
        detail: String,
    },
    /// Structured and syntax-highlighted diffs (T8.1).
    #[serde(alias = "DiffEstructurado")]
    StructuredDiff(Vec<DiffFile>),
    /// Active system and agent notifications (T8.2).
    #[serde(alias = "ListaNotificaciones")]
    NotificationList(Vec<NotificationItem>),
    /// Result of an action performed on a notification (T8.2).
    #[serde(alias = "ResultadoNotificacion")]
    NotificationResult {
        id: String,
        success: bool,
        message: String,
    },
    /// Status of the P2P antMesh network and connected peers (T9.1).
    #[serde(alias = "EstadoMesh")]
    MeshStatus(MeshStatus),
    /// Pairing token generated for a new peer node (T9.1).
    #[serde(alias = "TokenEmparejamientoGenerado")]
    PairingTokenGenerated(PairingToken),
    /// Result of connecting to a remote peer node (T9.1).
    #[serde(alias = "ResultadoConexionPeer")]
    PeerConnectionResult {
        address: String,
        success: bool,
        message: String,
    },
    /// Status of the distributed swarm cluster (T9.2).
    #[serde(alias = "EstadoSwarm")]
    SwarmStatus(SwarmStatus),
    /// Result of dispatching an agent role to a swarm node (T9.2).
    #[serde(alias = "ResultadoDespachoSwarm")]
    SwarmDispatchResult {
        ticket_id: String,
        role: AgentRole,
        assigned_node_id: String,
        success: bool,
        message: String,
    },
    /// Virtual directory listing in /antfs (T10.1).
    #[serde(alias = "ListadoVfs")]
    VfsList {
        virtual_path: String,
        entries: Vec<VfsEntry>,
    },
    /// Virtual content of a semantic file or diff in /antfs (T10.1).
    #[serde(alias = "ContenidoVfs")]
    VfsContent {
        virtual_path: String,
        content: String,
    },
    /// Result of VFS mount, unmount, or query operations (T10.1).
    #[serde(alias = "ResultadoVfs")]
    VfsResult {
        action: String,
        success: bool,
        message: String,
    },
    /// Pre-commit semantic syntax validation result (T10.2).
    #[serde(alias = "ResultadoValidacionVfs")]
    VfsValidationResult(ValidationResult),
    /// Status of the semantic write guard interceptor (T10.2).
    #[serde(alias = "EstadoGuardVfs")]
    VfsGuardStatus(VfsGuardStatus),
    /// Status of the kernel eBPF LSM security supervisor (T11.1).
    #[serde(alias = "EstadoEbpf")]
    EbpfStatus(EbpfStatus),
    /// Audit log trace events from the eBPF ring buffer (T11.1).
    #[serde(alias = "AuditLogEbpf")]
    EbpfAuditLog(Vec<EbpfSecurityEvent>),
    /// Result of an eBPF management action or probe simulation (T11.1).
    #[serde(alias = "ResultadoEbpf")]
    EbpfResult {
        action: String,
        success: bool,
        message: String,
    },
    /// Detailed profiling execution report (T11.2).
    #[serde(alias = "ReporteProfiler")]
    ProfilerReport(ProfileReport),
    /// Historical list of profiling reports (T11.2).
    #[serde(alias = "ListaReportesProfiler")]
    ProfilerReportList(Vec<ProfileReport>),
    /// Profiler analysis with hotspots and optimization suggestions (T11.2).
    #[serde(alias = "AnalisisProfiler")]
    ProfilerAnalysis {
        hotspots: Vec<ProfileHotspot>,
        suggestions: Vec<ProfileSuggestion>,
    },
    /// Embedded LSP server status (T12.1).
    #[serde(alias = "EstadoLsp")]
    LspStatus(LspServerStatus),
    /// Editor configuration helper for connecting to the LSP server (T12.1).
    #[serde(alias = "ConfiguracionLsp")]
    LspConfiguration {
        editor: LspEditorKind,
        config_content: String,
        target_file: String,
    },
    /// Real-time collaborative editing session status (T12.2).
    #[serde(alias = "EstadoCollabSession")]
    CollabSessionStatus(CollabSessionStatus),
    /// Isolated DAP debugging session status (T12.2).
    #[serde(alias = "EstadoDapSession")]
    DapSessionStatus(DapSessionStatus),
    /// Result of a collaborative editing or CRDT sync operation (T12.2).
    #[serde(alias = "ResultadoCollab")]
    CollabResult {
        action: String,
        success: bool,
        message: String,
    },
    /// Result of a DAP debugger operation (T12.2).
    #[serde(alias = "ResultadoDap")]
    DapResult {
        action: String,
        success: bool,
        message: String,
    },
    /// Wayland graphical desktop session status (T13.0).
    #[serde(alias = "EstadoDesktop")]
    DesktopStatus(DesktopSessionStatus),
    /// Listing of registered desktop hotkeys (T13.0).
    #[serde(alias = "ListaDesktopHotkeys")]
    DesktopHotkeysList(Vec<DesktopHotkey>),
    /// Real-time desktop bar telemetry status (T13.1).
    #[serde(alias = "EstadoBarraTelemetry")]
    BarraTelemetryStatus(BarraTelemetry),
    /// Visual desktop bar alert or notification (T13.1).
    #[serde(alias = "AlertaBarra")]
    BarraAlert(BarraAlert),
    /// Bare-metal boot pipeline status (T13.2).
    #[serde(alias = "EstadoBoot")]
    BootStatus(BootPipelineStatus),
    /// Execution output of a boot pipeline action (T13.2).
    #[serde(alias = "ResultadoBoot")]
    BootResult {
        action: String,
        output: String,
        success: bool,
    },
    /// List of available WebAssembly plugins (T14.1).
    #[serde(alias = "ListaPlugins")]
    PluginList(Vec<PluginSummary>),
    /// Execution result from a WebAssembly plugin action (T14.1).
    #[serde(alias = "ResultadoPlugin")]
    PluginResult(PluginResult),
    /// Result of a Wayland screen capture (T14.2).
    #[serde(alias = "ResultadoCaptura")]
    ScreenshotResult(ScreenshotResult),
    /// Visual QA audit and regression report (T14.2).
    #[serde(alias = "ReporteVisualQA")]
    VisualQAReport(VisualQAReport),
    /// List of detected disk storage devices (T15.1).
    #[serde(alias = "ListaDiscos")]
    DiskList(Vec<DiskDevice>),
    /// Detailed inspection of a disk device (T15.1).
    #[serde(alias = "DetalleDisco")]
    DiskDetail(Option<DiskDevice>),
    /// Disk partitioning plan or simulation (T15.1).
    #[serde(alias = "PlanParticionamiento")]
    PartitionPlan(PartitionPlan),
    /// Base operating system deployment report (T15.2).
    #[serde(alias = "ReporteInstalacion")]
    InstallReport(InstallReport),
    /// Detected operating systems on disk for dual-boot (T15.3).
    #[serde(alias = "SistemasOperativosDetectados")]
    DetectedOperatingSystems(Vec<OsEntry>),
    /// UEFI bootloader installation and configuration report (T15.3).
    #[serde(alias = "ReporteBootloader")]
    BootloaderReport(BootloaderReport),
    /// Status and diagnostic of the microVM hypervisor (T16.1).
    #[serde(alias = "EstadoMicrovm")]
    MicrovmStatus(MicrovmStatus),
    /// Listing of active microVMs (T16.1).
    #[serde(alias = "ListaMicrovms")]
    MicrovmList(Vec<MicrovmInstance>),
    /// Result of executing a command inside a microVM (T16.1).
    #[serde(alias = "ResultadoMicrovm")]
    MicrovmResult(MicrovmExecResult),
    /// General error message.
    #[serde(alias = "Error")]
    Error(String),
}

/// Backwards compatibility type aliases.
pub type Evento = Event;
pub type Mensaje = Request;
pub type Respuesta = Event;
pub type Message = Request;
pub type Response = Event;

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_repo_status_serialization() {
        let status = GitRepoStatus {
            branch: Some("main".into()),
            head_commit: Some("a1b2c3d".into()),
            ahead: 2,
            behind: 0,
            modified: vec![GitFileDiffSummary {
                path: "system/protocolo/src/lib.rs".into(),
                added_lines: 45,
                deleted_lines: 2,
                status: GitFileStatus::Modified,
            }],
            staged: vec![GitFileDiffSummary {
                path: "Cargo.toml".into(),
                added_lines: 1,
                deleted_lines: 0,
                status: GitFileStatus::Created,
            }],
            untracked: vec!["scratch.txt".into()],
            clean: false,
        };

        let json = serde_json::to_string(&status).expect("should serialize to JSON");
        let deserialized: GitRepoStatus =
            serde_json::from_str(&json).expect("should deserialize from JSON");

        assert_eq!(status, deserialized);
        assert!(!deserialized.is_clean());
        assert!(!deserialized.es_limpio());
    }

    #[test]
    fn test_query_git_status_request_serialization() {
        let request = Request::QueryGitStatus {
            workspace_path: "/Users/dev/workspace".into(),
        };

        let json = serde_json::to_string(&request).expect("should serialize request");
        let deserialized: Request =
            serde_json::from_str(&json).expect("should deserialize request");

        assert_eq!(request, deserialized);
    }

    #[test]
    fn test_git_status_and_not_repo_response_serialization() {
        let resp_ok = Response::GitStatus(GitRepoStatus {
            branch: Some("feature/git-inspector".into()),
            clean: true,
            ..Default::default()
        });

        let json_ok = serde_json::to_string(&resp_ok).expect("should serialize GitStatus");
        let deserialized_ok: Event =
            serde_json::from_str(&json_ok).expect("should deserialize GitStatus");
        assert_eq!(resp_ok, deserialized_ok);

        let resp_no_repo = Response::NotGitRepo;
        let json_no_repo =
            serde_json::to_string(&resp_no_repo).expect("should serialize NotGitRepo");
        let deserialized_no_repo: Event =
            serde_json::from_str(&json_no_repo).expect("should deserialize NotGitRepo");
        assert_eq!(resp_no_repo, deserialized_no_repo);
    }

    #[test]
    fn test_existing_message_compatibility() {
        // Legacy format in Spanish supported via serde alias
        let legacy_json =
            r#"{"Intencion":{"texto":"compilar kernel","planificador":"reglas","seco":false}}"#;
        let des_legacy: Request = serde_json::from_str(legacy_json).expect("deserialize legacy");
        match des_legacy {
            Request::Intent {
                text,
                planner,
                dry_run,
            } => {
                assert_eq!(text, "compilar kernel");
                assert_eq!(planner, Some("reglas".into()));
                assert!(!dry_run);
            }
            _ => panic!("must be Intent"),
        }

        // Modern format in English
        let intent_request = Request::Intent {
            text: "compilar kernel".into(),
            planner: Some("reglas".into()),
            dry_run: false,
        };
        let json = serde_json::to_string(&intent_request).expect("serialize intent");
        let deserialized: Request = serde_json::from_str(&json).expect("deserialize intent");
        assert_eq!(intent_request, deserialized);

        let event_note = Event::Note("analizando dependencias".into());
        let json_note = serde_json::to_string(&event_note).expect("serialize note");
        let deserialized_note: Event =
            serde_json::from_str(&json_note).expect("deserialize note");
        assert_eq!(event_note, deserialized_note);

        // Legacy Spanish event deserialization check
        let legacy_ev_json = r#"{"Nota":"hola mundo"}"#;
        let des_ev: Event = serde_json::from_str(legacy_ev_json).expect("deserialize legacy note");
        assert_eq!(des_ev, Event::Note("hola mundo".into()));
    }

    #[test]
    fn test_tickets_protocol_serialization() {
        let ticket = TicketDetail {
            id: "T1.3".into(),
            phase: "Phase 1".into(),
            title: "Ticket parser and indexer".into(),
            status: TicketStatus::InProgress,
            file_path: "docs/tickets/T1.3-spec-engine-tickets-parser.md".into(),
            description: "Build ticket indexer".into(),
            technical_scope: vec!["Markdown parser".into(), "IPC messages".into()],
            acceptance_criteria: vec!["antos tickets command".into()],
        };

        let json = serde_json::to_string(&ticket).expect("serialize ticket");
        let deserialized: TicketDetail = serde_json::from_str(&json).expect("deserialize ticket");
        assert_eq!(ticket, deserialized);

        let list_request = Request::ListTickets {
            workspace_path: "/workspace".into(),
        };
        let json_request =
            serde_json::to_string(&list_request).expect("serialize list request");
        let des_request: Request =
            serde_json::from_str(&json_request).expect("deserialize list request");
        assert_eq!(list_request, des_request);

        let list_response = Event::TicketList(vec![TicketSummary {
            id: "T1.3".into(),
            phase: "Phase 1".into(),
            title: "Ticket parser and indexer".into(),
            status: TicketStatus::InProgress,
            file_path: "docs/tickets/T1.3-spec-engine-tickets-parser.md".into(),
        }]);
        let json_resp = serde_json::to_string(&list_response).expect("serialize ticket list");
        let des_resp: Event =
            serde_json::from_str(&json_resp).expect("deserialize ticket list");
        assert_eq!(list_response, des_resp);

        let port_info = PortDiagnosticInfo {
            port: 3000,
            pid: 12345,
            process_name: "node".into(),
            command: "node server.js".into(),
            working_dir: Some("/app".into()),
        };
        let json_port = serde_json::to_string(&port_info).expect("serialize port");
        let des_port: PortDiagnosticInfo =
            serde_json::from_str(&json_port).expect("deserialize port");
        assert_eq!(port_info, des_port);

        let task = FlowTask {
            id: "flow-1".into(),
            ticket_id: "T3.1".into(),
            state: FlowState::Planning,
            current_role: Some(AgentRole::Architect),
            worktree_path: Some("/state/worktrees/t3.1".into()),
            branch_name: Some("agent/t3.1".into()),
            qa_retries: 0,
            max_qa_retries: 3,
            diff_preview: Some("+ new flow module".into()),
            audit_summary: Some("architecture approved".into()),
            history: vec![FlowTransition {
                timestamp_seconds: 1700000000,
                old_state: FlowState::Pending,
                new_state: FlowState::Planning,
                role: Some(AgentRole::Architect),
                detail: "assigning task to architect".into(),
            }],
        };

        let json_task = serde_json::to_string(&task).expect("serialize task");
        let des_task: FlowTask = serde_json::from_str(&json_task).expect("deserialize task");
        assert_eq!(task, des_task);
    }

    #[test]
    fn test_structured_diff_serialization() {
        let diff_file = DiffFile {
            old_path: "src/main.rs".into(),
            new_path: "src/main.rs".into(),
            additions: 2,
            deletions: 1,
            hunks: vec![DiffHunk {
                header: "@@ -10,4 +10,5 @@".into(),
                old_start: 10,
                old_lines: 4,
                new_start: 10,
                new_lines: 5,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Context,
                        old_line_num: Some(10),
                        new_line_num: Some(10),
                        content: "fn main() {".into(),
                        tokens: vec![
                            SyntaxToken {
                                text: "fn".into(),
                                token_type: SyntaxTokenType::Keyword,
                            },
                            SyntaxToken {
                                text: " main() {".into(),
                                token_type: SyntaxTokenType::Normal,
                            },
                        ],
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        old_line_num: None,
                        new_line_num: Some(11),
                        content: "    println!(\"antOS\");".into(),
                        tokens: vec![
                            SyntaxToken {
                                text: "    println!".into(),
                                token_type: SyntaxTokenType::Keyword,
                            },
                            SyntaxToken {
                                text: "(\"antOS\");".into(),
                                token_type: SyntaxTokenType::StringLit,
                            },
                        ],
                    },
                ],
            }],
        };

        let req = Request::QueryDiff {
            workspace_path: "/ws".into(),
            target: Some("T8.1".into()),
        };
        let json_req = serde_json::to_string(&req).expect("serialize req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req");
        assert_eq!(req, des_req);

        let event = Event::StructuredDiff(vec![diff_file.clone()]);
        let json_ev = serde_json::to_string(&event).expect("serialize event");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize event");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_notifications_serialization() {
        let notif = NotificationItem {
            id: "notif-1".into(),
            ticket_id: "T8.2".into(),
            title: "Review required for T8.2".into(),
            body: "QA agent validated all tests successfully.".into(),
            kind: NotificationKind::ApprovalRequired,
            created_at: 1700000000,
            read: false,
            actions: vec![
                NotificationAction::Approve,
                NotificationAction::Reject,
                NotificationAction::ViewDiff,
            ],
        };

        let req = Request::HandleNotificationAction {
            workspace_path: "/ws".into(),
            notification_id: "notif-1".into(),
            action: NotificationAction::Approve,
        };
        let json_req = serde_json::to_string(&req).expect("serialize req notif");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req notif");
        assert_eq!(req, des_req);

        let event = Event::NotificationList(vec![notif.clone()]);
        let json_ev = serde_json::to_string(&event).expect("serialize event notif");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize event notif");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_antmesh_serialization() {
        let peer = PeerNode {
            id: "node-e4f812".into(),
            hostname: "workstation-gpu".into(),
            address: "192.168.1.50:9042".into(),
            latency_ms: 12,
            connected: true,
            resources: NodeResources {
                cpu_cores: 16,
                memory_mb: 65536,
                vram_mb: Some(24576),
                available_models: vec!["qwen2.5-coder:7b".into(), "deepseek-coder:33b".into()],
            },
            last_seen_secs: 1700000000,
        };

        let status = MeshStatus {
            local_node: peer.clone(),
            peers: vec![peer.clone()],
        };

        let req = Request::ConnectPeer {
            workspace_path: "/ws".into(),
            address: "192.168.1.50:9042".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize mesh req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize mesh req");
        assert_eq!(req, des_req);

        let event = Event::MeshStatus(status.clone());
        let json_ev = serde_json::to_string(&event).expect("serialize mesh event");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize mesh event");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_swarm_serialization() {
        let task = SwarmTaskAssignment {
            task_id: "task-001".into(),
            ticket_id: "T9.2".into(),
            role: AgentRole::Coder,
            assigned_node_id: "node-gpu-1".into(),
            target_model: Some("deepseek-coder:33b".into()),
            worktree_branch: "agent/T9.2/coder".into(),
            status: "Running".into(),
            started_at: 1700000000,
        };

        let node = SwarmNodeStatus {
            node_id: "node-gpu-1".into(),
            hostname: "cluster-rig-01".into(),
            address: "10.0.0.5:9042".into(),
            is_local: false,
            vram_available_mb: Some(49152),
            cpu_cores: 32,
            running_tasks: vec![task.clone()],
        };

        let swarm_status = SwarmStatus {
            nodes: vec![node],
            total_tasks: 1,
        };

        let req = Request::DispatchRemoteRole {
            workspace_path: "/ws".into(),
            ticket_id: "T9.2".into(),
            role: AgentRole::Coder,
            node_id: Some("node-gpu-1".into()),
        };
        let json_req = serde_json::to_string(&req).expect("serialize swarm req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize swarm req");
        assert_eq!(req, des_req);

        let event = Event::SwarmStatus(swarm_status.clone());
        let json_ev = serde_json::to_string(&event).expect("serialize swarm event");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize swarm event");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_vfs_serialization() {
        let entry = VfsEntry {
            path: "/antfs/symbols/structs/MeshStatus".into(),
            name: "MeshStatus".into(),
            is_dir: false,
            size: 420,
            node_type: "Symbol".into(),
        };

        let req = Request::QueryVfs {
            workspace_path: "/ws".into(),
            virtual_path: "/antfs/symbols".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize vfs req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize vfs req");
        assert_eq!(req, des_req);

        let event = Event::VfsList {
            virtual_path: "/antfs/symbols".into(),
            entries: vec![entry],
        };
        let json_ev = serde_json::to_string(&event).expect("serialize vfs event");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize vfs event");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_vfs_guard_serialization() {
        let err = SyntaxValidationError {
            line: 42,
            column: 15,
            message: "unclosed delimiter `{`".into(),
            severity: "error".into(),
        };
        let val_res = ValidationResult {
            is_valid: false,
            language: "rust".into(),
            errors: vec![err],
            line_count: 50,
            file_path: "src/main.rs".into(),
        };
        let guard_status = VfsGuardStatus {
            enabled: true,
            total_intercepted: 14,
            total_rejected: 2,
            rejected_paths: vec!["src/main.rs".into()],
        };

        let req = Request::ValidateVfsWrite {
            file_path: "src/lib.rs".into(),
            content: "fn main() {}".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize guard req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize guard req");
        assert_eq!(req, des_req);

        let ev1 = Event::VfsValidationResult(val_res);
        let json_ev1 = serde_json::to_string(&ev1).expect("serialize val res");
        let des_ev1: Event = serde_json::from_str(&json_ev1).expect("deserialize val res");
        assert_eq!(ev1, des_ev1);

        let ev2 = Event::VfsGuardStatus(guard_status);
        let json_ev2 = serde_json::to_string(&ev2).expect("serialize guard status");
        let des_ev2: Event = serde_json::from_str(&json_ev2).expect("deserialize guard status");
        assert_eq!(ev2, des_ev2);
    }

    #[test]
    fn test_ebpf_serialization() {
        let status = EbpfStatus {
            available: true,
            lsm_enabled: true,
            active_probes: vec![
                "bprm_check_security".into(),
                "file_open".into(),
                "socket_connect".into(),
            ],
            total_events_captured: 120,
            total_violations_blocked: 3,
            ring_buffer_capacity: 1024,
            ring_buffer_utilization: 45,
        };
        let event = EbpfSecurityEvent {
            id: "evt-001".into(),
            timestamp_ms: 1725280000000,
            pid: 12345,
            comm: "python3".into(),
            hook: EbpfHookKind::SocketConnect,
            target_resource: "192.168.1.100:4444".into(),
            action_taken: EbpfSecurityAction::Blocked,
            violation_reason: Some(
                "Out-of-blast-radius network egress attempt blocked by eBPF LSM".into(),
            ),
        };

        let req = Request::QueryEbpfStatus {
            workspace_path: "/workspace".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize ebpf req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize ebpf req");
        assert_eq!(req, des_req);

        let ev1 = Event::EbpfStatus(status);
        let json_ev1 = serde_json::to_string(&ev1).expect("serialize ebpf status");
        let des_ev1: Event = serde_json::from_str(&json_ev1).expect("deserialize ebpf status");
        assert_eq!(ev1, des_ev1);

        let ev2 = Event::EbpfAuditLog(vec![event]);
        let json_ev2 = serde_json::to_string(&ev2).expect("serialize ebpf log");
        let des_ev2: Event = serde_json::from_str(&json_ev2).expect("deserialize ebpf log");
        assert_eq!(ev2, des_ev2);
    }

    #[test]
    fn test_profiler_serialization() {
        let hotspot = ProfileHotspot {
            name: "calculate_embeddings".into(),
            percentage_cpu: 64.5,
            percentage_memory: 32.1,
            calls_or_samples: 150,
        };
        let suggestion = ProfileSuggestion {
            kind: ProfileSuggestionKind::CpuOptimization,
            title: "Avoid superfluous cloning in embeddings calculation".into(),
            description: "The buffer is cloned inside the calculation loop. Replace with reference passing (&[f32]).".into(),
            potential_impact: "High (-45% CPU)".into(),
            target_symbol_or_path: Some("system/antosd/src/memory.rs".into()),
        };
        let report = ProfileReport {
            id: "prof-001".into(),
            command: "cargo test".into(),
            duration_ms: 1250,
            cpu_user_ms: 820,
            cpu_sys_ms: 110,
            peak_memory_bytes: 48 * 1024 * 1024,
            page_faults: 340,
            exit_code: 0,
            hotspots: vec![hotspot],
            suggestions: vec![suggestion],
        };

        let req = Request::RunProfiler {
            workspace_path: "/ws".into(),
            command: "cargo test".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize profiler req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize profiler req");
        assert_eq!(req, des_req);

        let ev1 = Event::ProfilerReport(report);
        let json_ev1 = serde_json::to_string(&ev1).expect("serialize report");
        let des_ev1: Event = serde_json::from_str(&json_ev1).expect("deserialize report");
        assert_eq!(ev1, des_ev1);
    }

    #[test]
    fn test_lsp_serialization() {
        let status = LspServerStatus {
            running: true,
            transport: "stdio".into(),
            socket_path: None,
            connected_clients: 1,
            active_workspace: "/Users/juandevelop/Develop/antOS".into(),
            indexed_symbols_count: 350,
            capabilities: vec![
                "textDocument/completion".into(),
                "textDocument/definition".into(),
                "textDocument/hover".into(),
                "textDocument/references".into(),
            ],
        };

        let req = Request::QueryLspStatus {
            workspace_path: "/Users/juandevelop/Develop/antOS".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize lsp status req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize lsp status req");
        assert_eq!(req, des_req);

        let ev1 = Event::LspStatus(status);
        let json_ev1 = serde_json::to_string(&ev1).expect("serialize lsp status ev");
        let des_ev1: Event = serde_json::from_str(&json_ev1).expect("deserialize lsp status ev");
        assert_eq!(ev1, des_ev1);

        let ev2 = Event::LspConfiguration {
            editor: LspEditorKind::Neovim,
            config_content: "vim.lsp.start({ name = 'antos-lsp', cmd = {'antos', 'lsp'} })".into(),
            target_file: "init.lua".into(),
        };
        let json_ev2 = serde_json::to_string(&ev2).expect("serialize lsp config ev");
        let des_ev2: Event = serde_json::from_str(&json_ev2).expect("deserialize lsp config ev");
        assert_eq!(ev2, des_ev2);
    }

    #[test]
    fn test_collab_and_dap_serialization() {
        let cursor = CollabCursor {
            client_id: "agent-coder".into(),
            line: 42,
            character: 10,
            ghost_text: Some("fn optimize_pipeline() -> Result<()>".into()),
        };
        let collab_status = CollabSessionStatus {
            session_id: "collab-001".into(),
            file_path: "src/main.rs".into(),
            collaborators: vec!["developer".into(), "agent-coder".into()],
            cursors: vec![cursor],
            buffer_length: 1024,
            active_ticket_id: Some("T12.2".into()),
        };

        let req_collab = Request::StartCollabSession {
            file_path: "src/main.rs".into(),
            ticket_id: Some("T12.2".into()),
            workspace_path: "/workspace".into(),
        };
        let json_req = serde_json::to_string(&req_collab).expect("serialize collab req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize collab req");
        assert_eq!(req_collab, des_req);

        let ev_collab = Event::CollabSessionStatus(collab_status);
        let json_ev = serde_json::to_string(&ev_collab).expect("serialize collab ev");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize collab ev");
        assert_eq!(ev_collab, des_ev);

        let dap_status = DapSessionStatus {
            session_id: "dap-001".into(),
            target_command: "cargo test".into(),
            state: "paused".into(),
            breakpoints: vec![DapBreakpoint {
                id: 1,
                file_path: "src/main.rs".into(),
                line: 50,
                verified: true,
            }],
            current_line: Some(50),
            call_stack: vec!["main()".into(), "test_runner()".into()],
            variables: vec![DapVariable {
                name: "counter".into(),
                value: "42".into(),
                type_name: "usize".into(),
            }],
        };

        let ev_dap = Event::DapSessionStatus(dap_status);
        let json_dap = serde_json::to_string(&ev_dap).expect("serialize dap ev");
        let des_dap: Event = serde_json::from_str(&json_dap).expect("deserialize dap ev");
        assert_eq!(ev_dap, des_dap);
    }

    #[test]
    fn test_desktop_serialization() {
        let hotkeys = vec![
            DesktopHotkey {
                key: "Super+Space".into(),
                action: "toggle_intent_bar".into(),
                description: "Open or focus intent bar".into(),
            },
            DesktopHotkey {
                key: "Super+A".into(),
                action: "toggle_agent_center".into(),
                description: "Open Agent Control Center".into(),
            },
        ];

        let status = DesktopSessionStatus {
            running: true,
            compositor_name: "labwc".into(),
            wayland_display: Some("wayland-0".into()),
            active_clients_count: 3,
            registered_hotkeys: hotkeys.clone(),
        };

        let req_status = Request::QueryDesktopStatus;
        let json_req = serde_json::to_string(&req_status).expect("serialize desktop req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize desktop req");
        assert_eq!(req_status, des_req);

        let ev_status = Event::DesktopStatus(status);
        let json_ev = serde_json::to_string(&ev_status).expect("serialize desktop ev");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize desktop ev");
        assert_eq!(ev_status, des_ev);

        let ev_keys = Event::DesktopHotkeysList(hotkeys);
        let json_keys = serde_json::to_string(&ev_keys).expect("serialize keys ev");
        let des_keys: Event = serde_json::from_str(&json_keys).expect("deserialize keys ev");
        assert_eq!(ev_keys, des_keys);
    }

    #[test]
    fn test_barra_telemetry_serialization() {
        let telemetry = BarraTelemetry {
            ebpf_lsm_active: true,
            ebpf_violations_count: 1,
            profiler_rss_bytes: 45 * 1024 * 1024,
            profiler_cpu_percent: 3.4,
            active_pair_session: Some("pair-001".into()),
            active_ghost_text_count: 2,
            mesh_peers_count: 3,
            active_notifications_count: 4,
        };

        let req = Request::QueryBarraTelemetry;
        let json_req = serde_json::to_string(&req).expect("serialize barra req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize barra req");
        assert_eq!(req, des_req);

        let ev = Event::BarraTelemetryStatus(telemetry);
        let json_ev = serde_json::to_string(&ev).expect("serialize barra ev");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize barra ev");
        assert_eq!(ev, des_ev);

        let alert = BarraAlert {
            category: "ebpf".into(),
            message: "Access denied to /root/.ssh/id_rsa".into(),
            urgent: true,
        };
        let req_alert = Request::EmitBarraAlert(alert.clone());
        let json_alert = serde_json::to_string(&req_alert).expect("serialize alert req");
        let des_alert: Request = serde_json::from_str(&json_alert).expect("deserialize alert req");
        assert_eq!(req_alert, des_alert);
    }

    #[test]
    fn test_boot_pipeline_serialization() {
        let status = BootPipelineStatus {
            kernel_elf_exists: true,
            kernel_elf_size_bytes: 3314112,
            bios_image_exists: true,
            bios_image_size_bytes: 35651584,
            qemu_installed: true,
            target_arch: "x86_64-unknown-none".into(),
        };

        let req = Request::QueryBootStatus;
        let json_req = serde_json::to_string(&req).expect("serialize boot req");
        let des_req: Request = serde_json::from_str(&json_req).expect("deserialize boot req");
        assert_eq!(req, des_req);

        let req_exec = Request::RunBootPipeline {
            action: "build".into(),
            headless: true,
        };
        let json_exec = serde_json::to_string(&req_exec).expect("serialize boot exec");
        let des_exec: Request = serde_json::from_str(&json_exec).expect("deserialize boot exec");
        assert_eq!(req_exec, des_exec);

        let ev = Event::BootStatus(status);
        let json_ev = serde_json::to_string(&ev).expect("serialize boot ev");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize boot ev");
        assert_eq!(ev, des_ev);
    }

    #[test]
    fn test_wasm_plugins_serialization() {
        let summary = PluginSummary {
            name: "markdown-formatter".into(),
            version: "1.0.0".into(),
            description: "Formats Markdown tables and headings".into(),
            capabilities: vec!["format".into(), "lint".into()],
            wasm_size_bytes: 40960,
        };

        let ev_list = Event::PluginList(vec![summary]);
        let json_ev_list = serde_json::to_string(&ev_list).expect("serialize list ev");
        let des_ev_list: Event = serde_json::from_str(&json_ev_list).expect("deserialize list ev");
        assert_eq!(ev_list, des_ev_list);

        let req_list = Request::ListPlugins;
        let json_req_list = serde_json::to_string(&req_list).expect("serialize list plugins");
        let des_req_list: Request =
            serde_json::from_str(&json_req_list).expect("deserialize list plugins");
        assert_eq!(req_list, des_req_list);

        let mut params = std::collections::BTreeMap::new();
        params.insert("target".into(), "README.md".into());
        let req_run = Request::RunPlugin {
            plugin_name: "markdown-formatter".into(),
            action: "format".into(),
            params,
        };
        let json_run = serde_json::to_string(&req_run).expect("serialize run plugin");
        let des_run: Request = serde_json::from_str(&json_run).expect("deserialize run plugin");
        assert_eq!(req_run, des_run);

        let result = PluginResult {
            plugin: "markdown-formatter".into(),
            action: "format".into(),
            output: "Formatting successful".into(),
            fuel_consumed: 1250,
            memory_allocated_bytes: 65536,
            success: true,
            error: None,
        };
        let ev = Event::PluginResult(result);
        let json_ev = serde_json::to_string(&ev).expect("serialize result ev");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize result ev");
        assert_eq!(ev, des_ev);
    }

    #[test]
    fn test_visual_qa_serialization() {
        assert_eq!(AgentRole::VisualQA.name(), "Visual QA");

        let req_cap = Request::CaptureScreen {
            target: Some("firefox".into()),
            save_path: Some("/tmp/screenshot.png".into()),
        };
        let json_cap = serde_json::to_string(&req_cap).expect("serialize cap req");
        let des_cap: Request = serde_json::from_str(&json_cap).expect("deserialize cap req");
        assert_eq!(req_cap, des_cap);

        let report = VisualQAReport {
            target: "antOS-Barra".into(),
            image_width: 1920,
            image_height: 1080,
            image_size_bytes: 204800,
            findings: vec![VisualFinding {
                category: "alignment".into(),
                severity: "warning".into(),
                description: "Right margin misaligned by 4px on eBPF badge".into(),
                coordinates: Some("x: 1840, y: 12, w: 60, h: 24".into()),
                recommendation: "Align padding-right to 8px in style.css".into(),
            }],
            pass: false,
            summary: "1 visual warning detected".into(),
        };

        let ev_rep = Event::VisualQAReport(report);
        let json_rep = serde_json::to_string(&ev_rep).expect("serialize rep ev");
        let des_rep: Event = serde_json::from_str(&json_rep).expect("deserialize rep ev");
        assert_eq!(ev_rep, des_rep);
    }

    #[test]
    fn test_storage_installer_serialization() {
        let req_list = Request::ListDisks;
        let json_list = serde_json::to_string(&req_list).expect("serialize list req");
        let des_list: Request = serde_json::from_str(&json_list).expect("deserialize list req");
        assert_eq!(req_list, des_list);

        let part = DiskPartition {
            number: 1,
            name: "EFI System Partition".into(),
            size_bytes: 536870912,
            fs_type: Some("vfat".into()),
            mountpoint: Some("/boot/efi".into()),
            is_efi: true,
            is_bootable: true,
            uuid: Some("ABCD-1234".into()),
        };

        let dev = DiskDevice {
            path: "/dev/nvme0n1".into(),
            model: "Samsung SSD 980 PRO 1TB".into(),
            size_bytes: 1000204886016,
            sector_size: 512,
            bus_type: "nvme".into(),
            partition_table: "gpt".into(),
            partitions: vec![part],
            is_read_only: false,
        };

        let ev_dev = Event::DiskList(vec![dev.clone()]);
        let json_ev = serde_json::to_string(&ev_dev).expect("serialize list ev");
        let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize list ev");
        assert_eq!(ev_dev, des_ev);

        let plan = PartitionPlan {
            target_device: "/dev/nvme0n1".into(),
            clean_install: true,
            efi_partition_bytes: 536870912,
            root_partition_bytes: 900000000000,
            swap_partition_bytes: 17179869184,
            aligned_start_sector: 2048,
            warnings: vec!["Drive will be entirely formatted".into()],
        };

        let ev_plan = Event::PartitionPlan(plan);
        let json_plan = serde_json::to_string(&ev_plan).expect("serialize plan ev");
        let des_plan: Event = serde_json::from_str(&json_plan).expect("deserialize plan ev");
        assert_eq!(ev_plan, des_plan);

        let cfg = InstallConfig {
            target_device: "/dev/sda".into(),
            clean_install: false,
            target_mount: "/mnt/test".into(),
            hostname: "antos-dev".into(),
            username: "developer".into(),
            timezone: "America/Bogota".into(),
            dry_run: true,
        };
        let req_install = Request::InstallSystem(cfg.clone());
        let json_ins = serde_json::to_string(&req_install).expect("serialize install req");
        let des_ins: Request = serde_json::from_str(&json_ins).expect("deserialize install req");
        assert_eq!(req_install, des_ins);

        let report = InstallReport {
            target_device: "/dev/sda".into(),
            mode: "dual-boot".into(),
            success: true,
            steps: vec![InstallStep {
                name: "mount".into(),
                description: "Partition mounting".into(),
                completed: true,
            }],
            efi_partition: "/dev/sda1".into(),
            root_partition: "/dev/sda3".into(),
            fstab_entries: vec!["UUID=123 / ext4 defaults 0 1".into()],
            summary: "Installation completed".into(),
        };
        let ev_rep = Event::InstallReport(report);
        let json_rep = serde_json::to_string(&ev_rep).expect("serialize rep ev");
        let des_rep: Event = serde_json::from_str(&json_rep).expect("deserialize rep ev");
        assert_eq!(ev_rep, des_rep);

        let os = OsEntry {
            name: "Windows Boot Manager".into(),
            os_type: "windows".into(),
            efi_path: "\\EFI\\Microsoft\\Boot\\bootmgfw.efi".into(),
            disk_device: "/dev/nvme0n1".into(),
            partition_number: 1,
        };
        let ev_os = Event::DetectedOperatingSystems(vec![os.clone()]);
        let json_os = serde_json::to_string(&ev_os).expect("serialize os ev");
        let des_os: Event = serde_json::from_str(&json_os).expect("deserialize os ev");
        assert_eq!(ev_os, des_os);

        let boot_cfg = BootloaderConfig {
            esp_mount: "/boot/efi".into(),
            target_device: "/dev/nvme0n1".into(),
            efi_partition: 1,
            default_os: "antos".into(),
            timeout_seconds: 5,
            detected_os: vec![os],
            dry_run: true,
        };
        let req_boot = Request::InstallBootloader(boot_cfg);
        let json_boot = serde_json::to_string(&req_boot).expect("serialize boot req");
        let des_boot: Request = serde_json::from_str(&json_boot).expect("deserialize boot req");
        assert_eq!(req_boot, des_boot);
    }

    #[test]
    fn test_microvm_types_serialization() {
        let cfg = MicrovmConfig {
            vm_id: "vm-test-1".into(),
            vcpu_count: 4,
            memory_mb: 1024,
            kernel_image: "/boot/antos-vmlinuz".into(),
            initrd_image: Some("/boot/initrd.img".into()),
            overlay_disk: Some("/tmp/overlay.qcow2".into()),
            vsock_port: 8080,
            command: Some("cargo test".into()),
        };
        let req_spawn = Request::SpawnMicrovm(cfg.clone());
        let json_spawn = serde_json::to_string(&req_spawn).expect("serialize spawn");
        let des_spawn: Request = serde_json::from_str(&json_spawn).expect("deserialize spawn");
        assert_eq!(req_spawn, des_spawn);

        let status = MicrovmStatus {
            kvm_available: true,
            hypervisor_engine: "Cloud-Hypervisor / KVM".into(),
            active_vms_count: 1,
            total_memory_allocated_mb: 1024,
            vsock_supported: true,
            kernel_version: "7.1.3".into(),
        };
        let ev_status = Event::MicrovmStatus(status.clone());
        let json_st = serde_json::to_string(&ev_status).expect("serialize status");
        let des_st: Event = serde_json::from_str(&json_st).expect("deserialize status");
        assert_eq!(ev_status, des_st);

        let exec_res = MicrovmExecResult {
            vm_id: "vm-test-1".into(),
            command: "echo hello".into(),
            exit_code: 0,
            stdout: "hello\n".into(),
            stderr: String::new(),
            duration_ms: 45,
            success: true,
        };
        let ev_exec = Event::MicrovmResult(exec_res.clone());
        let json_exec = serde_json::to_string(&ev_exec).expect("serialize exec");
        let des_exec: Event = serde_json::from_str(&json_exec).expect("deserialize exec");
        assert_eq!(ev_exec, des_exec);
    }
}
