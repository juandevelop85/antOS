//! Serialización de git, tickets, diffs, notificaciones y antMesh.
use crate::*;

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
    assert!(!deserialized.is_clean());
}

#[test]
fn test_query_git_status_request_serialization() {
    let request = Request::QueryGitStatus {
        workspace_path: "/Users/dev/workspace".into(),
    };

    let json = serde_json::to_string(&request).expect("should serialize request");
    let deserialized: Request = serde_json::from_str(&json).expect("should deserialize request");

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
    let json_no_repo = serde_json::to_string(&resp_no_repo).expect("should serialize NotGitRepo");
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
    let deserialized_note: Event = serde_json::from_str(&json_note).expect("deserialize note");
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
    let json_request = serde_json::to_string(&list_request).expect("serialize list request");
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
    let des_resp: Event = serde_json::from_str(&json_resp).expect("deserialize ticket list");
    assert_eq!(list_response, des_resp);

    let port_info = PortDiagnosticInfo {
        port: 3000,
        pid: 12345,
        process_name: "node".into(),
        command: "node server.js".into(),
        working_dir: Some("/app".into()),
    };
    let json_port = serde_json::to_string(&port_info).expect("serialize port");
    let des_port: PortDiagnosticInfo = serde_json::from_str(&json_port).expect("deserialize port");
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
            model: None,
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
        project_path: None,
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
        backend: EbpfBackend::Simulated,
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
        metrics_are_real: true,
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
