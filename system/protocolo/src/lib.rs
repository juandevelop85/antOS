//! El contrato entre el demonio de antOS y sus clientes.
//!
//! Vivía dentro de `antosd` mientras el único cliente era su propio terminal.
//! Sale a un crate aparte en cuanto aparece un segundo cliente —la barra de
//! intención— porque la alternativa sería que cada uno tuviera su copia de
//! estos tipos. Un protocolo duplicado es un protocolo que diverge.
//!
//! Aquí NO hay lógica: ni se decide un nivel de permiso, ni se calcula un
//! diff, ni se valida nada. Eso vive en el demonio, y es deliberado — un
//! cliente que pudiera calcular su propio nivel podría elegirlo.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ------------------------------------------------------------ nivel y plan

/// El orden de las variantes ES la escala: Auto < Confirm < Grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Auto,
    Confirm,
    Grant,
}

/// El suelo de la escala. Que el valor por defecto sea el nivel MÁS permisivo
/// es seguro precisamente porque la derivación solo sabe subir.
impl Default for Tier {
    fn default() -> Self {
        Tier::Auto
    }
}

impl Tier {
    pub fn label(self) -> &'static str {
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

// ----------------------------------------------------------- diff interactivo (T8.1)

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

// ----------------------------------------------------------- notificaciones (T8.2)

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

// ----------------------------------------------------------- red p2p / antMesh (T9.1)

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

// ----------------------------------------------------------- swarm distribuido (T9.2)

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

// ----------------------------------------------------------- vfs semantico (T10.1)

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

// --------------------------------------------- profiler continuo de runtime (T11.2)

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

/// Tipo de editor o cliente de desarrollo compatible con el servidor LSP de antOS.
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

/// Estado del servidor LSP embebido de antOS.
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

/// Posición de un cursor virtual en una sesión de co-edición colaborativa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollabCursor {
    pub client_id: String,
    pub line: usize,
    pub character: usize,
    pub ghost_text: Option<String>,
}

/// Estado de una sesión de co-edición en tiempo real (CRDT).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollabSessionStatus {
    pub session_id: String,
    pub file_path: String,
    pub collaborators: Vec<String>,
    pub cursors: Vec<CollabCursor>,
    pub buffer_length: usize,
    pub active_ticket_id: Option<String>,
}

/// Punto de interrupción en una sesión de depuración aislada DAP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapBreakpoint {
    pub id: usize,
    pub file_path: String,
    pub line: usize,
    pub verified: bool,
}

/// Variable inspeccionada en tiempo de depuración.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapVariable {
    pub name: String,
    pub value: String,
    pub type_name: String,
}

/// Estado de una sesión DAP de depuración aislada en sandbox.
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

/// Atajo de teclado global del escritorio antOS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopHotkey {
    pub key: String,
    pub action: String,
    pub description: String,
}

/// Estado del entorno de escritorio gráfico Wayland.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopSessionStatus {
    pub running: bool,
    pub compositor_name: String,
    pub wayland_display: Option<String>,
    pub active_clients_count: usize,
    pub registered_hotkeys: Vec<DesktopHotkey>,
}

// ------------------------------------------------ Barra Telemetry (T13.1)

/// Alerta o notificación visual emitida hacia la barra de escritorio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarraAlert {
    pub category: String,
    pub message: String,
    pub urgent: bool,
}

/// Telemetría consolidada en tiempo real para la barra de escritorio antOS.
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

/// Estado del pipeline de arranque bare metal y emulación QEMU.
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

/// Resumen de un plugin WebAssembly instalado en antOS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSummary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub wasm_size_bytes: u64,
}

/// Resultado de la ejecución de una acción en un plugin WASM.
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

/// Hallazgo específico detectado durante la inspección visual multimodal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualFinding {
    pub category: String,
    pub severity: String,
    pub description: String,
    pub coordinates: Option<String>,
    pub recommendation: String,
}

/// Reporte consolidado de auditoría de interfaz gráfica generado por VisualQA.
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

/// Resultado de una captura de pantalla Wayland.
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

/// Representa una partición individual dentro de una unidad de almacenamiento.
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

/// Representa un dispositivo de almacenamiento físico o virtual.
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

/// Plan de particionamiento propuesto para instalación en disco.
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

/// Configuración de instalación del sistema operativo antOS (T15.2).
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

/// Paso individual en el pipeline de instalación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallStep {
    pub name: String,
    pub description: String,
    pub completed: bool,
}

/// Reporte de finalización o simulación del despliegue del sistema (T15.2).
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

/// Entrada de sistema operativo detectado para arranque dual (T15.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsEntry {
    pub name: String,
    pub os_type: String,
    pub efi_path: String,
    pub disk_device: String,
    pub partition_number: u32,
}

/// Configuración de instalación del cargador de arranque UEFI (T15.3).
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

/// Reporte de configuración o instalación del bootloader UEFI (T15.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootloaderReport {
    pub success: bool,
    pub esp_path: String,
    pub efibootmgr_command: String,
    pub entries_configured: Vec<String>,
    pub loader_conf_content: String,
    pub summary: String,
}

// -------------------------------------------------------------- propuesta

/// Lo que se le enseña a alguien antes de tocar nada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Propuesta {
    pub plan: Plan,
    pub cambios: Vec<Line>,
    pub radio: Radio,
    pub nivel: Tier,
    pub razones: Vec<String>,
    pub recinto: Recinto,
    /// Si es `true`, no se ejecutará pase lo que pase: solo se está mirando.
    pub seco: bool,
}

/// Los efectos DECLARADOS, ya resueltos a texto.
///
/// Se resuelven en el demonio y no en el cliente a propósito: un cliente no
/// debería necesitar acceso al sistema de ficheros para enseñar un plan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Radio {
    pub escribe: Vec<String>,
    pub borra: Vec<String>,
    pub lee: Vec<String>,
    pub sistema: Vec<String>,
    pub red: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recinto {
    pub motor: String,
    pub garantiza: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resultado {
    pub ok: bool,
    pub mensaje: String,
    pub instantanea: Option<String>,
}

// ------------------------------------------------- introspección git (T1.1)

/// Estado de modificación de un archivo rastreado o no rastreado en Git.
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

/// Resumen granular de cambios en un archivo dentro del repositorio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileDiffSummary {
    pub ruta: String,
    pub lineas_anadidas: usize,
    pub lineas_borradas: usize,
    pub estado: GitFileStatus,
}

