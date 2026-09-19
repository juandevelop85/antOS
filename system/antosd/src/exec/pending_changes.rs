//! `PendingChanges`: lo que el plan ya ha decidido escribir o borrar,
//! antes de haberlo hecho — para que dos pasos que tocan el mismo
//! fichero en el mismo plan se compongan en vez de pisarse
//! (T31.15: extraído de `exec/mod.rs`).
#![allow(unused_imports, dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::Change;

/// Lo que el plan ya ha decidido escribir, antes de haberlo escrito.
///
/// Sin esto, dos pasos que tocan el mismo fichero se pisan: cada uno lo lee
/// del disco tal y como estaba ANTES del plan, y al ejecutar gana el último.
/// Declarar tres dependencias dejaba una.
///
/// Y lo grave no era perder dos líneas: era que el diff aprobado dejaba de
/// describir el resultado. Que lo que ves sea lo que pasa es la propiedad
/// que sostiene todo lo demás.
#[derive(Default)]
pub struct PendingChanges {
    writes: BTreeMap<PathBuf, String>,
    deletes: std::collections::BTreeSet<PathBuf>,
}

impl PendingChanges {
    /// El contenido que tendrá el fichero cuando llegue este paso.
    /// `None` significa «pregúntale al disco».
    pub fn read(&self, path: &Path) -> Option<String> {
        if self.deletes.contains(path) {
            return Some(String::new());
        }
        self.writes.get(path).cloned()
    }

    pub fn apply(&mut self, change: &Change) {
        match change {
            Change::Write { path, content } => {
                self.deletes.remove(path);
                self.writes.insert(path.clone(), content.clone());
            }
            Change::Delete { path } => {
                self.writes.remove(path);
                self.deletes.insert(path.clone());
            }
            Change::Mkdir { .. }
            | Change::Read { .. }
            | Change::Patch { .. }
            | Change::ListDir { .. }
            | Change::TestRun { .. }
            | Change::StackCommand { .. }
            | Change::GitStatus { .. }
            | Change::GitCommit { .. }
            | Change::GitBranch { .. }
            | Change::GitWorktreeCreate { .. }
            | Change::GitWorktreeCleanup { .. }
            | Change::GitWorktreeMerge { .. }
            | Change::PortStatus { .. }
            | Change::PortKill { .. }
            | Change::ServiceUp { .. }
            | Change::ServiceDown { .. }
            | Change::ServiceStatus { .. }
            | Change::SecretGrant { .. }
            | Change::SecretRevoke { .. }
            | Change::SecretList { .. }
            | Change::SecretSet { .. }
            | Change::SecretRead { .. }
            | Change::TicketCreate { .. }
            | Change::TicketUpdateStatus { .. }
            | Change::TicketList { .. }
            | Change::MemoryIndex { .. }
            | Change::MemorySearch { .. }
            | Change::MemoryGraph { .. }
            | Change::EnvProfileInit { .. }
            | Change::EnvProfileSync { .. }
            | Change::EnvProfileStatus { .. }
            | Change::QuotaStatus { .. }
            | Change::QuotaSet { .. }
            | Change::UiDiffViewer { .. }
            | Change::UiTerminal { .. }
            | Change::DevWorkspace { .. }
            | Change::NotifyList { .. }
            | Change::NotifyAction { .. }
            | Change::MeshStatus { .. }
            | Change::MeshConnect { .. }
            | Change::MeshPair { .. }
            | Change::SwarmStatus { .. }
            | Change::SwarmDispatch { .. }
            | Change::VfsQuery { .. }
            | Change::VfsMount { .. }
            | Change::VfsUnmount { .. }
            | Change::VfsValidateWrite { .. }
            | Change::VfsGuardStatus { .. }
            | Change::EbpfStatus { .. }
            | Change::EbpfAuditLog { .. }
            | Change::ProfileRun { .. }
            | Change::ProfileAnalyze { .. }
            | Change::LspStart { .. }
            | Change::LspStatus { .. }
            | Change::CollabSession { .. }
            | Change::DapAttach { .. }
            | Change::DesktopSession { .. }
            | Change::DesktopKeys { .. }
            | Change::BarraStatus { .. }
            | Change::BarraNotify { .. }
            | Change::BootPipeline { .. }
            | Change::PluginList { .. }
            | Change::PluginRun { .. }
            | Change::PluginInstall { .. }
            | Change::UiScreenshot { .. }
            | Change::UiInspectVisual { .. }
            | Change::DiskList { .. }
            | Change::DiskInspect { .. }
            | Change::DiskPartition { .. }
            | Change::InstallPrepare { .. }
            | Change::InstallDeploy { .. }
            | Change::BootloaderProbe { .. }
            | Change::BootloaderInstall { .. }
            | Change::MicrovmSpawn { .. }
            | Change::MicrovmExec { .. }
            | Change::MicrovmDestroy { .. }
            | Change::HostShellExec { .. }
            | Change::PackageInstall { .. }
            | Change::PackageRemove { .. }
            | Change::PackageRollback { .. }
            | Change::PackageList { .. }
            | Change::PackageVerify { .. }
            | Change::AutopilotStart { .. }
            | Change::AutopilotStop { .. }
            | Change::AutopilotStatus { .. }
            | Change::AutopilotScan { .. }
            | Change::AutopilotResolve { .. }
            | Change::WebStart { .. }
            | Change::WebStop { .. }
            | Change::WebStatus { .. }
            | Change::WebToken { .. }
            | Change::ProjectGitInit { .. }
            | Change::TestReproduce { .. }
            | Change::TestGen { .. }
            | Change::CiRun { .. }
            | Change::CiStatus { .. }
            | Change::GitHookManage { .. }
            | Change::SnapshotCreate { .. }
            | Change::SnapshotList { .. }
            | Change::SnapshotRestore { .. }
            | Change::SnapshotDelete { .. }
            | Change::BenchRun { .. }
            | Change::BenchDiff { .. }
            | Change::BenchHistory { .. }
            | Change::IssueList { .. }
            | Change::IssueImport { .. }
            | Change::PrCreate { .. }
            | Change::PrStatus { .. }
            | Change::DocArch { .. }
            | Change::DocSync { .. }
            | Change::DocCheck { .. } => {}
        }
    }
}

/// Lee un fichero teniendo en cuenta lo que el plan ya ha decidido.
fn read_with_pending(path: &Path, pending: &PendingChanges) -> String {
    pending
        .read(path)
        .unwrap_or_else(|| std::fs::read_to_string(path).unwrap_or_default())
}
