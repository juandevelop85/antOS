//! Serialización de herramientas de desarrollo (TDD, CI, snapshots,
//! benchmarks, forge) y de los modelos del kernel (scheduler, framebuffer,
//! almacenamiento, syscalls, arranque Limine, ramdisk).
use crate::*;

#[test]
fn test_submodule_namespaces_and_roundtrips() {
    use crate::dev::{DapSessionStatus, LspServerStatus};
    use crate::flow::{AgentRole, FlowState, FlowTask};
    use crate::git::{GitFileDiffSummary, GitFileStatus, GitRepoStatus};
    use crate::ipc::{Event, Request};
    use crate::mesh::{MeshStatus, NodeResources, PeerNode};
    use crate::plan::{Plan, Step, Tier};
    use crate::spec::{TicketDetail, TicketStatus, TicketSummary};
    use crate::system::VfsStatus;
    use crate::vm::{MicrovmConfig, MicrovmStatus};
    use crate::wasm::PluginSummary;

    // 1. plan
    let mut args = std::collections::BTreeMap::new();
    args.insert("path".into(), "src/main.rs".into());
    let plan = Plan {
        id: "plan-1".into(),
        intent: "Build feature".into(),
        planner: "agent-alpha".into(),
        steps: vec![Step {
            capability: "fs".into(),
            args,
        }],
    };
    let json_plan = serde_json::to_string(&plan).expect("plan serde");
    let des_plan: Plan = serde_json::from_str(&json_plan).expect("plan deser");
    assert_eq!(plan, des_plan);

    let tier = Tier::Auto;
    let json_tier = serde_json::to_string(&tier).expect("tier serde");
    let des_tier: Tier = serde_json::from_str(&json_tier).expect("tier deser");
    assert_eq!(tier, des_tier);

    // 2. git
    let git_status = GitRepoStatus {
        branch: Some("main".into()),
        clean: true,
        modified: vec![GitFileDiffSummary {
            path: "src/lib.rs".into(),
            added_lines: 10,
            deleted_lines: 2,
            status: GitFileStatus::Modified,
        }],
        ..Default::default()
    };
    let json_git = serde_json::to_string(&git_status).expect("git serde");
    let des_git: GitRepoStatus = serde_json::from_str(&json_git).expect("git deser");
    assert_eq!(git_status, des_git);

    // 3. flow
    let role = AgentRole::Architect;
    let json_role = serde_json::to_string(&role).expect("role serde");
    let des_role: AgentRole = serde_json::from_str(&json_role).expect("role deser");
    assert_eq!(role, des_role);

    let task = FlowTask {
        id: "task-10".into(),
        ticket_id: "T22.2".into(),
        state: FlowState::Implementing,
        current_role: Some(AgentRole::Coder),
        worktree_path: None,
        branch_name: Some("feature/modular".into()),
        qa_retries: 0,
        max_qa_retries: 3,
        diff_preview: None,
        audit_summary: None,
        history: vec![],
        backend: FlowBackend::Simulated,
    };
    let json_task = serde_json::to_string(&task).expect("task serde");
    let des_task: FlowTask = serde_json::from_str(&json_task).expect("task deser");
    assert_eq!(task, des_task);

    // 4. spec
    let ticket = TicketSummary {
        id: "T22.2".into(),
        phase: "Fase 22".into(),
        title: "Modular Protocol".into(),
        status: TicketStatus::InProgress,
        file_path: "docs/tickets/T22.2.md".into(),
    };
    let json_ticket = serde_json::to_string(&ticket).expect("ticket serde");
    let des_ticket: TicketSummary = serde_json::from_str(&json_ticket).expect("ticket deser");
    assert_eq!(ticket, des_ticket);

    let detail = TicketDetail {
        id: "T22.2".into(),
        phase: "Fase 22".into(),
        title: "Modular Protocol".into(),
        status: TicketStatus::InProgress,
        file_path: "docs/tickets/T22.2.md".into(),
        description: "Decompose into submodules".into(),
        technical_scope: vec!["crates".into()],
        acceptance_criteria: vec!["Zero breaking changes".into()],
    };
    let json_detail = serde_json::to_string(&detail).expect("detail serde");
    let des_detail: TicketDetail = serde_json::from_str(&json_detail).expect("detail deser");
    assert_eq!(detail, des_detail);

    // 5. mesh
    let node_res = NodeResources {
        cpu_cores: 8,
        memory_mb: 16384,
        vram_mb: Some(8192),
        available_models: vec!["qwen2.5-coder".into()],
    };
    let peer = PeerNode {
        id: "peer-1".into(),
        hostname: "dev-laptop".into(),
        address: "192.168.1.50:9000".into(),
        latency_ms: 5,
        connected: true,
        resources: node_res,
        last_seen_secs: 1725440000,
    };
    let mesh = MeshStatus {
        local_node: peer.clone(),
        peers: vec![peer],
    };
    let json_mesh = serde_json::to_string(&mesh).expect("mesh serde");
    let des_mesh: MeshStatus = serde_json::from_str(&json_mesh).expect("mesh deser");
    assert_eq!(mesh, des_mesh);

    // 6. wasm
    let plugin = PluginSummary {
        name: "Linter".into(),
        version: "0.1.0".into(),
        description: "Rust linter plugin".into(),
        capabilities: vec!["fs_read".into()],
        wasm_size_bytes: 1024,
    };
    let json_plugin = serde_json::to_string(&plugin).expect("plugin serde");
    let des_plugin: PluginSummary = serde_json::from_str(&json_plugin).expect("plugin deser");
    assert_eq!(plugin, des_plugin);

    // 7. dev
    let lsp = LspServerStatus {
        running: true,
        transport: "ipc".into(),
        socket_path: Some("/tmp/lsp.sock".into()),
        connected_clients: 1,
        active_workspace: "/workspace".into(),
        indexed_symbols_count: 50,
        capabilities: vec!["hover".into()],
    };
    let json_lsp = serde_json::to_string(&lsp).expect("lsp serde");
    let des_lsp: LspServerStatus = serde_json::from_str(&json_lsp).expect("lsp deser");
    assert_eq!(lsp, des_lsp);

    let dap = DapSessionStatus {
        session_id: "dap-1".into(),
        target_command: "cargo run".into(),
        state: "running".into(),
        breakpoints: vec![],
        current_line: Some(10),
        call_stack: vec!["main".into()],
        variables: vec![],
        simulated: true,
    };
    let json_dap = serde_json::to_string(&dap).expect("dap serde");
    let des_dap: DapSessionStatus = serde_json::from_str(&json_dap).expect("dap deser");
    assert_eq!(dap, des_dap);

    // 8. vm
    let vm = MicrovmConfig {
        vm_id: "vm-1".into(),
        vcpu_count: 2,
        memory_mb: 512,
        kernel_image: "/boot/vmlinux".into(),
        initrd_image: None,
        overlay_disk: None,
        vsock_port: 5252,
        command: Some("/bin/sh".into()),
    };
    let json_vm = serde_json::to_string(&vm).expect("vm serde");
    let des_vm: MicrovmConfig = serde_json::from_str(&json_vm).expect("vm deser");
    assert_eq!(vm, des_vm);

    let vm_status = MicrovmStatus {
        kvm_available: true,
        hypervisor_engine: "cloud-hypervisor".into(),
        active_vms_count: 1,
        total_memory_allocated_mb: 512,
        vsock_supported: true,
        kernel_version: "6.6.0".into(),
    };
    let json_vm_status = serde_json::to_string(&vm_status).expect("vm_status serde");
    let des_vm_status: MicrovmStatus =
        serde_json::from_str(&json_vm_status).expect("vm_status deser");
    assert_eq!(vm_status, des_vm_status);

    // 9. system
    let vfs = VfsStatus {
        mount_point: Some("/workspace/vfs".into()),
        is_mounted: true,
        total_symbols: 100,
        total_modules: 12,
    };
    let json_vfs = serde_json::to_string(&vfs).expect("vfs serde");
    let des_vfs: VfsStatus = serde_json::from_str(&json_vfs).expect("vfs deser");
    assert_eq!(vfs, des_vfs);

    // 10. ipc
    let req = Request::QueryGitStatus {
        workspace_path: "/workspace".into(),
        project: None,
    };
    let json_req = serde_json::to_string(&req).expect("req serde");
    let des_req: Request = serde_json::from_str(&json_req).expect("req deser");
    assert_eq!(req, des_req);

    let ev = Event::GitStatus(git_status);
    let json_ev = serde_json::to_string(&ev).expect("ev serde");
    let des_ev: Event = serde_json::from_str(&json_ev).expect("ev deser");
    assert_eq!(ev, des_ev);
}