/// Estado global de un repositorio Git en el espacio de trabajo.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitRepoStatus {
    /// Rama activa actual (ej. `main`, `feature/x`) o `None` si es HEAD desacoplado.
    pub rama: Option<String>,
    /// Hash abreviado o completo del commit HEAD.
    pub head_commit: Option<String>,
    /// Cantidad de commits locales por delante del upstream remoto.
    pub delante: usize,
    /// Cantidad de commits locales por detrás del upstream remoto.
    pub detras: usize,
    /// Archivos con modificaciones en el árbol de trabajo (no staged).
    pub modificados: Vec<GitFileDiffSummary>,
    /// Archivos preparados en el índice (staged).
    pub staged: Vec<GitFileDiffSummary>,
    /// Archivos no rastreados en el repositorio.
    pub sin_seguimiento: Vec<String>,
    /// Indica si el árbol de trabajo y el índice están totalmente limpios.
    pub limpio: bool,
}

impl GitRepoStatus {
    pub fn is_clean(&self) -> bool {
        self.modificados.is_empty() && self.staged.is_empty() && self.sin_seguimiento.is_empty()
    }

    /// Helper para verificar si el estado no tiene modificaciones ni archivos pendientes.
    pub fn es_limpio(&self) -> bool {
        self.is_clean()
    }
}

// ------------------------------------------------ spec engine y tickets (T1.3)

/// Estado de un ticket de especificación o desarrollo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Pendiente,
    EnProgreso,
    EnRevision,
    Completado,
}

impl TicketStatus {
    pub fn label(&self) -> &'static str {
        match self {
            TicketStatus::Pendiente => "Pending",
            TicketStatus::EnProgreso => "In Progress",
            TicketStatus::EnRevision => "In Review",
            TicketStatus::Completado => "Completed",
        }
    }

    pub fn etiqueta(&self) -> &'static str {
        match self {
            TicketStatus::Pendiente => "⏳ Pendiente",
            TicketStatus::EnProgreso => "🔄 En Progreso",
            TicketStatus::EnRevision => "🔍 En Revisión",
            TicketStatus::Completado => "✅ Completado",
        }
    }
}

/// Resumen de un ticket para listados y tableros Kanban.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketSummary {
    pub id: String,
    pub fase: String,
    pub titulo: String,
    pub estado: TicketStatus,
    pub ruta_archivo: String,
}

/// Detalle completo de un ticket parseado desde Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketDetail {
    pub id: String,
    pub fase: String,
    pub titulo: String,
    pub estado: TicketStatus,
    pub ruta_archivo: String,
    pub descripcion: String,
    pub alcance_tecnico: Vec<String>,
    pub criterios_aceptacion: Vec<String>,
}

/// Información de diagnóstico de un puerto TCP en escucha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortDiagnosticInfo {
    pub port: u16,
    pub pid: u32,
    pub process_name: String,
    pub command: String,
    pub working_dir: Option<String>,
}

// ---------------------------------------------------------------- antFlow: multi-agente (T3.1)

/// Rol especializado de un agente dentro del flujo antFlow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Arquitecto,
    Coder,
    QA,
    Auditor,
    VisualQA,
}

impl AgentRole {
    pub fn name(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => "Architect",
            AgentRole::Coder => "Coder",
            AgentRole::QA => "QA / Tester",
            AgentRole::Auditor => "Security Auditor",
            AgentRole::VisualQA => "Visual QA",
        }
    }

    pub fn nombre(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => "Arquitecto",
            AgentRole::Coder => "Coder",
            AgentRole::QA => "QA / Tester",
            AgentRole::Auditor => "Auditor de Seguridad",
            AgentRole::VisualQA => "QA Visual",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => "Technical planning, ticket breakdown and architecture design.",
            AgentRole::Coder => "Modular implementation of changes and refactoring in the worktree.",
            AgentRole::QA => "Automated test suite generation and execution in sandbox.",
            AgentRole::Auditor => "Review of diffs, security, style and blast radius.",
            AgentRole::VisualQA => "Multimodal inspection of GUI windows, screenshots and visual regression testing.",
        }
    }

    pub fn descripcion(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => "Planificación técnica, descomposición de tickets y diseño de arquitectura.",
            AgentRole::Coder => "Implementación modular de cambios y refactorización en el worktree.",
            AgentRole::QA => "Generación y ejecución de pruebas automatizadas en sandbox.",
            AgentRole::Auditor => "Revisión de diffs, seguridad, estilo y radio de impacto.",
            AgentRole::VisualQA => "Inspección visual multimodal de interfaces gráficas, capturas de pantalla y regresión visual.",
        }
    }

    pub fn system_prompt(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => {
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

    pub fn prompt_sistema(&self) -> &'static str {
        self.system_prompt()
    }
}

/// Estado en la máquina de estados del ciclo de vida de una tarea en antFlow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowState {
    Pendiente,
    Planificando,
    Implementando,
    VerificandoTests,
    RevisionAuditor,
    ListoParaAprobacion,
    Fusionado,
    Fallido,
}

impl FlowState {
    pub fn label(&self) -> &'static str {
        match self {
            FlowState::Pendiente => "Pending",
            FlowState::Planificando => "Planning (Architect)",
            FlowState::Implementando => "Implementing (Coder)",
            FlowState::VerificandoTests => "Running Tests (QA)",
            FlowState::RevisionAuditor => "Reviewing (Auditor)",
            FlowState::ListoParaAprobacion => "Ready for Approval",
            FlowState::Fusionado => "Merged",
            FlowState::Fallido => "Failed",
        }
    }

    pub fn etiqueta(&self) -> &'static str {
        match self {
            FlowState::Pendiente => "⏳ Pendiente",
            FlowState::Planificando => "📐 Planificando (Arquitecto)",
            FlowState::Implementando => "💻 Implementando (Coder)",
            FlowState::VerificandoTests => "🧪 Verificando Tests (QA)",
            FlowState::RevisionAuditor => "🛡️ Revisión (Auditor)",
            FlowState::ListoParaAprobacion => "✨ Listo para Aprobación",
            FlowState::Fusionado => "✅ Fusionado",
            FlowState::Fallido => "❌ Fallido",
        }
    }

    pub fn active_role(&self) -> Option<AgentRole> {
        match self {
            FlowState::Planificando => Some(AgentRole::Arquitecto),
            FlowState::Implementando => Some(AgentRole::Coder),
            FlowState::VerificandoTests => Some(AgentRole::QA),
            FlowState::RevisionAuditor => Some(AgentRole::Auditor),
            _ => None,
        }
    }

    pub fn rol_activo(&self) -> Option<AgentRole> {
        self.active_role()
    }
}

/// Registro de una transición de estado en el flujo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowTransition {
    pub timestamp_segundos: u64,
    pub estado_anterior: FlowState,
    pub estado_nuevo: FlowState,
    pub rol: Option<AgentRole>,
    pub detalle: String,
}

