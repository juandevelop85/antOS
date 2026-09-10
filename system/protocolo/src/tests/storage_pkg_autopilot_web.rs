//! Serialización de almacenamiento, instalador, microVMs, paquetes,
//! autopilot y consola web.
use crate::*;

#[test]
fn test_package_types_serialization() {
    let manifest = PackageManifest {
        name: "ripgrep".into(),
        version: "14.1.0".into(),
        description: "Fast line-oriented search tool".into(),
        homepage: Some("https://github.com/BurntSushi/ripgrep".into()),
        license: Some("MIT".into()),
        source_url: Some("https://github.com/BurntSushi/ripgrep/archive/14.1.0.tar.gz".into()),
        sha256: Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into()),
        signature: Some("sig123abc".into()),
        signer_public_key: Some("pubkey456def".into()),
        dependencies: vec!["pcre2".into()],
        build_script: Some("cargo build --release".into()),
        binaries: vec!["rg".into()],
        app_type: PackageAppType::Cli,
        desktop_entry: None,
        icons: Vec::new(),
    };
    let json_manifest = serde_json::to_string(&manifest).expect("serialize manifest");
    let des_manifest: PackageManifest =
        serde_json::from_str(&json_manifest).expect("deserialize manifest");
    assert_eq!(manifest, des_manifest);

    // GUI package manifest with desktop entry and icons (T25.1)
    let gui_manifest = PackageManifest {
        name: "firefox".into(),
        version: "130.0".into(),
        description: "Mozilla Firefox Web Browser".into(),
        homepage: Some("https://mozilla.org/firefox".into()),
        license: Some("MPL-2.0".into()),
        source_url: Some("https://packages.antos.dev/sources/firefox-130.0.tar.gz".into()),
        sha256: Some("abcdef123456".into()),
        signature: None,
        signer_public_key: None,
        dependencies: vec!["gtk4".into(), "wayland".into()],
        build_script: None,
        binaries: vec!["firefox".into()],
        app_type: PackageAppType::Gui,
        desktop_entry: Some(DesktopEntryManifest {
            name: "Mozilla Firefox".into(),
            generic_name: Some("Web Browser".into()),
            comment: Some("Navegador web libre y seguro".into()),
            exec: "firefox %u".into(),
            icon: Some("firefox".into()),
            categories: vec!["Network".into(), "WebBrowser".into()],
            mime_types: vec!["text/html".into(), "x-scheme-handler/http".into()],
            terminal: false,
            startup_wm_class: Some("firefox".into()),
        }),
        icons: vec![IconAsset {
            resolution: "scalable".into(),
            format: "svg".into(),
            path: "share/icons/hicolor/scalable/apps/firefox.svg".into(),
        }],
    };
    let json_gui = serde_json::to_string(&gui_manifest).expect("serialize gui manifest");
    let des_gui: PackageManifest =
        serde_json::from_str(&json_gui).expect("deserialize gui manifest");
    assert_eq!(gui_manifest, des_gui);

    let req_install = Request::InstallPackage {
        recipe_path_or_name: "ripgrep".into(),
        dry_run: true,
    };
    let json_install = serde_json::to_string(&req_install).expect("serialize install req");
    let des_install: Request =
        serde_json::from_str(&json_install).expect("deserialize install req");
    assert_eq!(req_install, des_install);

    let req_desktop = Request::ListDesktopApps;
    let json_desktop_req = serde_json::to_string(&req_desktop).expect("serialize list desktop req");
    let des_desktop_req: Request =
        serde_json::from_str(&json_desktop_req).expect("deserialize list desktop req");
    assert_eq!(req_desktop, des_desktop_req);

    let req_validate = Request::ValidateDesktopEntry {
        content: "[Desktop Entry]\nType=Application\nName=App\nExec=app\n".into(),
    };
    let json_val_req = serde_json::to_string(&req_validate).expect("serialize val req");
    let des_val_req: Request = serde_json::from_str(&json_val_req).expect("deserialize val req");
    assert_eq!(req_validate, des_val_req);

    let summary = PackageSummary {
        name: "ripgrep".into(),
        version: "14.1.0".into(),
        description: "Fast line-oriented search tool".into(),
        store_hash: "a1b2c3d4e5f6".into(),
        installed_size_bytes: 5242880,
        installed_at: "2026-09-03T08:00:00Z".into(),
        binaries: vec!["rg".into()],
        generation: 1,
        app_type: PackageAppType::Cli,
        desktop_entry: None,
        desktop_file: None,
        icons_linked: Vec::new(),
    };
    let ev_list = Event::PackageList(vec![summary.clone()]);
    let json_list = serde_json::to_string(&ev_list).expect("serialize package list ev");
    let des_list: Event = serde_json::from_str(&json_list).expect("deserialize package list ev");
    assert_eq!(ev_list, des_list);

    let app_summary = DesktopAppSummary {
        id: "firefox".into(),
        name: "Mozilla Firefox".into(),
        generic_name: Some("Web Browser".into()),
        comment: Some("Navegador web libre".into()),
        exec: "firefox %u".into(),
        icon: Some("firefox".into()),
        icon_path: Some("/var/antos/current/share/icons/hicolor/scalable/apps/firefox.svg".into()),
        categories: vec!["Network".into(), "WebBrowser".into()],
        mime_types: vec!["text/html".into()],
        desktop_file_path: "/var/antos/current/share/applications/firefox.desktop".into(),
        package_name: "firefox".into(),
        package_version: "130.0".into(),
    };
    let ev_apps = Event::DesktopAppList(vec![app_summary.clone()]);
    let json_apps = serde_json::to_string(&ev_apps).expect("serialize apps list ev");
    let des_apps: Event = serde_json::from_str(&json_apps).expect("deserialize apps list ev");
    assert_eq!(ev_apps, des_apps);

    let report = PackageInstallReport {
        name: "ripgrep".into(),
        version: "14.1.0".into(),
        store_path: "/var/antos/store/a1b2c3d4e5f6-ripgrep-14.1.0".into(),
        generation: 1,
        binaries_linked: vec!["rg".into()],
        desktop_entries_linked: Vec::new(),
        icons_linked: Vec::new(),
        checksum_verified: true,
        signature_verified: true,
        success: true,
        message: "Installed successfully".into(),
    };
    let ev_report = Event::PackageInstallReport(report);
    let json_rep = serde_json::to_string(&ev_report).expect("serialize report ev");
    let des_rep: Event = serde_json::from_str(&json_rep).expect("deserialize report ev");
    assert_eq!(ev_report, des_rep);

    let status = PackageStoreStatus {
        store_path: "/var/antos/store".into(),
        current_profile_path: "/var/antos/current".into(),
        current_generation: 1,
        total_packages: 1,
        total_store_bytes: 5242880,
        generations_count: 1,
    };
    let ev_status = Event::PackageStoreStatus(status);
    let json_status = serde_json::to_string(&ev_status).expect("serialize store status ev");
    let des_status: Event =
        serde_json::from_str(&json_status).expect("deserialize store status ev");
    assert_eq!(ev_status, des_status);

    let gen = PackageGeneration {
        generation: 1,
        timestamp: "2026-09-03T08:00:00Z".into(),
        packages: vec!["ripgrep@14.1.0".into()],
        active: true,
    };
    let ev_gens = Event::PackageGenerationsList(vec![gen]);
    let json_gens = serde_json::to_string(&ev_gens).expect("serialize gens ev");
    let des_gens: Event = serde_json::from_str(&json_gens).expect("deserialize gens ev");
    assert_eq!(ev_gens, des_gens);

    let ev_verify = Event::PackageVerificationResult {
        all_valid: true,
        verified_packages: 1,
        details: vec!["ripgrep: SHA256 OK".into()],
    };
    let json_verify = serde_json::to_string(&ev_verify).expect("serialize verify ev");
    let des_verify: Event = serde_json::from_str(&json_verify).expect("deserialize verify ev");
    assert_eq!(ev_verify, des_verify);
}

