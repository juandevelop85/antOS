//! El enum `Change`: cada capacidad de antOS se traduce, al planificarse,
//! en una lista de `Change` — la unidad que tanto la vista previa como
//! `apply()` entienden (T31.15: extraído de `exec/mod.rs`).
#![allow(unused_imports, dead_code)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum Change {
    Write {
        path: PathBuf,
        content: String,
    },
    Mkdir {
        path: PathBuf,
    },
    Delete {
        path: PathBuf,
    },
    Read {
        path: PathBuf,
    },
    GitStatus {
        repo_root: PathBuf,
    },
    GitCommit {
        repo_root: PathBuf,
        commit_msg: String,
    },
    GitBranch {
        repo_root: PathBuf,
        branch_name: String,
        base: Option<String>,
    },
    GitWorktreeCreate {
        repo_root: PathBuf,
        target_path: PathBuf,
        branch_name: String,
        base: String,
    },
    GitWorktreeCleanup {
        repo_root: PathBuf,
        target_path: PathBuf,
        force: bool,
    },
    GitWorktreeMerge {
        repo_root: PathBuf,
        branch_name: String,
        target_branch: String,
        message: Option<String>,
    },
    PortStatus {
        port: Option<u16>,
    },
    PortKill {
        port: u16,
        force: bool,
    },
    ServiceUp {
        service: String,
        port: Option<u16>,
        db_name: Option<String>,
        state_dir: PathBuf,
        workspace: PathBuf,
    },
    ServiceDown {
        service: String,
        state_dir: PathBuf,
    },
    ServiceStatus {
        service: Option<String>,
        state_dir: PathBuf,
    },
    SecretGrant {
        secret: String,
        minutes: i64,
        reason: Option<String>,
        grants_path: PathBuf,
    },
    SecretRevoke {
        secret: String,
        grants_path: PathBuf,
    },
    SecretList {
        state_dir: PathBuf,
        grants_path: PathBuf,
    },
    SecretSet {
        key: String,
        value: String,
        state_dir: PathBuf,
    },
    SecretRead {
        key: String,
        state_dir: PathBuf,
        grants_path: PathBuf,
    },
    TicketCreate {
        ticket_id: String,
        title: String,
        description: Option<String>,
        phase: Option<String>,
        workspace: PathBuf,
    },
    TicketUpdateStatus {
        ticket_id: String,
        status: String,
        workspace: PathBuf,
    },
    TicketList {
        workspace: PathBuf,
        filter: Option<String>,
    },
    MemoryIndex {
        workspace: PathBuf,
    },
    MemorySearch {
        workspace: PathBuf,
        query: String,
        limit: usize,
    },
    MemoryGraph {
        workspace: PathBuf,
        target: Option<String>,
    },
    EnvProfileInit {
        workspace: PathBuf,
        profile: Option<String>,
        create_devbox: bool,
        create_flake: bool,
    },
    EnvProfileSync {
        workspace: PathBuf,
    },
    EnvProfileStatus {
        workspace: PathBuf,
    },
    QuotaStatus {
        workspace: PathBuf,
    },
    QuotaSet {
        workspace: PathBuf,
        quota: crate::sandbox::quota::ResourceQuota,
    },
    UiDiffViewer {
        workspace: PathBuf,
        target: Option<String>,
        /// Optional specific developer project directory to diff (T17.2).
        /// When set, git diff runs in this directory with GIT_CEILING_DIRECTORIES
        /// to prevent leaking the antOS OS repository.
        project_path: Option<PathBuf>,
    },
    UiTerminal {
        command: Option<String>,
    },
    DevWorkspace {
        workspace: PathBuf,
        project: Option<String>,
        action: Option<String>,
    },
    NotifyList {
        workspace: PathBuf,
    },
    NotifyAction {
        workspace: PathBuf,
        notification_id: String,
        action: antos_protocol::NotificationAction,
    },
    MeshStatus {
        workspace: PathBuf,
    },
    MeshConnect {
        workspace: PathBuf,
        address: String,
    },
    MeshPair {
        workspace: PathBuf,
    },
    SwarmStatus {
        workspace: PathBuf,
    },
    SwarmDispatch {
        workspace: PathBuf,
        ticket_id: String,
        role: antos_protocol::AgentRole,
        node: Option<String>,
    },
    VfsQuery {
        workspace: PathBuf,
        path: Option<String>,
    },
    VfsMount {
        workspace: PathBuf,
        mount_point: Option<String>,
    },
    VfsUnmount {
        workspace: PathBuf,
        mount_point: Option<String>,
    },
    VfsValidateWrite {
        workspace: PathBuf,
        file_path: String,
        content: Option<String>,
    },
    VfsGuardStatus {
        workspace: PathBuf,
    },
    EbpfStatus {
        workspace: PathBuf,
    },
    EbpfAuditLog {
        workspace: PathBuf,
        limit: usize,
        pid: Option<u32>,
    },
    ProfileRun {
        workspace: PathBuf,
        command: String,
    },
    ProfileAnalyze {
        workspace: PathBuf,
    },
    LspStart {
        workspace: PathBuf,
        mode: String,
    },
    LspStatus {
        workspace: PathBuf,
    },
    CollabSession {
        workspace: PathBuf,
        file: String,
        ticket: Option<String>,
    },
    DapAttach {
        workspace: PathBuf,
        command: String,
    },
    DesktopSession {
        workspace: PathBuf,
        action: Option<String>,
    },
    DesktopKeys {
        workspace: PathBuf,
    },
    BarraStatus {
        workspace: PathBuf,
    },
    BarraNotify {
        workspace: PathBuf,
        category: String,
        message: String,
        urgent: bool,
    },
    BootPipeline {
        workspace: PathBuf,
        action: String,
    },
    PluginList {
        workspace: PathBuf,
    },
    PluginRun {
        workspace: PathBuf,
        plugin: String,
        action: String,
        params: std::collections::BTreeMap<String, String>,
    },
    PluginInstall {
        workspace: PathBuf,
        source_path: PathBuf,
    },
    UiScreenshot {
        workspace: PathBuf,
        target: Option<String>,
        path: Option<PathBuf>,
    },
    UiInspectVisual {
        workspace: PathBuf,
        target: String,
        criteria: Vec<String>,
    },
    DiskList {
        workspace: PathBuf,
    },
    DiskInspect {
        workspace: PathBuf,
        device: String,
    },
    DiskPartition {
        workspace: PathBuf,
        device: String,
        clean: bool,
        dry_run: bool,
    },
    InstallPrepare {
        workspace: PathBuf,
        target_device: String,
        target_mount: Option<String>,
    },
    InstallDeploy {
        workspace: PathBuf,
        config: antos_protocol::InstallConfig,
    },
    BootloaderProbe {
        workspace: PathBuf,
        esp_path: Option<String>,
    },
    BootloaderInstall {
        workspace: PathBuf,
        config: antos_protocol::BootloaderConfig,
    },
    MicrovmSpawn {
        state_dir: PathBuf,
        config: antos_protocol::MicrovmConfig,
    },
    MicrovmExec {
        state_dir: PathBuf,
        vm_id: String,
        command: String,
    },
    MicrovmDestroy {
        state_dir: PathBuf,
        vm_id: String,
    },
    /// Runs an ad hoc shell command on the host, confined by whatever
    /// recinto `sandbox::for_host()` offers on this platform (T31.4).
    ///
    /// This is the *only* place this command actually executes: it must
    /// only ever be submitted to `sandbox::run`, never applied directly by
    /// a broker process. `crate::vm::MicrovmManager::exec_vm` is the sole
    /// intended caller — see that module's doc comment for why a "MicroVM"
    /// exec ends up here instead of inside a hypervisor-isolated guest.
    HostShellExec {
        command: String,
    },
    PackageInstall {
        state_dir: PathBuf,
        package: String,
        dry_run: bool,
    },
    PackageRemove {
        state_dir: PathBuf,
        package: String,
    },
    PackageRollback {
        state_dir: PathBuf,
        generation: Option<u64>,
    },
    PackageList {
        state_dir: PathBuf,
    },
    PackageVerify {
        state_dir: PathBuf,
    },
    AutopilotStart {
        state_dir: PathBuf,
        workspace_dir: PathBuf,
        config: antos_protocol::AutopilotConfig,
    },
    AutopilotStop {
        state_dir: PathBuf,
        workspace_dir: PathBuf,
    },
    AutopilotStatus {
        state_dir: PathBuf,
        workspace_dir: PathBuf,
    },
    AutopilotScan {
        state_dir: PathBuf,
        workspace_dir: PathBuf,
    },
    AutopilotResolve {
        state_dir: PathBuf,
        workspace_dir: PathBuf,
        incident_id: String,
        approve: bool,
    },
    WebStart {
        state_dir: PathBuf,
        workspace_dir: PathBuf,
        config: antos_protocol::WebConsoleConfig,
    },
    WebStop {
        state_dir: PathBuf,
    },
    WebStatus {
        state_dir: PathBuf,
    },
    WebToken {
        state_dir: PathBuf,
        label: Option<String>,
        ttl: Option<u64>,
    },
    /// T17.3 — Initializes an isolated Git repository in a developer project
    /// under workspace/, with optional branch name and auto-detected .gitignore.
    ProjectGitInit {
        project_dir: PathBuf,
        branch: String,
        language_hint: Option<String>,
    },
    /// T20.2 — Autonomous bug reproduction and TDD verification.
    TestReproduce {
        workspace: PathBuf,
        state_dir: PathBuf,
        error_log: String,
        target_file: Option<String>,
    },
    /// T20.2 — Automated test suite generation for target source file or module.
    TestGen {
        workspace: PathBuf,
        target: String,
        suite_type: String,
        cases: usize,
    },
    /// T20.3 — Local parallel CI pipeline execution.
    CiRun {
        workspace: PathBuf,
        state_dir: PathBuf,
        stage: Option<String>,
        fast: bool,
    },
    /// T20.3 — Query last CI pipeline run status.
    CiStatus {
        state_dir: PathBuf,
    },
    /// T20.3 — Manage Git pre-commit and pre-push hooks.
    GitHookManage {
        workspace: PathBuf,
        action: String,
    },
    /// T20.4 — Create atomic dev environment snapshot.
    SnapshotCreate {
        workspace: PathBuf,
        state_dir: PathBuf,
        label: Option<String>,
        author: Option<String>,
    },
    /// T20.4 — List atomic dev environment snapshots.
    SnapshotList {
        state_dir: PathBuf,
    },
    /// T20.4 — Restore atomic dev environment snapshot.
    SnapshotRestore {
        workspace: PathBuf,
        state_dir: PathBuf,
        id_or_label: String,
        create_rescue: bool,
    },
    /// T20.4 — Delete atomic dev environment snapshot.
    SnapshotDelete {
        state_dir: PathBuf,
        id: String,
    },
    /// T21.1 — Run continuous microbenchmarks.
    BenchRun {
        workspace: PathBuf,
        state_dir: PathBuf,
        target: Option<String>,
    },
    /// T21.1 — Compare performance against baseline branch/worktree.
    BenchDiff {
        workspace: PathBuf,
        state_dir: PathBuf,
        against_branch: Option<String>,
        threshold_pct: Option<f64>,
    },
    /// T21.1 — Retrieve historical benchmark records.
    BenchHistory {
        state_dir: PathBuf,
    },
    /// T21.2 — List open remote issues.
    IssueList {
        workspace: PathBuf,
        state_dir: PathBuf,
    },
    /// T21.2 — Import remote issue to local technical ticket.
    IssueImport {
        workspace: PathBuf,
        state_dir: PathBuf,
        id: String,
    },
    /// T21.2 — Create and publish Pull Request.
    PrCreate {
        workspace: PathBuf,
        state_dir: PathBuf,
        title: Option<String>,
        base_branch: Option<String>,
        draft: bool,
    },
    /// T21.2 — Query Pull Request status.
    PrStatus {
        state_dir: PathBuf,
        number: Option<u64>,
    },
    /// T21.3 — Generate live architecture diagrams in Mermaid.
    DocArch {
        workspace: PathBuf,
        kind: Option<String>,
    },
    /// T21.3 — Synchronize architecture diagrams into markdown files.
    DocSync {
        workspace: PathBuf,
        target_file: Option<String>,
    },
    /// T21.3 — Check if architecture documentation is in sync with workspace.
    DocCheck {
        workspace: PathBuf,
        target_file: Option<String>,
    },
}