#[test]
fn test_preemptive_scheduler_and_pcb_tcb_serde() {
    let pcb = ProcessInfo {
        pid: 1,
        name: "antos-init".into(),
        page_table_root: 0x1000_0000,
        state: ProcessState::Running,
        threads: vec![1, 2],
        exit_code: None,
        memory_bytes: 65536,
    };
    let json_pcb = serde_json::to_string(&pcb).expect("serialize pcb");
    let des_pcb: ProcessInfo = serde_json::from_str(&json_pcb).expect("deserialize pcb");
    assert_eq!(pcb, des_pcb);

    let tcb = ThreadInfo {
        tid: 1,
        pid: 1,
        state: ThreadState::Running,
        priority: 10,
        user_sp: 0x7000_0000,
        kernel_sp: 0xFFFF_8000_2000_0000,
        total_ticks: 42,
        time_slice_remaining: 2,
    };
    let json_tcb = serde_json::to_string(&tcb).expect("serialize tcb");
    let des_tcb: ThreadInfo = serde_json::from_str(&json_tcb).expect("deserialize tcb");
    assert_eq!(tcb, des_tcb);

    let stats = SchedulerStats {
        total_processes: 2,
        active_processes: 2,
        total_threads: 3,
        ready_threads: 2,
        context_switches: 150,
        quantum_ticks: 2,
        preemption_enabled: true,
    };
    let json_stats = serde_json::to_string(&stats).expect("serialize stats");
    let des_stats: SchedulerStats = serde_json::from_str(&json_stats).expect("deserialize stats");
    assert_eq!(stats, des_stats);
}