#[test]
fn test_autopilot_types_serialization() {
    let cfg = AutopilotConfig {
        enabled: true,
        poll_interval_secs: 10,
        watch_paths: vec!["src".into()],
        auto_merge: false,
        target_branch: "master".into(),
    };
    let req_start = Request::StartAutopilot(cfg.clone());
    let json_start = serde_json::to_string(&req_start).expect("serialize start req");
    let des_start: Request = serde_json::from_str(&json_start).expect("deserialize start req");
    assert_eq!(req_start, des_start);

    let status = AutopilotStatus {
        active: true,
        workspace_path: "/home/dev/project".into(),
        poll_interval_secs: 10,
        active_incidents_count: 1,
        resolved_incidents_count: 2,
        last_scan_timestamp: Some("2026-09-03T08:30:00Z".into()),
    };
    let ev_status = Event::AutopilotStatus(status.clone());
    let json_status = serde_json::to_string(&ev_status).expect("serialize status ev");
    let des_status: Event = serde_json::from_str(&json_status).expect("deserialize status ev");
    assert_eq!(ev_status, des_status);

    let proposal = AutopilotFixProposal {
        incident_id: "inc-1".into(),
        branch: "autopilot/inc-1".into(),
        title: "Fix syntax error in main.rs".into(),
        diff: "+ fn test() {}".into(),
        test_output: "test result: ok. 1 passed".into(),
        reviewed_by_auditor: true,
    };

    let incident = AutopilotIncident {
        id: "inc-1".into(),
        timestamp: "2026-09-03T08:30:00Z".into(),
        incident_type: "SyntaxError".into(),
        severity: "high".into(),
        file_path: "src/main.rs".into(),
        error_message: "Unclosed delimiter".into(),
        status: "ready_for_approval".into(),
        worktree_branch: Some("autopilot/inc-1".into()),
        fix_proposal: Some(proposal),
    };

    let ev_alert = Event::AutopilotAlert(incident.clone());
    let json_alert = serde_json::to_string(&ev_alert).expect("serialize alert ev");
    let des_alert: Event = serde_json::from_str(&json_alert).expect("deserialize alert ev");
    assert_eq!(ev_alert, des_alert);

    let ev_list = Event::AutopilotIncidentsList(vec![incident]);
    let json_list = serde_json::to_string(&ev_list).expect("serialize list ev");
    let des_list: Event = serde_json::from_str(&json_list).expect("deserialize list ev");
    assert_eq!(ev_list, des_list);

    let req_resolve = Request::ResolveAutopilotIncident {
        incident_id: "inc-1".into(),
        approve_and_merge: true,
    };
    let json_res = serde_json::to_string(&req_resolve).expect("serialize resolve req");
    let des_res: Request = serde_json::from_str(&json_res).expect("deserialize resolve req");
    assert_eq!(req_resolve, des_res);
}