/// Tarea activa o histórica gestionada por el orquestador antFlow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowTask {
    pub id: String,
    pub ticket_id: String,
    pub estado: FlowState,
    pub rol_actual: Option<AgentRole>,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub reintentos_qa: u32,
    pub max_reintentos_qa: u32,
    pub diff_preview: Option<String>,
    pub resumen_auditoria: Option<String>,
    pub historial: Vec<FlowTransition>,
}

// ---------------------------------------------------------------- mensajes

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Peticion {
    Intencion {
        texto: String,
        planificador: Option<String>,
        seco: bool,
    },
    /// La respuesta a una propuesta. Es lo ÚNICO que un cliente decide.
    Aprobacion(bool),
    /// Consulta el estado del repositorio Git en la ruta del espacio de trabajo.
    ConsultarEstadoGit {
        workspace_path: String,
    },
    /// Lista todos los tickets disponibles en el espacio de trabajo.
    ListarTickets {
        workspace_path: String,
    },
    /// Obtiene el detalle de un ticket específico en el espacio de trabajo.
    ObtenerTicket {
        workspace_path: String,
        ticket_id: String,
    },
    /// Diagnostica puertos TCP en escucha y los procesos asociados.
    DiagnosticarPuertos {
        port: Option<u16>,
    },
    /// Inicia el flujo multi-agente antFlow para un ticket.
    IniciarFlow {
        workspace_path: String,
        ticket_id: String,
    },
    /// Consulta el estado de la tarea antFlow para un ticket.
    ConsultarFlow {
        ticket_id: String,
    },
    /// Lista todas las tareas de agentes activas.
    ListarFlows {
        workspace_path: String,
    },
    /// Aprueba o rechaza los cambios finales de una tarea en antFlow.
    AprobarFlow {
        ticket_id: String,
        decision: bool,
    },
    /// Consulta el diff estructurado y sintáctico para un ticket, archivo o commit (T8.1)
    ConsultarDiff {
        workspace_path: String,
        target: Option<String>,
    },
    /// Lista notificaciones pendientes de agentes y del sistema (T8.2)
    ListarNotificaciones {
        workspace_path: String,
    },
    /// Ejecuta una acción sobre una notificación (aprobación, rechazo, descarte) (T8.2)
    AccionNotificacion {
        workspace_path: String,
        notification_id: String,
        action: NotificationAction,
    },
    /// Consulta el estado de la red P2P antMesh y los peers conocidos (T9.1)
    ConsultarMesh {
        workspace_path: String,
    },
    /// Conecta a un nodo peer por dirección IP/puerto o multiaddr (T9.1)
    ConectarPeer {
        workspace_path: String,
        address: String,
    },
    /// Genera un token seguro de emparejamiento con expiración (T9.1)
    GenerarTokenEmparejamiento {
        workspace_path: String,
    },
    /// Consulta el estado de distribución de tareas del Swarm multi-agente (T9.2)
    ConsultarSwarm {
        workspace_path: String,
    },
    /// Despacha un rol específico de un ticket a un nodo remoto o auto-seleccionado (T9.2)
    DespacharRolRemoto {
        workspace_path: String,
        ticket_id: String,
        role: AgentRole,
        node_id: Option<String>,
    },
    /// Consulta una ruta virtual o lista un directorio en /antfs (T10.1)
    ConsultarVfs {
        workspace_path: String,
        virtual_path: String,
    },
    /// Monta la proyección virtual de /antfs en el punto de montaje indicado (T10.1)
    MontarVfs {
        workspace_path: String,
        mount_point: Option<String>,
    },
    /// Desmonta la proyección virtual de /antfs (T10.1)
    DesmontarVfs {
        workspace_path: String,
        mount_point: Option<String>,
    },
    /// Intercepta y valida sintácticamente un buffer de código antes de persistir (T10.2)
    ValidarEscrituraVfs {
        file_path: String,
        content: String,
    },
    /// Consulta el estado del interceptor de escrituras semánticas (T10.2)
    ConsultarGuardVfs {
        workspace_path: String,
    },
    /// Consulta el estado y compatibilidad de las sondas kernel eBPF LSM (T11.1)
    ConsultarEbpfStatus {
        workspace_path: String,
    },
    /// Consulta el registro de auditoría de syscalls y eventos de seguridad eBPF (T11.1)
    ConsultarEbpfAuditLog {
        workspace_path: String,
        limit: usize,
    },
    /// Simula un intento de evasión de sandbox para verificar la detección (T11.1)
    SimularViolacionEbpf {
        workspace_path: String,
        hook: EbpfHookKind,
        target_resource: String,
    },
    /// Ejecuta un comando bajo el profiler de rendimiento continuo (T11.2)
    EjecutarProfiler {
        workspace_path: String,
        command: String,
    },
    /// Consulta el histórico de reportes del profiler (T11.2)
    ConsultarProfilerReportes {
        workspace_path: String,
        limit: usize,
    },
    /// Analiza puntos calientes y genera recomendaciones para agentes (T11.2)
    AnalizarProfilerHotspots {
        workspace_path: String,
    },
    /// Consulta el estado y capacidades del servidor LSP embebido (T12.1)
    ConsultarLspStatus {
        workspace_path: String,
    },
    /// Genera la configuración para conectar el editor al servidor LSP de antOS (T12.1)
    ObtenerLspConfig {
        editor: LspEditorKind,
        workspace_path: String,
    },
    /// Inicia una sesión interactiva de pair programming y co-edición con el agente Coder (T12.2)
    IniciarCollabSession {
        file_path: String,
        ticket_id: Option<String>,
        workspace_path: String,
    },
    /// Consulta el estado y cursores de la sesión de co-edición activa (T12.2)
    ConsultarCollabStatus {
        session_id: String,
        workspace_path: String,
    },
    /// Inicia una sesión de depuración aislada bajo el protocolo DAP en sandbox (T12.2)
    IniciarDapSession {
        command: String,
        workspace_path: String,
    },
    /// Consulta el estado, variables y pila de llamadas de la sesión DAP (T12.2)
    ConsultarDapStatus {
        session_id: String,
        workspace_path: String,
    },
    /// Consulta el estado del compositor y entorno de escritorio Wayland (T13.0)
    ConsultarDesktopStatus,
    /// Obtiene los atajos de teclado globales registrados en el escritorio (T13.0)
    ListarDesktopHotkeys,
    /// Inicia la sesión de escritorio Wayland de antOS (T13.0)
    IniciarDesktopSession {
        nested: bool,
    },
    /// Consulta el estado consolidado de telemetría de fondo para la barra (T13.1)
    ConsultarBarraTelemetry,
    /// Emite una alerta o actualización visual hacia la barra (T13.1)
    EmitirBarraAlert(BarraAlert),
    /// Consulta el estado del pipeline de arranque bare metal y binarios (T13.2)
    ConsultarBootStatus,
    /// Ejecuta una acción del pipeline de arranque (build, qemu, test) (T13.2)
    EjecutarBootPipeline {
        action: String,
        headless: bool,
    },
    /// Lista los plugins WebAssembly instalados (T14.1)
    ListarPlugins,
    /// Ejecuta una acción dentro de un plugin WASM (T14.1)
    EjecutarPlugin {
        plugin_name: String,
        action: String,
        params: std::collections::BTreeMap<String, String>,
    },
    /// Instala un plugin WASM desde un directorio o manifiesto (T14.1)
    InstalarPlugin {
        source_path: String,
    },
    /// Captura una pantalla o ventana Wayland (T14.2)
    CapturarPantalla {
        target: Option<String>,
        save_path: Option<String>,
    },
    /// Ejecuta inspección visual multimodal con VisualQA (T14.2)
    InspeccionarVisualQA {
        target: String,
        criteria: Vec<String>,
    },
    /// Lista los dispositivos de almacenamiento detectados en el sistema (T15.1)
    ListarDiscos,
    /// Inspecciona un dispositivo de almacenamiento específico (T15.1)
    InspeccionarDisco {
        device: String,
    },
    /// Calcula o aplica un esquema de particiones GPT en un disco (T15.1)
    ParticionarDisco {
        device: String,
        clean_install: bool,
        dry_run: bool,
    },
    /// Instala el sistema antOS en un disco físico o virtual (T15.2)
    InstalarSistema(InstallConfig),
    /// Sondea sistemas operativos existentes en particiones EFI / disco (T15.3)
    SondearSistemasOperativos {
        esp_mount: Option<String>,
    },
    /// Instala y configura el cargador de arranque UEFI (T15.3)
    InstalarBootloader(BootloaderConfig),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Evento {
    Inicio { intencion: String, planificador: String },
    Nota(String),
    Propuesta(Box<Propuesta>),
    Salida(String),
    Resultado(Resultado),
    /// Respuesta con el estado detallado del repositorio Git.
    EstadoGit(GitRepoStatus),
    /// Respuesta cuando el directorio consultado no es un repositorio Git válido.
    NoEsRepoGit,
    /// Respuesta con el listado de tickets encontrados en el workspace.
    ListaTickets(Vec<TicketSummary>),
    /// Respuesta con el detalle de un ticket específico.
    DetalleTicket(Option<TicketDetail>),
    /// Respuesta con el listado de puertos diagnosticados.
    EstadoPuertos(Vec<PortDiagnosticInfo>),
    /// Estado detallado de una tarea de agentes antFlow.
    EstadoFlow(Option<FlowTask>),
    /// Listado de todas las tareas antFlow.
    ListaFlows(Vec<FlowTask>),
    /// Notificación de transición de estado en antFlow.
    TransicionFlow {
        ticket_id: String,
        estado_anterior: FlowState,
        estado_nuevo: FlowState,
        rol: Option<AgentRole>,
        detalle: String,
    },
    /// Respuesta con diffs estructurados y coloreados sintácticamente (T8.1)
    DiffEstructurado(Vec<DiffFile>),
    /// Respuesta con la lista de notificaciones activas (T8.2)
    ListaNotificaciones(Vec<NotificationItem>),
    /// Respuesta al ejecutar una acción sobre una notificación (T8.2)
    ResultadoNotificacion {
        id: String,
        success: bool,
        message: String,
    },
    /// Respuesta con el estado de la malla antMesh y lista de peers (T9.1)
    EstadoMesh(MeshStatus),
    /// Token de emparejamiento generado para un nuevo nodo (T9.1)
    TokenEmparejamientoGenerado(PairingToken),
    /// Resultado de la conexión a un nodo peer (T9.1)
    ResultadoConexionPeer {
        address: String,
        success: bool,
        message: String,
    },
    /// Respuesta con el estado del clúster Swarm y roles asignados (T9.2)
    EstadoSwarm(SwarmStatus),
    /// Resultado del despacho de un rol a un nodo Swarm (T9.2)
    ResultadoDespachoSwarm {
        ticket_id: String,
        role: AgentRole,
        assigned_node_id: String,
        success: bool,
        message: String,
    },
    /// Listado de entradas virtuales en un directorio de /antfs (T10.1)
    ListadoVfs {
        virtual_path: String,
        entries: Vec<VfsEntry>,
    },
    /// Contenido virtual de un nodo semántico o diff en /antfs (T10.1)
    ContenidoVfs {
        virtual_path: String,
        content: String,
    },
    /// Resultado de montar, desmontar o consultar el VFS (T10.1)
    ResultadoVfs {
        action: String,
        success: bool,
        message: String,
    },
    /// Resultado de la validación sintáctica previa a disco (T10.2)
    ResultadoValidacionVfs(ValidationResult),
    /// Estado del interceptor de escrituras semánticas (T10.2)
    EstadoGuardVfs(VfsGuardStatus),
    /// Estado de las sondas kernel eBPF LSM (T11.1)
    EstadoEbpf(EbpfStatus),
    /// Eventos de seguridad y trazas del ring buffer eBPF (T11.1)
    AuditLogEbpf(Vec<EbpfSecurityEvent>),
    /// Resultado de una acción o simulación eBPF (T11.1)
    ResultadoEbpf {
        action: String,
        success: bool,
        message: String,
    },
    /// Reporte detallado de ejecución bajo el profiler (T11.2)
    ReporteProfiler(ProfileReport),
    /// Histórico de reportes de profiling (T11.2)
    ListaReportesProfiler(Vec<ProfileReport>),
    /// Diagnóstico de puntos calientes y sugerencias para agentes (T11.2)
    AnalisisProfiler {
        hotspots: Vec<ProfileHotspot>,
        suggestions: Vec<ProfileSuggestion>,
    },
    /// Estado del servidor LSP embebido de antOS (T12.1)
    EstadoLsp(LspServerStatus),
    /// Configuración recomendada para conectar un editor externo al servidor LSP (T12.1)
    ConfiguracionLsp {
        editor: LspEditorKind,
        config_content: String,
        target_file: String,
    },
    /// Estado de la sesión de co-edición y programación en pareja (T12.2)
    EstadoCollabSession(CollabSessionStatus),
    /// Estado de la sesión de depuración supervisada DAP (T12.2)
    EstadoDapSession(DapSessionStatus),
    /// Resultado de una acción de colaboración o delta CRDT (T12.2)
    ResultadoCollab {
        action: String,
        success: bool,
        message: String,
    },
    /// Resultado de una operación DAP (breakpoint, step, eval) (T12.2)
    ResultadoDap {
        action: String,
        success: bool,
        message: String,
    },
    /// Estado del entorno de escritorio gráfico Wayland (T13.0)
    EstadoDesktop(DesktopSessionStatus),
    /// Listado de atajos de teclado del escritorio (T13.0)
    ListaDesktopHotkeys(Vec<DesktopHotkey>),
    /// Estado consolidado de telemetría para la barra de escritorio (T13.1)
    EstadoBarraTelemetry(BarraTelemetry),
    /// Alerta o notificación visual emitida a la barra (T13.1)
    AlertaBarra(BarraAlert),
    /// Estado del pipeline de arranque bare metal y binarios (T13.2)
    EstadoBoot(BootPipelineStatus),
    /// Resultado de la ejecución del pipeline de arranque (T13.2)
    ResultadoBoot {
        action: String,
        output: String,
        success: bool,
    },
    /// Listado de plugins WebAssembly disponibles (T14.1)
    ListaPlugins(Vec<PluginSummary>),
    /// Resultado de la ejecución de una acción de plugin WASM (T14.1)
    ResultadoPlugin(PluginResult),
    /// Resultado de la captura de pantalla Wayland (T14.2)
    ResultadoCaptura(ScreenshotResult),
    /// Reporte de auditoría visual multimodal de VisualQA (T14.2)
    ReporteVisualQA(VisualQAReport),
    /// Lista de dispositivos de almacenamiento detectados (T15.1)
    ListaDiscos(Vec<DiskDevice>),
    /// Detalle e inspección de un dispositivo de almacenamiento (T15.1)
    DetalleDisco(Option<DiskDevice>),
    /// Plan o resultado de particionamiento (T15.1)
    PlanParticionamiento(PartitionPlan),
    /// Reporte de instalación del sistema base antOS (T15.2)
    ReporteInstalacion(InstallReport),
    /// Lista de sistemas operativos detectados en la máquina (T15.3)
    SistemasOperativosDetectados(Vec<OsEntry>),
    /// Reporte de instalación y configuración de bootloader UEFI (T15.3)
    ReporteBootloader(BootloaderReport),
    Error(String),
}

/// Alias semánticos para clientes y especificaciones IPC.
pub type Mensaje = Peticion;
pub type Respuesta = Evento;

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serializacion_git_repo_status() {
        let status = GitRepoStatus {
            rama: Some("main".into()),
            head_commit: Some("a1b2c3d".into()),
            delante: 2,
            detras: 0,
            modificados: vec![GitFileDiffSummary {
                ruta: "system/protocolo/src/lib.rs".into(),
                lineas_anadidas: 45,
                lineas_borradas: 2,
                estado: GitFileStatus::Modified,
            }],
            staged: vec![GitFileDiffSummary {
                ruta: "Cargo.toml".into(),
                lineas_anadidas: 1,
                lineas_borradas: 0,
                estado: GitFileStatus::Created,
            }],
            sin_seguimiento: vec!["scratch.txt".into()],
            limpio: false,
        };

        let json = serde_json::to_string(&status).expect("debe serializar a JSON");
        let deserializado: GitRepoStatus =
            serde_json::from_str(&json).expect("debe deserializar desde JSON");

        assert_eq!(status, deserializado);
        assert!(!deserializado.es_limpio());
    }

    #[test]
    fn test_serializacion_peticion_consultar_estado_git() {
        let peticion = Mensaje::ConsultarEstadoGit {
            workspace_path: "/Users/dev/workspace".into(),
        };

        let json = serde_json::to_string(&peticion).expect("debe serializar petición");
        let deserializado: Peticion =
            serde_json::from_str(&json).expect("debe deserializar petición");

        assert_eq!(peticion, deserializado);
    }

    #[test]
    fn test_serializacion_respuesta_estado_git_y_no_es_repo() {
        let resp_ok = Respuesta::EstadoGit(GitRepoStatus {
            rama: Some("feature/git-inspector".into()),
            limpio: true,
            ..Default::default()
        });

        let json_ok = serde_json::to_string(&resp_ok).expect("debe serializar EstadoGit");
        let deserializado_ok: Evento =
            serde_json::from_str(&json_ok).expect("debe deserializar EstadoGit");
        assert_eq!(resp_ok, deserializado_ok);

        let resp_no_repo = Respuesta::NoEsRepoGit;
        let json_no_repo =
            serde_json::to_string(&resp_no_repo).expect("debe serializar NoEsRepoGit");
        let deserializado_no_repo: Evento =
            serde_json::from_str(&json_no_repo).expect("debe deserializar NoEsRepoGit");
        assert_eq!(resp_no_repo, deserializado_no_repo);
    }

    #[test]
    fn test_compatibilidad_mensajes_existentes() {
        let peticion_intencion = Peticion::Intencion {
            texto: "compilar kernel".into(),
            planificador: Some("reglas".into()),
            seco: false,
        };
        let json = serde_json::to_string(&peticion_intencion).expect("serializar intencion");
        let deserializado: Peticion = serde_json::from_str(&json).expect("deserializar intencion");
        assert_eq!(peticion_intencion, deserializado);

        let evento_nota = Evento::Nota("analizando dependencias".into());
        let json_nota = serde_json::to_string(&evento_nota).expect("serializar nota");
        let deserializado_nota: Evento =
            serde_json::from_str(&json_nota).expect("deserializar nota");
        assert_eq!(evento_nota, deserializado_nota);
    }

    #[test]
    fn test_serializacion_tickets_protocolo() {
        let ticket = TicketDetail {
            id: "T1.3".into(),
            fase: "Fase 1".into(),
            titulo: "Indexador y parser de tickets".into(),
            estado: TicketStatus::EnProgreso,
            ruta_archivo: "docs/tickets/T1.3-spec-engine-tickets-parser.md".into(),
            descripcion: "Construir indexador de tickets".into(),
            alcance_tecnico: vec!["Parser markdown".into(), "Mensajes IPC".into()],
            criterios_aceptacion: vec!["Comando antos tickets".into()],
        };

        let json = serde_json::to_string(&ticket).expect("serializar ticket");
        let deserializado: TicketDetail = serde_json::from_str(&json).expect("deserializar ticket");
        assert_eq!(ticket, deserializado);

        let peticion_listar = Peticion::ListarTickets {
            workspace_path: "/workspace".into(),
        };
        let json_peticion = serde_json::to_string(&peticion_listar).expect("serializar peticion listar");
        let des_peticion: Peticion = serde_json::from_str(&json_peticion).expect("deserializar peticion listar");
        assert_eq!(peticion_listar, des_peticion);

        let respuesta_lista = Evento::ListaTickets(vec![TicketSummary {
            id: "T1.3".into(),
            fase: "Fase 1".into(),
            titulo: "Indexador y parser de tickets".into(),
            estado: TicketStatus::EnProgreso,
            ruta_archivo: "docs/tickets/T1.3-spec-engine-tickets-parser.md".into(),
        }]);
        let json_resp = serde_json::to_string(&respuesta_lista).expect("serializar lista tickets");
        let des_resp: Evento = serde_json::from_str(&json_resp).expect("deserializar lista tickets");
        assert_eq!(respuesta_lista, des_resp);

        let info_puerto = PortDiagnosticInfo {
            port: 3000,
            pid: 12345,
            process_name: "node".into(),
            command: "node server.js".into(),
            working_dir: Some("/app".into()),
        };
        let json_puerto = serde_json::to_string(&info_puerto).expect("serializar puerto");
        let des_puerto: PortDiagnosticInfo = serde_json::from_str(&json_puerto).expect("deserializar puerto");
        assert_eq!(info_puerto, des_puerto);

        let task = FlowTask {
            id: "flow-1".into(),
            ticket_id: "T3.1".into(),
            estado: FlowState::Planificando,
            rol_actual: Some(AgentRole::Arquitecto),
            worktree_path: Some("/state/worktrees/t3.1".into()),
            branch_name: Some("agent/t3.1".into()),
            reintentos_qa: 0,
            max_reintentos_qa: 3,
            diff_preview: Some("+ nuevo modulo flow".into()),
            resumen_auditoria: Some("arquitectura aprobada".into()),
            historial: vec![FlowTransition {
                timestamp_segundos: 1700000000,
                estado_anterior: FlowState::Pendiente,
                estado_nuevo: FlowState::Planificando,
                rol: Some(AgentRole::Arquitecto),
                detalle: "asignando tarea al arquitecto".into(),
            }],
        };

        let json_task = serde_json::to_string(&task).expect("serializar task");
        let des_task: FlowTask = serde_json::from_str(&json_task).expect("deserializar task");
        assert_eq!(task, des_task);
    }

    #[test]
    fn test_serializacion_diff_estructurado() {
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
                            SyntaxToken { text: "fn".into(), token_type: SyntaxTokenType::Keyword },
                            SyntaxToken { text: " main() {".into(), token_type: SyntaxTokenType::Normal },
                        ],
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        old_line_num: None,
                        new_line_num: Some(11),
                        content: "    println!(\"antOS\");".into(),
                        tokens: vec![
                            SyntaxToken { text: "    println!".into(), token_type: SyntaxTokenType::Keyword },
                            SyntaxToken { text: "(\"antOS\");".into(), token_type: SyntaxTokenType::StringLit },
                        ],
                    },
                ],
            }],
        };

        let req = Peticion::ConsultarDiff {
            workspace_path: "/ws".into(),
            target: Some("T8.1".into()),
        };
        let json_req = serde_json::to_string(&req).expect("serialize req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize req");
        assert_eq!(req, des_req);

        let event = Evento::DiffEstructurado(vec![diff_file.clone()]);
        let json_ev = serde_json::to_string(&event).expect("serialize event");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize event");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_serializacion_notificaciones() {
        let notif = NotificationItem {
            id: "notif-1".into(),
            ticket_id: "T8.2".into(),
            title: "Revisión requerida para T8.2".into(),
            body: "Agente QA validó todos los tests con éxito.".into(),
            kind: NotificationKind::ApprovalRequired,
            created_at: 1700000000,
            read: false,
            actions: vec![NotificationAction::Approve, NotificationAction::Reject, NotificationAction::ViewDiff],
        };

        let req = Peticion::AccionNotificacion {
            workspace_path: "/ws".into(),
            notification_id: "notif-1".into(),
            action: NotificationAction::Approve,
        };
        let json_req = serde_json::to_string(&req).expect("serialize req notif");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize req notif");
        assert_eq!(req, des_req);

        let event = Evento::ListaNotificaciones(vec![notif.clone()]);
        let json_ev = serde_json::to_string(&event).expect("serialize event notif");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize event notif");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_serializacion_antmesh() {
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

        let req = Peticion::ConectarPeer {
            workspace_path: "/ws".into(),
            address: "192.168.1.50:9042".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize mesh req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize mesh req");
        assert_eq!(req, des_req);

        let event = Evento::EstadoMesh(status.clone());
        let json_ev = serde_json::to_string(&event).expect("serialize mesh event");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize mesh event");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_serializacion_swarm() {
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

        let req = Peticion::DespacharRolRemoto {
            workspace_path: "/ws".into(),
            ticket_id: "T9.2".into(),
            role: AgentRole::Coder,
            node_id: Some("node-gpu-1".into()),
        };
        let json_req = serde_json::to_string(&req).expect("serialize swarm req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize swarm req");
        assert_eq!(req, des_req);

        let event = Evento::EstadoSwarm(swarm_status.clone());
        let json_ev = serde_json::to_string(&event).expect("serialize swarm event");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize swarm event");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_serializacion_vfs() {
        let entry = VfsEntry {
            path: "/antfs/symbols/structs/MeshStatus".into(),
            name: "MeshStatus".into(),
            is_dir: false,
            size: 420,
            node_type: "Symbol".into(),
        };

        let req = Peticion::ConsultarVfs {
            workspace_path: "/ws".into(),
            virtual_path: "/antfs/symbols".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize vfs req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize vfs req");
        assert_eq!(req, des_req);

        let event = Evento::ListadoVfs {
            virtual_path: "/antfs/symbols".into(),
            entries: vec![entry],
        };
        let json_ev = serde_json::to_string(&event).expect("serialize vfs event");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize vfs event");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_serializacion_vfs_guard() {
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

        let req = Peticion::ValidarEscrituraVfs {
            file_path: "src/lib.rs".into(),
            content: "fn main() {}".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize guard req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize guard req");
        assert_eq!(req, des_req);

        let ev1 = Evento::ResultadoValidacionVfs(val_res);
        let json_ev1 = serde_json::to_string(&ev1).expect("serialize val res");
        let des_ev1: Evento = serde_json::from_str(&json_ev1).expect("deserialize val res");
        assert_eq!(ev1, des_ev1);

        let ev2 = Evento::EstadoGuardVfs(guard_status);
        let json_ev2 = serde_json::to_string(&ev2).expect("serialize guard status");
        let des_ev2: Evento = serde_json::from_str(&json_ev2).expect("deserialize guard status");
        assert_eq!(ev2, des_ev2);
    }

    #[test]
    fn test_serializacion_ebpf() {
        let status = EbpfStatus {
            available: true,
            lsm_enabled: true,
            active_probes: vec!["bprm_check_security".into(), "file_open".into(), "socket_connect".into()],
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
            violation_reason: Some("Out-of-blast-radius network egress attempt blocked by eBPF LSM".into()),
        };

        let req = Peticion::ConsultarEbpfStatus {
            workspace_path: "/workspace".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize ebpf req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize ebpf req");
        assert_eq!(req, des_req);

        let ev1 = Evento::EstadoEbpf(status);
        let json_ev1 = serde_json::to_string(&ev1).expect("serialize ebpf status");
        let des_ev1: Evento = serde_json::from_str(&json_ev1).expect("deserialize ebpf status");
        assert_eq!(ev1, des_ev1);

        let ev2 = Evento::AuditLogEbpf(vec![event]);
        let json_ev2 = serde_json::to_string(&ev2).expect("serialize ebpf log");
        let des_ev2: Evento = serde_json::from_str(&json_ev2).expect("deserialize ebpf log");
        assert_eq!(ev2, des_ev2);
    }

    #[test]
    fn test_serializacion_profiler() {
        let hotspot = ProfileHotspot {
            name: "calculate_embeddings".into(),
            percentage_cpu: 64.5,
            percentage_memory: 32.1,
            calls_or_samples: 150,
        };
        let suggestion = ProfileSuggestion {
            kind: ProfileSuggestionKind::CpuOptimization,
            title: "Evitar clonado superfluo en cálculo de embeddings".into(),
            description: "El buffer se clona dentro del bucle de cálculo. Reemplazar por paso por referencia (&[f32]).".into(),
            potential_impact: "Alto (-45% CPU)".into(),
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

        let req = Peticion::EjecutarProfiler {
            workspace_path: "/ws".into(),
            command: "cargo test".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize profiler req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize profiler req");
        assert_eq!(req, des_req);

        let ev1 = Evento::ReporteProfiler(report);
        let json_ev1 = serde_json::to_string(&ev1).expect("serialize report");
        let des_ev1: Evento = serde_json::from_str(&json_ev1).expect("deserialize report");
        assert_eq!(ev1, des_ev1);
    }

    #[test]
    fn test_serializacion_lsp() {
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

        let req = Peticion::ConsultarLspStatus {
            workspace_path: "/Users/juandevelop/Develop/antOS".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize lsp status req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize lsp status req");
        assert_eq!(req, des_req);

        let ev1 = Evento::EstadoLsp(status);
        let json_ev1 = serde_json::to_string(&ev1).expect("serialize lsp status ev");
        let des_ev1: Evento = serde_json::from_str(&json_ev1).expect("deserialize lsp status ev");
        assert_eq!(ev1, des_ev1);

        let ev2 = Evento::ConfiguracionLsp {
            editor: LspEditorKind::Neovim,
            config_content: "vim.lsp.start({ name = 'antos-lsp', cmd = {'antos', 'lsp'} })".into(),
            target_file: "init.lua".into(),
        };
        let json_ev2 = serde_json::to_string(&ev2).expect("serialize lsp config ev");
        let des_ev2: Evento = serde_json::from_str(&json_ev2).expect("deserialize lsp config ev");
        assert_eq!(ev2, des_ev2);
    }

    #[test]
    fn test_serializacion_collab_y_dap() {
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

        let req_collab = Peticion::IniciarCollabSession {
            file_path: "src/main.rs".into(),
            ticket_id: Some("T12.2".into()),
            workspace_path: "/workspace".into(),
        };
        let json_req = serde_json::to_string(&req_collab).expect("serialize collab req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize collab req");
        assert_eq!(req_collab, des_req);

        let ev_collab = Evento::EstadoCollabSession(collab_status);
        let json_ev = serde_json::to_string(&ev_collab).expect("serialize collab ev");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize collab ev");
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

        let ev_dap = Evento::EstadoDapSession(dap_status);
        let json_dap = serde_json::to_string(&ev_dap).expect("serialize dap ev");
        let des_dap: Evento = serde_json::from_str(&json_dap).expect("deserialize dap ev");
        assert_eq!(ev_dap, des_dap);
    }

    #[test]
    fn test_serializacion_desktop() {
        let hotkeys = vec![
            DesktopHotkey {
                key: "Super+Space".into(),
                action: "toggle_intent_bar".into(),
                description: "Abrir o enfocar la barra de intenciones".into(),
            },
            DesktopHotkey {
                key: "Super+A".into(),
                action: "toggle_agent_center".into(),
                description: "Abrir Centro de Control de Agentes".into(),
            },
        ];

        let status = DesktopSessionStatus {
            running: true,
            compositor_name: "labwc".into(),
            wayland_display: Some("wayland-0".into()),
            active_clients_count: 3,
            registered_hotkeys: hotkeys.clone(),
        };

        let req_status = Peticion::ConsultarDesktopStatus;
        let json_req = serde_json::to_string(&req_status).expect("serialize desktop req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize desktop req");
        assert_eq!(req_status, des_req);

        let ev_status = Evento::EstadoDesktop(status);
        let json_ev = serde_json::to_string(&ev_status).expect("serialize desktop ev");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize desktop ev");
        assert_eq!(ev_status, des_ev);

        let ev_keys = Evento::ListaDesktopHotkeys(hotkeys);
        let json_keys = serde_json::to_string(&ev_keys).expect("serialize keys ev");
        let des_keys: Evento = serde_json::from_str(&json_keys).expect("deserialize keys ev");
        assert_eq!(ev_keys, des_keys);
    }

    #[test]
    fn test_serializacion_barra_telemetry() {
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

        let req = Peticion::ConsultarBarraTelemetry;
        let json_req = serde_json::to_string(&req).expect("serialize barra req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize barra req");
        assert_eq!(req, des_req);

        let ev = Evento::EstadoBarraTelemetry(telemetry);
        let json_ev = serde_json::to_string(&ev).expect("serialize barra ev");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize barra ev");
        assert_eq!(ev, des_ev);

        let alert = BarraAlert {
            category: "ebpf".into(),
            message: "Acceso denegado a /root/.ssh/id_rsa".into(),
            urgent: true,
        };
        let req_alert = Peticion::EmitirBarraAlert(alert.clone());
        let json_alert = serde_json::to_string(&req_alert).expect("serialize alert req");
        let des_alert: Peticion = serde_json::from_str(&json_alert).expect("deserialize alert req");
        assert_eq!(req_alert, des_alert);
    }

    #[test]
    fn test_serializacion_boot_pipeline() {
        let status = BootPipelineStatus {
            kernel_elf_exists: true,
            kernel_elf_size_bytes: 3314112,
            bios_image_exists: true,
            bios_image_size_bytes: 35651584,
            qemu_installed: true,
            target_arch: "x86_64-unknown-none".into(),
        };

        let req = Peticion::ConsultarBootStatus;
        let json_req = serde_json::to_string(&req).expect("serialize boot req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize boot req");
        assert_eq!(req, des_req);

        let req_exec = Peticion::EjecutarBootPipeline {
            action: "build".into(),
            headless: true,
        };
        let json_exec = serde_json::to_string(&req_exec).expect("serialize boot exec");
        let des_exec: Peticion = serde_json::from_str(&json_exec).expect("deserialize boot exec");
        assert_eq!(req_exec, des_exec);

        let ev = Evento::EstadoBoot(status);
        let json_ev = serde_json::to_string(&ev).expect("serialize boot ev");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize boot ev");
        assert_eq!(ev, des_ev);
    }

    #[test]
    fn test_serializacion_wasm_plugins() {
        let summary = PluginSummary {
            name: "markdown-formatter".into(),
            version: "1.0.0".into(),
            description: "Formatea tablas y encabezados Markdown".into(),
            capabilities: vec!["format".into(), "lint".into()],
            wasm_size_bytes: 40960,
        };

        let ev_list = Evento::ListaPlugins(vec![summary]);
        let json_ev_list = serde_json::to_string(&ev_list).expect("serialize list ev");
        let des_ev_list: Evento = serde_json::from_str(&json_ev_list).expect("deserialize list ev");
        assert_eq!(ev_list, des_ev_list);

        let req_list = Peticion::ListarPlugins;
        let json_req_list = serde_json::to_string(&req_list).expect("serialize list plugins");
        let des_req_list: Peticion = serde_json::from_str(&json_req_list).expect("deserialize list plugins");
        assert_eq!(req_list, des_req_list);

        let mut params = std::collections::BTreeMap::new();
        params.insert("target".into(), "README.md".into());
        let req_run = Peticion::EjecutarPlugin {
            plugin_name: "markdown-formatter".into(),
            action: "format".into(),
            params,
        };
        let json_run = serde_json::to_string(&req_run).expect("serialize run plugin");
        let des_run: Peticion = serde_json::from_str(&json_run).expect("deserialize run plugin");
        assert_eq!(req_run, des_run);

        let result = PluginResult {
            plugin: "markdown-formatter".into(),
            action: "format".into(),
            output: "Formateo exitoso".into(),
            fuel_consumed: 1250,
            memory_allocated_bytes: 65536,
            success: true,
            error: None,
        };
        let ev = Evento::ResultadoPlugin(result);
        let json_ev = serde_json::to_string(&ev).expect("serialize result ev");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize result ev");
        assert_eq!(ev, des_ev);
    }

    #[test]
    fn test_serializacion_visual_qa() {
        assert_eq!(AgentRole::VisualQA.name(), "Visual QA");
        assert_eq!(AgentRole::VisualQA.nombre(), "QA Visual");

        let req_cap = Peticion::CapturarPantalla {
            target: Some("firefox".into()),
            save_path: Some("/tmp/screenshot.png".into()),
        };
        let json_cap = serde_json::to_string(&req_cap).expect("serialize cap req");
        let des_cap: Peticion = serde_json::from_str(&json_cap).expect("deserialize cap req");
        assert_eq!(req_cap, des_cap);

        let report = VisualQAReport {
            target: "antOS-Barra".into(),
            image_width: 1920,
            image_height: 1080,
            image_size_bytes: 204800,
            findings: vec![VisualFinding {
                category: "alignment".into(),
                severity: "warning".into(),
                description: "Margen derecho desfasado 4px en badge de eBPF".into(),
                coordinates: Some("x: 1840, y: 12, w: 60, h: 24".into()),
                recommendation: "Alinear padding-right a 8px en estilo.css".into(),
            }],
            pass: false,
            summary: "1 advertencia visual detectada".into(),
        };

        let ev_rep = Evento::ReporteVisualQA(report);
        let json_rep = serde_json::to_string(&ev_rep).expect("serialize rep ev");
        let des_rep: Evento = serde_json::from_str(&json_rep).expect("deserialize rep ev");
        assert_eq!(ev_rep, des_rep);
    }

    #[test]
    fn test_serializacion_storage_installer() {
        let req_list = Peticion::ListarDiscos;
        let json_list = serde_json::to_string(&req_list).expect("serialize list req");
        let des_list: Peticion = serde_json::from_str(&json_list).expect("deserialize list req");
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

        let ev_dev = Evento::ListaDiscos(vec![dev.clone()]);
        let json_ev = serde_json::to_string(&ev_dev).expect("serialize list ev");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize list ev");
        assert_eq!(ev_dev, des_ev);

        let plan = PartitionPlan {
            target_device: "/dev/nvme0n1".into(),
            clean_install: true,
            efi_partition_bytes: 536870912,
            root_partition_bytes: 900000000000,
            swap_partition_bytes: 17179869184,
            aligned_start_sector: 2048,
            warnings: vec!["El disco se formateará por completo".into()],
        };

        let ev_plan = Evento::PlanParticionamiento(plan);
        let json_plan = serde_json::to_string(&ev_plan).expect("serialize plan ev");
        let des_plan: Evento = serde_json::from_str(&json_plan).expect("deserialize plan ev");
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
        let req_install = Peticion::InstalarSistema(cfg.clone());
        let json_ins = serde_json::to_string(&req_install).expect("serialize install req");
        let des_ins: Peticion = serde_json::from_str(&json_ins).expect("deserialize install req");
        assert_eq!(req_install, des_ins);

        let report = InstallReport {
            target_device: "/dev/sda".into(),
            mode: "dual-boot".into(),
            success: true,
            steps: vec![InstallStep {
                name: "mount".into(),
                description: "Montaje de particiones".into(),
                completed: true,
            }],
            efi_partition: "/dev/sda1".into(),
            root_partition: "/dev/sda3".into(),
            fstab_entries: vec!["UUID=123 / ext4 defaults 0 1".into()],
            summary: "Instalación completada".into(),
        };
        let ev_rep = Evento::ReporteInstalacion(report);
        let json_rep = serde_json::to_string(&ev_rep).expect("serialize rep ev");
        let des_rep: Evento = serde_json::from_str(&json_rep).expect("deserialize rep ev");
        assert_eq!(ev_rep, des_rep);

        let os = OsEntry {
            name: "Windows Boot Manager".into(),
            os_type: "windows".into(),
            efi_path: "\\EFI\\Microsoft\\Boot\\bootmgfw.efi".into(),
            disk_device: "/dev/nvme0n1".into(),
            partition_number: 1,
        };
        let ev_os = Evento::SistemasOperativosDetectados(vec![os.clone()]);
        let json_os = serde_json::to_string(&ev_os).expect("serialize os ev");
        let des_os: Evento = serde_json::from_str(&json_os).expect("deserialize os ev");
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
        let req_boot = Peticion::InstalarBootloader(boot_cfg);
        let json_boot = serde_json::to_string(&req_boot).expect("serialize boot req");
        let des_boot: Peticion = serde_json::from_str(&json_boot).expect("deserialize boot req");
        assert_eq!(req_boot, des_boot);
    }
}