#[test]
fn test_framebuffer_and_console_models() {
    let fb = FramebufferInfoModel {
        width: 1280,
        height: 720,
        stride: 1280,
        bytes_per_pixel: 3,
        format: FramebufferFormat::Bgr,
    };
    let json_fb = serde_json::to_string(&fb).expect("serialize fb info");
    let des_fb: FramebufferInfoModel = serde_json::from_str(&json_fb).expect("deserialize fb info");
    assert_eq!(fb, des_fb);

    let console = ConsoleInfoModel {
        cols: 160,
        rows: 45,
        cursor_col: 10,
        cursor_row: 5,
        font_width: 8,
        font_height: 16,
        ansi_enabled: true,
    };
    let json_cons = serde_json::to_string(&console).expect("serialize console info");
    let des_cons: ConsoleInfoModel =
        serde_json::from_str(&json_cons).expect("deserialize console info");
    assert_eq!(console, des_cons);
}

#[test]
fn test_storage_and_kernel_vfs_models() {
    let blk = BlockDeviceStats {
        device_type: StorageDeviceType::VirtioBlk,
        capacity_sectors: 3098,
        sector_size: 512,
        capacity_bytes: 3098 * 512,
        pci_address: Some("0:4.0".into()),
    };
    let json_blk = serde_json::to_string(&blk).expect("serialize blk stats");
    let des_blk: BlockDeviceStats = serde_json::from_str(&json_blk).expect("deserialize blk stats");
    assert_eq!(blk, des_blk);

    let mount = KernelVfsMount {
        mount_point: "/".into(),
        fs_type: "tarfs".into(),
        total_files: 4,
        read_only: true,
    };
    let json_mount = serde_json::to_string(&mount).expect("serialize mount");
    let des_mount: KernelVfsMount = serde_json::from_str(&json_mount).expect("deserialize mount");
    assert_eq!(mount, des_mount);

    let node = KernelVfsNode {
        path: "/bin/init".into(),
        size: 790672,
        is_dir: false,
    };
    let json_node = serde_json::to_string(&node).expect("serialize node");
    let des_node: KernelVfsNode = serde_json::from_str(&json_node).expect("deserialize node");
    assert_eq!(node, des_node);
}