#[test]
fn test_web_console_types_serialization() {
    let cfg = WebConsoleConfig {
        bind_addr: "0.0.0.0".into(),
        port: 9090,
        auth_required: true,
        ws_ping_interval_secs: 15,
    };
    let req_start = Request::StartWebConsole(cfg.clone());
    let json_start = serde_json::to_string(&req_start).expect("serialize start web");
    let des_start: Request = serde_json::from_str(&json_start).expect("deserialize start web");
    assert_eq!(req_start, des_start);

    let status = WebConsoleStatus {
        running: true,
        bind_addr: "127.0.0.1".into(),
        port: 8088,
        connected_clients: 2,
        active_sessions_count: 3,
        url: "http://127.0.0.1:8088".into(),
    };
    let ev_status = Event::WebConsoleStatus(status.clone());
    let json_status = serde_json::to_string(&ev_status).expect("serialize web status");
    let des_status: Event = serde_json::from_str(&json_status).expect("deserialize web status");
    assert_eq!(ev_status, des_status);

    let session = WebAuthSession {
        token: "tok_1234567890abcdef".into(),
        created_at: 1725350000,
        expires_at: 1725353600,
        client_label: Some("secondary-laptop".into()),
    };
    let ev_token = Event::WebTokenGenerated(session.clone());
    let json_token = serde_json::to_string(&ev_token).expect("serialize token ev");
    let des_token: Event = serde_json::from_str(&json_token).expect("deserialize token ev");
    assert_eq!(ev_token, des_token);

    let ws_msg = WebSocketMessage {
        topic: "telemetry".into(),
        payload: "{\"cpu\": 12.5}".into(),
        timestamp: 1725350000,
    };
    let json_ws = serde_json::to_string(&ws_msg).expect("serialize ws msg");
    let des_ws: WebSocketMessage = serde_json::from_str(&json_ws).expect("deserialize ws msg");
    assert_eq!(ws_msg, des_ws);
}

