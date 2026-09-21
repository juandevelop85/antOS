//! Serialización de VFS Guard, eBPF, profiler, LSP, DAP, desktop y barra.
use crate::*;

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
        simulated: true,
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
        keymap: "us".into(),
        system: "x86_64-linux".into(),
        password_hash: None,
        locale: "en_US.UTF-8".into(),
        encrypt: false,
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
            executed: false,
        }],
        efi_partition: "/dev/sda1".into(),
        root_partition: "/dev/sda3".into(),
        fstab_entries: vec!["UUID=123 / ext4 defaults 0 1".into()],
        summary: "Installation simulated".into(),
        simulated: true,
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
        esp_mount: "/boot".into(),
        target_device: "/dev/nvme0n1".into(),
        efi_partition: 1,
        default_os: "antos".into(),
        timeout_seconds: 5,
        detected_os: vec![os],
        dry_run: true,
        target: BootTarget::BareMetal,
        efi_binary: None,
    };
    let req_boot = Request::InstallBootloader(boot_cfg);
    let json_boot = serde_json::to_string(&req_boot).expect("serialize boot req");
    let des_boot: Request = serde_json::from_str(&json_boot).expect("deserialize boot req");
    assert_eq!(req_boot, des_boot);
}

/// T36.1 added `executed`, `simulated`, `system`, `target` and `efi_binary`
/// with serde defaults: a report or config written before the change still
/// parses, and the defaults are the honest ones (`executed = false`,
/// `simulated = false`, `target = nix-os`).
#[test]
fn test_installer_types_pre_t36_1_wire_format_still_parses() {
    let step: InstallStep = serde_json::from_str(
        r#"{"name":"mount","description":"Partition mounting","completed":true}"#,
    )
    .expect("old InstallStep");
    assert!(step.completed);
    assert!(!step.executed);

    let report: InstallReport = serde_json::from_str(
        r#"{"target_device":"/dev/sda","mode":"clean","success":true,"steps":[],
            "efi_partition":"/dev/sda1","root_partition":"/dev/sda2",
            "fstab_entries":[],"summary":"ok"}"#,
    )
    .expect("old InstallReport");
    assert!(!report.simulated);

    let cfg: InstallConfig = serde_json::from_str(
        r#"{"target_device":"/dev/sda","clean_install":true,"target_mount":"/mnt",
            "hostname":"h","username":"u","timezone":"UTC","dry_run":true}"#,
    )
    .expect("old InstallConfig");
    assert_eq!(cfg.keymap, "us");
    assert!(cfg.system == "x86_64-linux" || cfg.system == "aarch64-linux");
    assert!(cfg.password_hash.is_none());
    assert_eq!(cfg.locale, "en_US.UTF-8");
    assert!(!cfg.encrypt);
    // Sin hash no se serializa el campo: el formato no cambia para nadie.
    assert!(!serde_json::to_string(&cfg)
        .expect("json")
        .contains("password_hash"));

    let boot: BootloaderConfig = serde_json::from_str(
        r#"{"esp_mount":"/boot","target_device":"/dev/sda","efi_partition":1,
            "default_os":"antos","timeout_seconds":5,"detected_os":[],"dry_run":true}"#,
    )
    .expect("old BootloaderConfig");
    assert_eq!(boot.target, BootTarget::NixOs);
    assert!(boot.efi_binary.is_none());
    assert_eq!(
        serde_json::to_value(BootTarget::BareMetal).expect("json"),
        serde_json::json!("bare-metal")
    );
}

#[test]
fn test_usb_types_serialization() {
    let usb_dev = UsbDeviceInfo {
        path: "/dev/sdb".into(),
        vendor: "SanDisk".into(),
        model: "Ultra USB 3.0".into(),
        size_bytes: 32 * 1024 * 1024 * 1024,
        bus_type: "usb".into(),
        is_removable: true,
        is_system_disk: false,
        mount_points: vec!["/media/usb".into()],
    };
    let json_dev = serde_json::to_string(&usb_dev).expect("serialize usb_dev");
    let des_dev: UsbDeviceInfo = serde_json::from_str(&json_dev).expect("deserialize usb_dev");
    assert_eq!(usb_dev, des_dev);

    let build_rep = UsbBuildReport {
        success: true,
        iso_path: "/tmp/antos-live.iso".into(),
        sha256_path: "/tmp/antos-live.iso.sha256".into(),
        sha256_checksum: "abcd1234ef".into(),
        architecture: "x86_64".into(),
        size_bytes: 750000000,
        summary: "ISO build succeeded".into(),
    };
    let json_bld = serde_json::to_string(&build_rep).expect("serialize build_rep");
    let des_bld: UsbBuildReport = serde_json::from_str(&json_bld).expect("deserialize build_rep");
    assert_eq!(build_rep, des_bld);

    let flash_rep = UsbFlashReport {
        success: true,
        target_device: "/dev/sdb".into(),
        image_path: "/tmp/antos-live.iso".into(),
        bytes_written: 750000000,
        sha256_checksum: "abcd1234ef".into(),
        duration_seconds: 15.2,
        average_speed_mbps: 49.34,
        verified: true,
        summary: "USB flashed and verified successfully".into(),
    };
    let json_fls = serde_json::to_string(&flash_rep).expect("serialize flash_rep");
    let des_fls: UsbFlashReport = serde_json::from_str(&json_fls).expect("deserialize flash_rep");
    assert_eq!(flash_rep, des_fls);
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