#[test]
fn test_syscall_and_microkernel_ipc_models() {
    let sc = SyscallTableInfo {
        total_syscalls: 12,
        active_architecture: "x86_64".into(),
        user_address_limit: 0x0000_0100_0000_0000,
        smap_efault_protection: true,
    };
    let json_sc = serde_json::to_string(&sc).expect("serialize syscall table info");
    let des_sc: SyscallTableInfo =
        serde_json::from_str(&json_sc).expect("deserialize syscall table info");
    assert_eq!(sc, des_sc);

    let chan = IpcChannelInfo {
        channel_id: 1,
        pending_messages: 2,
        max_messages: 32,
        max_message_size: 1024,
        waiting_receiver: false,
    };
    let json_chan = serde_json::to_string(&chan).expect("serialize ipc channel info");
    let des_chan: IpcChannelInfo =
        serde_json::from_str(&json_chan).expect("deserialize ipc channel info");
    assert_eq!(chan, des_chan);

    let msg = IpcMessageSummary {
        channel_id: 1,
        sender_pid: 2,
        payload_size: 64,
    };
    let json_msg = serde_json::to_string(&msg).expect("serialize ipc message summary");
    let des_msg: IpcMessageSummary =
        serde_json::from_str(&json_msg).expect("deserialize ipc message summary");
    assert_eq!(msg, des_msg);
}

#[test]
fn test_limine_boot_config_and_hybrid_report() {
    let default_cfg = LimineBootConfig::default();
    assert_eq!(default_cfg.timeout_seconds, 3);
    assert_eq!(default_cfg.protocol, "limine");
    assert_eq!(default_cfg.kernel_path, "boot():/KERNEL.ELF");
    assert_eq!(default_cfg.resolution, "1280x720x32");

    let json_cfg = serde_json::to_string(&default_cfg).expect("serialize limine config");
    let des_cfg: LimineBootConfig =
        serde_json::from_str(&json_cfg).expect("deserialize limine config");
    assert_eq!(default_cfg, des_cfg);

    let report = HybridBootImageReport {
        success: true,
        image_path: "/target/antos-uefi-x86_64.img".into(),
        architecture: "x86_64".into(),
        format: "gpt-esp-hybrid".into(),
        efi_bootloader: "BOOTX64.EFI".into(),
        pe_signature_valid: true,
        limine_conf_generated: true,
        esp_size_bytes: 67108864,
        summary: "Hybrid UEFI GPT / BIOS Limine image generated successfully".into(),
    };
    let json_rep = serde_json::to_string(&report).expect("serialize hybrid report");
    let des_rep: HybridBootImageReport =
        serde_json::from_str(&json_rep).expect("deserialize hybrid report");
    assert_eq!(report, des_rep);
}

#[test]
fn test_live_ramdisk_config_and_manifest() {
    let cfg = LiveRamdiskConfig::default();
    assert_eq!(cfg.format, "ustar");
    assert_eq!(cfg.compression, "none");
    assert_eq!(cfg.target_path, "boot():/initrd.img");
    assert!(cfg.include_tools.contains(&"/bin/sh".to_string()));
    assert!(cfg.include_tools.contains(&"/bin/parted".to_string()));
    assert!(cfg.include_tools.contains(&"/bin/mkfs.ext4".to_string()));
    assert!(cfg.include_configs.contains(&"/etc/os-release".to_string()));
    assert!(cfg.mount_points.contains(&"/proc".to_string()));

    let json_cfg = serde_json::to_string(&cfg).expect("serialize ramdisk config");
    let des_cfg: LiveRamdiskConfig =
        serde_json::from_str(&json_cfg).expect("deserialize ramdisk config");
    assert_eq!(cfg, des_cfg);

    let manifest = LiveRamdiskManifest {
        hostname: "antos-live".into(),
        os_name: "antOS".into(),
        version: "0.1.0-alpha".into(),
        total_entries: 20,
        total_size_bytes: 2097152,
        has_init: true,
        tools_count: 6,
        summary: "Live Ramdisk USTAR rootfs packaged with all standard tools".into(),
    };
    let json_m = serde_json::to_string(&manifest).expect("serialize ramdisk manifest");
    let des_m: LiveRamdiskManifest =
        serde_json::from_str(&json_m).expect("deserialize ramdisk manifest");
    assert_eq!(manifest, des_m);
}