#[test]
fn test_dev_workspace_status_serialization() {
    let status = DevWorkspaceStatus {
        active_project: Some("core-engine".into()),
        active_panel: DevPanelKind::Editor,
        editor_command: "nvim".into(),
        side_panel_visible: true,
        terminal_drawer_open: false,
        term_columns: 140,
        term_rows: 40,
        editor_rect: DevPanelRect {
            x: 0,
            y: 0,
            width: 98,
            height: 40,
        },
        agent_monitor_rect: DevPanelRect {
            x: 98,
            y: 0,
            width: 42,
            height: 20,
        },
        diff_viewer_rect: DevPanelRect {
            x: 98,
            y: 20,
            width: 42,
            height: 20,
        },
        terminal_rect: None,
        registered_hotkeys: vec!["Ctrl+W".into(), "Super+W".into()],
    };

    let req = Request::GetDevWorkspaceStatus {
        project: Some("core-engine".into()),
    };
    let json_req = serde_json::to_string(&req).expect("serialize req");
    let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req");
    assert_eq!(req, des_req);

    let ev = Event::DevWorkspaceStatus(status.clone());
    let json_ev = serde_json::to_string(&ev).expect("serialize ev");
    let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize ev");
    assert_eq!(ev, des_ev);
    assert_eq!(status.active_panel.name(), "Editor (Neovim)");
}

#[test]
fn test_tdd_reproduce_serialization() {
    let diag = ParsedErrorDiagnostic {
        language: ErrorLanguage::Rust,
        error_type: "Panic".into(),
        message: "index out of bounds: the len is 3 but the index is 5".into(),
        target_file: Some("src/parser.rs".into()),
        target_line: Some(42),
        target_function: Some("parse_token".into()),
        frames: vec![ParsedStackFrame {
            file: "src/parser.rs".into(),
            line: Some(42),
            col: Some(15),
            function: Some("parse_token".into()),
        }],
    };

    let report = TddRegressionReport {
        id: "tdd-001".into(),
        diagnostic: diag,
        test_code: "#[test]\nfn test_reproduce_panic() { assert!(true); }".into(),
        test_file: "tests/regression_tdd_001.rs".into(),
        phase: TddPhase::Red,
        fix_summary: Some("bounds check added in parse_token".into()),
        audited: true,
    };

    let req = Request::ReproduceBug {
        error_text: "thread 'main' panicked at src/parser.rs:42:15".into(),
        target_file: Some("src/parser.rs".into()),
    };
    let json_req = serde_json::to_string(&req).expect("serialize req");
    let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req");
    assert_eq!(req, des_req);

    let ev = Event::TddReport(report.clone());
    let json_ev = serde_json::to_string(&ev).expect("serialize ev");
    let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize ev");
    assert_eq!(ev, des_ev);
    assert_eq!(report.phase.label(), "🔴 Red (Falla reproducible)");
}

