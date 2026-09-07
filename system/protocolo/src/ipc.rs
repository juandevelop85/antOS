//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};

use crate::plan::*;
use crate::git::*;
use crate::flow::*;
use crate::spec::*;
use crate::mesh::*;
use crate::wasm::*;
use crate::dev::*;
use crate::vm::*;
use crate::system::*;

// ============================================================================
// IPC Messages, Requests, Events and Serialization Codecs
// ============================================================================

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
    /// Query structured syntax diff for ticket, file, or commit (T8.1 / T17.2).
    #[serde(alias = "ConsultarDiff")]
    QueryDiff {
        workspace_path: String,
        target: Option<String>,
        /// Optional project name or absolute path inside workspace/ to restrict the diff
        /// to a specific developer project, preventing leakage from the antOS OS repo.
        project_path: Option<String>,
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
    /// Install package from local recipe or package name (T16.2).
    #[serde(alias = "InstallPackage")]
    InstallPackage {
        recipe_path_or_name: String,
        dry_run: bool,
    },
    /// Remove package from current active profile (T16.2).
    #[serde(alias = "RemovePackage")]
    RemovePackage {
        package_name: String,
    },
    /// List packages installed in current active profile (T16.2).
    #[serde(alias = "ListPackages")]
    ListPackages,
    /// Rollback package profile to a previous generation (T16.2).
    #[serde(alias = "RollbackPackage")]
    RollbackPackage {
        target_generation: Option<u64>,
    },
    /// Verify SHA-256 checksums and signatures of installed packages (T16.2).
    #[serde(alias = "VerifyPackages")]
    VerifyPackages,
    /// Query global immutable package store status (T16.2).
    #[serde(alias = "QueryPackageStoreStatus")]
    QueryPackageStoreStatus,
    /// List graphical desktop applications registered in current active profile (T25.1).
    #[serde(alias = "ListDesktopApps", alias = "list_desktop_apps")]
    ListDesktopApps,
    /// Validate Freedesktop .desktop entry syntax (T25.1).
    #[serde(alias = "ValidateDesktopEntry", alias = "validate_desktop_entry")]
    ValidateDesktopEntry {
        content: String,
    },
    /// Start continuous autonomous sentinel agent daemon (T16.3).
    #[serde(alias = "IniciarAutopilot")]
    StartAutopilot(AutopilotConfig),
    /// Stop continuous autonomous sentinel agent daemon (T16.3).
    #[serde(alias = "DetenerAutopilot")]
    StopAutopilot,
    /// Query status and metrics of the autonomous sentinel (T16.3).
    #[serde(alias = "ConsultarAutopilotStatus")]
    GetAutopilotStatus,
    /// List detected, investigating, and resolved incidents (T16.3).
    #[serde(alias = "ListarAutopilotIncidentes")]
    ListAutopilotIncidents,
    /// Manually trigger a workspace scan for flaws or broken builds (T16.3).
    #[serde(alias = "EscanearAutopilot")]
    ScanAutopilot,
    /// Resolve or reject an autonomous fix proposal (T16.3).
    #[serde(alias = "ResolverAutopilotIncidente")]
    ResolveAutopilotIncident {
        incident_id: String,
        approve_and_merge: bool,
    },
    /// Start embedded HTTP/WebSocket web console server (T16.4).
    #[serde(alias = "IniciarWebConsole")]
    StartWebConsole(WebConsoleConfig),
    /// Stop embedded HTTP/WebSocket web console server (T16.4).
    #[serde(alias = "DetenerWebConsole")]
    StopWebConsole,
    /// Query operational status of the embedded web console (T16.4).
    #[serde(alias = "ConsultarWebConsoleStatus")]
    GetWebConsoleStatus,
    /// Generate a cryptographic session token for secure web access (T16.4).
    #[serde(alias = "GenerarTokenWeb")]
    GenerateWebToken {
        client_label: Option<String>,
        ttl_secs: Option<u64>,
    },
    /// Query status and layout configuration of the Dev TUI workspace (T20.1).
    #[serde(alias = "ConsultarDevWorkspace")]
    GetDevWorkspaceStatus {
        project: Option<String>,
    },
    /// Ingest stack trace and execute autonomous TDD bug reproduction (T20.2).
    #[serde(alias = "ReproducirBug")]
    ReproduceBug {
        error_text: String,
        target_file: Option<String>,
    },
    /// Generate unit/regression test cases for a target function or file (T20.2).
    #[serde(alias = "GenerarTest")]
    GenerateTest {
        target: String,
    },
    /// Execute local parallel CI/CD pipeline (T20.3).
    #[serde(alias = "EjecutarCi")]
    RunCi {
        stage: Option<String>,
        fast: bool,
    },
    /// Query status and metrics of the last local CI run (T20.3).
    #[serde(alias = "ConsultarEstadoCi")]
    GetCiStatus,
    /// Manage Git hooks (install, uninstall, check) (T20.3).
    #[serde(alias = "GestionarGitHooks")]
    ManageGitHooks {
        action: String,
    },
    /// Create an atomic development environment snapshot (T20.4).
    #[serde(alias = "CrearSnapshot")]
    CreateSnapshot {
        label: Option<String>,
        author: Option<String>,
    },
    /// List all atomic development environment snapshots (T20.4).
    #[serde(alias = "ListarSnapshots")]
    ListSnapshots,
    /// Restore an atomic snapshot reverting workspace and runtime state (T20.4).
    #[serde(alias = "RestaurarSnapshot")]
    RestoreSnapshot {
        id_or_label: String,
        create_rescue: bool,
    },
    /// Delete an atomic development snapshot (T20.4).
    #[serde(alias = "EliminarSnapshot")]
    DeleteSnapshot {
        id: String,
    },
    /// Run continuous benchmarks on project (T21.1).
    #[serde(alias = "EjecutarBenchmark")]
    RunBenchmark {
        target: Option<String>,
    },
    /// Compare performance against a baseline branch or previous run (T21.1).
    #[serde(alias = "CompararBenchmark")]
    CompareBenchmark {
        against_branch: Option<String>,
        threshold_pct: Option<u32>,
    },
    /// Retrieve historical benchmark runs (T21.1).
    #[serde(alias = "ConsultarHistorialBenchmark")]
    GetBenchmarkHistory,
    /// List open issues from remote Git forge (T21.2).
    #[serde(alias = "ListarIssuesRemotos")]
    ListRemoteIssues,
    /// Import a remote issue and generate technical ticket in docs/tickets/ (T21.2).
    #[serde(alias = "ImportarIssueRemoto")]
    ImportRemoteIssue {
        id_or_url: String,
    },
    /// Create and publish a Pull Request / Merge Request to remote forge (T21.2).
    #[serde(alias = "CrearPullRequest")]
    CreatePullRequest {
        title: Option<String>,
        base_branch: Option<String>,
        draft: bool,
    },
    /// Query status and CI checks of a Pull Request (T21.2).
    #[serde(alias = "ConsultarPullRequest")]
    GetPullRequestStatus {
        number: Option<u64>,
    },
    /// Generate live architecture diagrams in Mermaid format (T21.3).
    #[serde(alias = "GenerarDiagramaArquitectura")]
    GenerateArchDiagram {
        kind: Option<String>,
    },
    /// Synchronize architecture Mermaid diagrams in markdown documentation (T21.3).
    #[serde(alias = "SincronizarDocumentacionArquitectura")]
    SyncArchDocs {
        target_file: Option<String>,
    },
    /// Check whether architecture documentation is in sync with workspace code (T21.3).
    #[serde(alias = "VerificarDocumentacionArquitectura")]
    CheckArchDocs {
        target_file: Option<String>,
    },
}