#[test]
fn test_storage_subsystem_serialization() {
    let sda = StorageDeviceInfo {
        device_node: "/dev/sda".into(),
        interface: StorageInterfaceKind::AhciSata,
        model: "Samsung SSD 870 EVO 500GB".into(),
        sector_size: 512,
        total_sectors: 976773168,
        capacity_bytes: 500107862016,
        read_only: false,
    };

    let nvme0n1 = StorageDeviceInfo {
        device_node: "/dev/nvme0n1".into(),
        interface: StorageInterfaceKind::Nvme,
        model: "WD_BLACK SN850X 1000GB".into(),
        sector_size: 4096,
        total_sectors: 244190646,
        capacity_bytes: 1000204886016,
        read_only: false,
    };

    let status = StorageSubsystemStatus {
        total_devices: 2,
        ahci_controllers_found: 1,
        nvme_controllers_found: 1,
        virtio_devices_found: 0,
        devices: vec![sda.clone(), nvme0n1.clone()],
    };

    let json_status = serde_json::to_string(&status).expect("serialize storage subsystem status");
    let des_status: StorageSubsystemStatus =
        serde_json::from_str(&json_status).expect("deserialize storage subsystem status");
    assert_eq!(status, des_status);
    assert_eq!(des_status.devices.len(), 2);
    assert_eq!(
        des_status.devices[0].interface,
        StorageInterfaceKind::AhciSata
    );
    assert_eq!(des_status.devices[1].interface, StorageInterfaceKind::Nvme);
}

#[test]
fn test_app_management_serialization() {
    let app = DesktopApp {
        id: "com.visualstudio.code".to_string(),
        name: "Visual Studio Code".to_string(),
        version: "1.93.0".to_string(),
        source: AppSource::Flatpak,
        description: "Code editing. Redefined.".to_string(),
        icon: Some("com.visualstudio.code".to_string()),
        categories: vec!["Development".to_string(), "IDE".to_string()],
        permissions: vec!["network".to_string(), "wayland".to_string()],
        installed: true,
        exec_cmd: "flatpak run com.visualstudio.code".to_string(),
    };

    let json_app = serde_json::to_string(&app).expect("serialize app");
    let des_app: DesktopApp = serde_json::from_str(&json_app).expect("deserialize app");
    assert_eq!(app, des_app);
    assert_eq!(des_app.source, AppSource::Flatpak);
    assert_eq!(des_app.source.as_str(), "flatpak");

    let search_res = AppSearchResult {
        id: "org.mozilla.firefox".to_string(),
        name: "Firefox".to_string(),
        version: "130.0".to_string(),
        source: AppSource::Flatpak,
        description: "Fast, Private & Safe Web Browser".to_string(),
        installed: false,
    };
    let json_sr = serde_json::to_string(&search_res).expect("serialize search_res");
    let des_sr: AppSearchResult = serde_json::from_str(&json_sr).expect("deserialize search_res");
    assert_eq!(search_res, des_sr);

    let progress = AppProgress {
        app_id: "com.visualstudio.code".to_string(),
        percentage: 65.5,
        status: "Downloading runtime...".to_string(),
        done: false,
    };
    let json_prog = serde_json::to_string(&progress).expect("serialize progress");
    let des_prog: AppProgress = serde_json::from_str(&json_prog).expect("deserialize progress");
    assert_eq!(progress.app_id, des_prog.app_id);
    assert_eq!(progress.percentage, des_prog.percentage);

    let launch_res = AppLaunchResult {
        app_id: "com.visualstudio.code".to_string(),
        pid: Some(12345),
        workspace: Some("/home/developer/workspace".to_string()),
        success: true,
        message: "Application launched successfully".to_string(),
    };
    let json_launch = serde_json::to_string(&launch_res).expect("serialize launch_res");
    let des_launch: AppLaunchResult =
        serde_json::from_str(&json_launch).expect("deserialize launch_res");
    assert_eq!(launch_res, des_launch);

    // Requests
    let req_list = Request::ListApps {
        source: Some(AppSource::Flatpak),
    };
    let json_req = serde_json::to_string(&req_list).expect("serialize req_list");
    let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req_list");
    assert_eq!(req_list, des_req);

    let req_search = Request::SearchApps {
        query: "code".to_string(),
    };
    let json_search = serde_json::to_string(&req_search).expect("serialize req_search");
    let des_search: Request = serde_json::from_str(&json_search).expect("deserialize req_search");
    assert_eq!(req_search, des_search);

    let req_install = Request::InstallApp {
        id: "com.visualstudio.code".to_string(),
        source: Some(AppSource::Flatpak),
    };
    let json_install = serde_json::to_string(&req_install).expect("serialize req_install");
    let des_install: Request =
        serde_json::from_str(&json_install).expect("deserialize req_install");
    assert_eq!(req_install, des_install);

    let req_launch = Request::LaunchApp {
        id: "com.visualstudio.code".to_string(),
        workspace: Some("/home/developer/workspace".to_string()),
        args: vec![".".to_string()],
    };
    let json_launch_req = serde_json::to_string(&req_launch).expect("serialize req_launch");
    let des_launch_req: Request =
        serde_json::from_str(&json_launch_req).expect("deserialize req_launch");
    assert_eq!(req_launch, des_launch_req);

    // Events
    let ev_list = Event::AppList(vec![app.clone()]);
    let json_ev = serde_json::to_string(&ev_list).expect("serialize ev_list");
    let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize ev_list");
    assert_eq!(ev_list, des_ev);

    let ev_prog = Event::AppProgress(progress);
    let json_prog_ev = serde_json::to_string(&ev_prog).expect("serialize ev_prog");
    let des_prog_ev: Event = serde_json::from_str(&json_prog_ev).expect("deserialize ev_prog");
    assert_eq!(ev_prog, des_prog_ev);
}