#[test]
fn test_ci_and_git_hooks_serialization() {
    let stage = CiStageResult {
        name: "lint".into(),
        command: "cargo clippy".into(),
        status: CiStageStatus::Passed,
        duration_ms: 120,
        output_snippet: "clean 0 warnings".into(),
        exit_code: Some(0),
    };
    let report = CiReport {
        id: "ci-001".into(),
        success: true,
        stages: vec![stage],
        total_duration_ms: 250,
        security_clean: true,
        secrets_found: Vec::new(),
        timestamp_secs: 1725380000,
    };

    let req = Request::RunCi {
        stage: Some("test".into()),
        fast: true,
    };
    let json_req = serde_json::to_string(&req).expect("serialize req");
    let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req");
    assert_eq!(req, des_req);

    let ev = Event::CiReport(report.clone());
    let json_ev = serde_json::to_string(&ev).expect("serialize ev");
    let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize ev");
    assert_eq!(ev, des_ev);

    let hook_status = GitHookStatus {
        pre_commit_installed: true,
        pre_push_installed: true,
        hook_dir: ".git/hooks".into(),
        active_guards: vec!["secret_scanner".into(), "ci_fast".into()],
    };
    let ev_hook = Event::GitHooksStatus(hook_status.clone());
    let json_hook = serde_json::to_string(&ev_hook).expect("serialize ev_hook");
    let des_hook: Event = serde_json::from_str(&json_hook).expect("deserialize ev_hook");
    assert_eq!(ev_hook, des_hook);
}

#[test]
fn test_dev_snapshot_serialization() {
    let meta = DevSnapshotMetadata {
        id: "snap-20260903-test".into(),
        label: Some("pre-refactor".into()),
        author: "human".into(),
        timestamp_secs: 1725385000,
        git_branch: Some("master".into()),
        git_commit: Some("75e0297".into()),
        files_count: 42,
        total_bytes: 1048576,
        services_included: vec!["postgres".into()],
        memory_graph_included: true,
        method: "clon".into(),
    };

    let req = Request::CreateSnapshot {
        label: Some("pre-refactor".into()),
        author: Some("human".into()),
    };
    let json_req = serde_json::to_string(&req).expect("serialize req");
    let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req");
    assert_eq!(req, des_req);

    let ev = Event::SnapshotCreated(meta.clone());
    let json_ev = serde_json::to_string(&ev).expect("serialize ev");
    let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize ev");
    assert_eq!(ev, des_ev);

    let res = SnapshotRestoreResult {
        snapshot_id: "snap-20260903-test".into(),
        rescue_snapshot_id: Some("rescue-1234".into()),
        files_restored: 42,
        files_deleted: 2,
        services_restored: vec!["postgres".into()],
        memory_graph_restored: true,
        duration_ms: 120,
    };
    let ev_res = Event::SnapshotRestored(res.clone());
    let json_res = serde_json::to_string(&ev_res).expect("serialize ev_res");
    let des_res: Event = serde_json::from_str(&json_res).expect("deserialize ev_res");
    assert_eq!(ev_res, des_res);
}

#[test]
fn test_benchmark_serialization() {
    let metric = BenchmarkMetric {
        name: "ipc_roundtrip".into(),
        mean_ns: 12500,
        min_ns: 11000,
        max_ns: 15000,
        p95_ns: 14200,
        p99_ns: 14800,
        peak_rss_bytes: 4096 * 1024,
        ops_per_sec: 80000.0,
    };

    let report = BenchmarkRunReport {
        id: "bench-test-1".into(),
        timestamp_secs: 1725389000,
        branch: "master".into(),
        commit: Some("a73fce6".into()),
        suite_name: "microbenchmarks".into(),
        metrics: vec![metric],
        total_duration_ms: 350,
    };

    let req = Request::RunBenchmark {
        target: Some("microbenchmarks".into()),
    };
    let json_req = serde_json::to_string(&req).expect("serialize req");
    let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req");
    assert_eq!(req, des_req);

    let ev = Event::BenchmarkReport(report.clone());
    let json_ev = serde_json::to_string(&ev).expect("serialize ev");
    let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize ev");
    assert_eq!(ev, des_ev);

    let diff_metric = BenchmarkComparisonMetric {
        name: "ipc_roundtrip".into(),
        base_mean_ns: 12500,
        target_mean_ns: 11000,
        delta_pct: -12.0,
        base_rss_bytes: 4096 * 1024,
        target_rss_bytes: 3900 * 1024,
        rss_delta_pct: -4.78,
        is_regression: false,
        severity: "None".into(),
    };

    let diff_report = BenchmarkDiffReport {
        id: "diff-test-1".into(),
        timestamp_secs: 1725389000,
        base_branch: "master".into(),
        target_branch: "ticket/T21.1".into(),
        comparisons: vec![diff_metric],
        has_regression: false,
        max_regression_pct: 0.0,
        auditor_verdict: "Aprobado: rendimiento optimizado o estable".into(),
    };

    let ev_diff = Event::BenchmarkDiff(diff_report.clone());
    let json_diff = serde_json::to_string(&ev_diff).expect("serialize ev_diff");
    let des_diff: Event = serde_json::from_str(&json_diff).expect("deserialize ev_diff");
    assert_eq!(ev_diff, des_diff);
}