/// Backwards compatibility type alias.
#[deprecated(note = "use Request")]
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
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
    /// Package installation and compilation report (T16.2).
    #[serde(alias = "ReportePaquete")]
    PackageInstallReport(PackageInstallReport),
    /// List of packages in active profile or store (T16.2).
    #[serde(alias = "ListaPaquetes")]
    PackageList(Vec<PackageSummary>),
    /// Status and statistics of immutable package store (T16.2).
    #[serde(alias = "EstadoStore")]
    PackageStoreStatus(PackageStoreStatus),
    /// List of profile generations available for rollback (T16.2).
    #[serde(alias = "ListaGeneracionesPaquetes")]
    PackageGenerationsList(Vec<PackageGeneration>),
    /// Result of package integrity and cryptographic verification (T16.2).
    #[serde(alias = "ResultadoVerificacionPaquetes")]
    PackageVerificationResult {
        all_valid: bool,
        verified_packages: usize,
        details: Vec<String>,
    },
    /// List of registered graphical desktop applications (T25.1).
    #[serde(alias = "ListaAplicacionesEscritorio")]
    DesktopAppList(Vec<DesktopAppSummary>),
    /// Verification report for a Freedesktop .desktop entry (T25.1).
    #[serde(alias = "ReporteValidacionEscritorio")]
    DesktopValidationReport(DesktopValidationReport),
    /// Real-time status of the Autopilot daemon (T16.3).
    #[serde(alias = "EstadoAutopilot")]
    AutopilotStatus(AutopilotStatus),
    /// List of tracked autopilot incidents and fix proposals (T16.3).
    #[serde(alias = "ListaIncidentesAutopilot")]
    AutopilotIncidentsList(Vec<AutopilotIncident>),
    /// Asynchronous desktop alert emitted when an incident is detected or fix is ready (T16.3).
    #[serde(alias = "AlertaAutopilot")]
    AutopilotAlert(AutopilotIncident),
    /// Operational status of the embedded web console (T16.4).
    #[serde(alias = "EstadoWebConsole")]
    WebConsoleStatus(WebConsoleStatus),
    /// Emitted when a web access token is generated (T16.4).
    #[serde(alias = "TokenWebGenerado")]
    WebTokenGenerated(WebAuthSession),
    /// Status of the integrated Dev TUI workspace (T20.1).
    #[serde(alias = "EstadoDevWorkspace")]
    DevWorkspaceStatus(DevWorkspaceStatus),
    /// Outcome of autonomous bug reproduction and TDD verification (T20.2).
    #[serde(alias = "ReporteTdd")]
    TddReport(TddRegressionReport),
    /// Consolidated local CI report outcome (T20.3).
    #[serde(alias = "ReporteCi")]
    CiReport(CiReport),
    /// Git hooks installation and guard status (T20.3).
    #[serde(alias = "EstadoGitHooks")]
    GitHooksStatus(GitHookStatus),
    /// Notification when an atomic snapshot is created (T20.4).
    #[serde(alias = "SnapshotCreado")]
    SnapshotCreated(DevSnapshotMetadata),
    /// Chronological list of atomic snapshots (T20.4).
    #[serde(alias = "ListaSnapshots")]
    SnapshotsList(Vec<DevSnapshotMetadata>),
    /// Notification when an atomic snapshot is restored (T20.4).
    #[serde(alias = "SnapshotRestaurado")]
    SnapshotRestored(SnapshotRestoreResult),
    /// Notification when an atomic snapshot is deleted (T20.4).
    #[serde(alias = "SnapshotEliminado")]
    SnapshotDeleted {
        id: String,
    },
    /// Outcome of a benchmark suite run (T21.1).
    #[serde(alias = "ReporteBenchmark")]
    BenchmarkReport(BenchmarkRunReport),
    /// Outcome of a performance differential comparison (T21.1).
    #[serde(alias = "DiferencialBenchmark")]
    BenchmarkDiff(BenchmarkDiffReport),
    /// Historical list of benchmark suite reports (T21.1).
    #[serde(alias = "HistorialBenchmark")]
    BenchmarkHistory(Vec<BenchmarkRunReport>),
    /// List of issues retrieved from remote forge (T21.2).
    #[serde(alias = "ListaIssuesRemotos")]
    RemoteIssuesList(Vec<RemoteIssue>),
    /// Confirmation of imported issue as local ticket (T21.2).
    #[serde(alias = "IssueRemotoImportado")]
    RemoteIssueImported {
        ticket_id: String,
        path: String,
        title: String,
    },
    /// Confirmation of Pull Request created (T21.2).
    #[serde(alias = "PullRequestCreado")]
    PullRequestCreated(RemotePullRequest),
    /// Inspection report for Pull Request status (T21.2).
    #[serde(alias = "EstadoPullRequest")]
    PullRequestStatus(PullRequestStatusReport),
    /// Generated architecture diagrams in Mermaid (T21.3).
    #[serde(alias = "DiagramaArquitectura")]
    ArchDiagram(ArchDiagramReport),
    /// Result of architecture documentation sync or check (T21.3).
    #[serde(alias = "SincronizacionDocumentacion")]
    DocSync(DocSyncReport),
    /// General error message.
    #[serde(alias = "Error")]
    Error(String),
}

/// Backwards compatibility type aliases.
#[deprecated(note = "use Event")]
pub type Evento = Event;
#[deprecated(note = "use Request")]
pub type Mensaje = Request;
#[deprecated(note = "use Event")]
pub type Respuesta = Event;
pub type Message = Request;
pub type Response = Event;