#[test]
fn test_package_catalog_requests_serialization() {
    let req_search = Request::SearchPackages {
        query: "browser".to_string(),
    };
    let json_search = serde_json::to_string(&req_search).expect("serialize req_search");
    let des_search: Request = serde_json::from_str(&json_search).expect("deserialize req_search");
    assert_eq!(req_search, des_search);

    let req_info = Request::GetPackageInfo {
        recipe: "firefox".to_string(),
    };
    let json_info = serde_json::to_string(&req_info).expect("serialize req_info");
    let des_info: Request = serde_json::from_str(&json_info).expect("deserialize req_info");
    assert_eq!(req_info, des_info);

    let manifest = PackageManifest {
            name: "firefox".to_string(),
            version: "130.0".to_string(),
            description: "Mozilla Firefox Web Browser".to_string(),
            homepage: Some("https://www.mozilla.org/firefox".to_string()),
            license: Some("MPL-2.0".to_string()),
            source_url: Some("https://download-installer.cdn.mozilla.net/pub/firefox/releases/130.0/linux-x86_64/en-US/firefox-130.0.tar.bz2".to_string()),
            sha256: Some("c6a1e1bf88ff842d0fa5716df166412be8fbeeeae397c88b2eb60980df94a9a0".to_string()),
            signature: None,
            signer_public_key: None,
            dependencies: Vec::new(),
            build_script: Some("install".to_string()),
            binaries: vec!["firefox".to_string()],
            app_type: PackageAppType::Gui,
            desktop_entry: None,
            icons: Vec::new(),
        };

    let ev_search = Event::PackageSearchResults(vec![manifest.clone()]);
    let json_ev_search = serde_json::to_string(&ev_search).expect("serialize ev_search");
    let des_ev_search: Event =
        serde_json::from_str(&json_ev_search).expect("deserialize ev_search");
    assert_eq!(ev_search, des_ev_search);

    let ev_info = Event::PackageInfo(manifest.clone());
    let json_ev_info = serde_json::to_string(&ev_info).expect("serialize ev_info");
    let des_ev_info: Event = serde_json::from_str(&json_ev_info).expect("deserialize ev_info");
    assert_eq!(ev_info, des_ev_info);
}