#[test]
fn test_forge_and_pr_serialization() {
    let issue = RemoteIssue {
        id: 101,
        number: 42,
        title: "Fix crash on invalid IPC token".into(),
        body: "When token has invalid chars daemon panics".into(),
        state: "open".into(),
        author: "octocat".into(),
        labels: vec!["bug".into(), "critical".into()],
        url: "https://github.com/antos/antos/issues/42".into(),
        created_at: "2026-09-04T08:00:00Z".into(),
    };

    let req_import = Request::ImportRemoteIssue {
        id_or_url: "42".into(),
    };
    let json_req = serde_json::to_string(&req_import).expect("serialize req");
    let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req");
    assert_eq!(req_import, des_req);

    let ev_issues = Event::RemoteIssuesList(vec![issue]);
    let json_ev = serde_json::to_string(&ev_issues).expect("serialize ev");
    let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize ev");
    assert_eq!(ev_issues, des_ev);

    let pr = RemotePullRequest {
        id: 501,
        number: 12,
        title: "feat(ipc): validate tokens safely".into(),
        body: "Fixes #42 - prevents daemon panic on invalid token".into(),
        head_branch: "ticket/T-GH-42".into(),
        base_branch: "master".into(),
        state: "open".into(),
        url: "https://github.com/antos/antos/pull/12".into(),
        draft: false,
    };

    let ev_pr = Event::PullRequestCreated(pr);
    let json_pr = serde_json::to_string(&ev_pr).expect("serialize pr");
    let des_pr: Event = serde_json::from_str(&json_pr).expect("deserialize pr");
    assert_eq!(ev_pr, des_pr);
}

#[test]
fn test_arch_doc_serialization() {
    let report = ArchDiagramReport {
        kind: ArchDiagramKind::Components,
        mermaid_content: "graph TD\n  A[kernel] --> B[antosd]".into(),
        crates_count: 5,
        modules_count: 24,
        caps_count: 36,
        generated_at_secs: 1725440000,
    };

    let req = Request::GenerateArchDiagram {
        kind: Some("components".into()),
    };
    let json_req = serde_json::to_string(&req).expect("serialize req");
    let des_req: Request = serde_json::from_str(&json_req).expect("deserialize req");
    assert_eq!(req, des_req);

    let ev = Event::ArchDiagram(report.clone());
    let json_ev = serde_json::to_string(&ev).expect("serialize ev");
    let des_ev: Event = serde_json::from_str(&json_ev).expect("deserialize ev");
    assert_eq!(ev, des_ev);

    let sync_report = DocSyncReport {
        files_scanned: 3,
        files_updated: 1,
        in_sync: true,
        updated_paths: vec!["docs/arquitectura.md".into()],
        message: "Documentación arquitectónica sincronizada con éxito.".into(),
    };
    let ev_sync = Event::DocSync(sync_report.clone());
    let json_sync = serde_json::to_string(&ev_sync).expect("serialize ev_sync");
    let des_sync: Event = serde_json::from_str(&json_sync).expect("deserialize ev_sync");
    assert_eq!(ev_sync, des_sync);
}
