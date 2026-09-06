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
        };
        let json_manifest = serde_json::to_string(&manifest).expect("serialize manifest");
        let des_manifest: PackageManifest = serde_json::from_str(&json_manifest).expect("deserialize manifest");
        assert_eq!(manifest, des_manifest);

        let req_install = Request::InstallPackage {
            recipe_path_or_name: "ripgrep".into(),
            dry_run: true,
        };
        let json_install = serde_json::to_string(&req_install).expect("serialize install req");
        let des_install: Request = serde_json::from_str(&json_install).expect("deserialize install req");
        assert_eq!(req_install, des_install);

        let summary = PackageSummary {
            name: "ripgrep".into(),
            version: "14.1.0".into(),
            description: "Fast line-oriented search tool".into(),
            store_hash: "a1b2c3d4e5f6".into(),
            installed_size_bytes: 5242880,
            installed_at: "2026-09-03T08:00:00Z".into(),
            binaries: vec!["rg".into()],
            generation: 1,
        };
        let ev_list = Event::PackageList(vec![summary.clone()]);
        let json_list = serde_json::to_string(&ev_list).expect("serialize package list ev");
        let des_list: Event = serde_json::from_str(&json_list).expect("deserialize package list ev");
        assert_eq!(ev_list, des_list);

        let report = PackageInstallReport {
            name: "ripgrep".into(),
            version: "14.1.0".into(),
            store_path: "/var/antos/store/a1b2c3d4e5f6-ripgrep-14.1.0".into(),
            generation: 1,
            binaries_linked: vec!["rg".into()],
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
        let des_status: Event = serde_json::from_str(&json_status).expect("deserialize store status ev");
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
            editor_rect: DevPanelRect { x: 0, y: 0, width: 98, height: 40 },
            agent_monitor_rect: DevPanelRect { x: 98, y: 0, width: 42, height: 20 },
            diff_viewer_rect: DevPanelRect { x: 98, y: 20, width: 42, height: 20 },
            terminal_rect: None,
            registered_hotkeys: vec!["Ctrl+W".into(), "Super+W".into()],
        };

        let req = Request::GetDevWorkspaceStatus { project: Some("core-engine".into()) };
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

    #[test]
    fn test_submodule_namespaces_and_roundtrips() {
        use crate::plan::{Plan, Step, Tier};
        use crate::git::{GitRepoStatus, GitFileStatus, GitFileDiffSummary};
        use crate::flow::{AgentRole, FlowTask, FlowState};
        use crate::spec::{TicketSummary, TicketDetail, TicketStatus};
        use crate::mesh::{MeshStatus, PeerNode, NodeResources};
        use crate::wasm::PluginSummary;
        use crate::dev::{LspServerStatus, DapSessionStatus};
        use crate::vm::{MicrovmConfig, MicrovmStatus};
        use crate::system::VfsStatus;
        use crate::ipc::{Request, Event};

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
        let des_vm_status: MicrovmStatus = serde_json::from_str(&json_vm_status).expect("vm_status deser");
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
        let des_cons: ConsoleInfoModel = serde_json::from_str(&json_cons).expect("deserialize console info");
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
        let des_sc: SyscallTableInfo = serde_json::from_str(&json_sc).expect("deserialize syscall table info");
        assert_eq!(sc, des_sc);

        let chan = IpcChannelInfo {
            channel_id: 1,
            pending_messages: 2,
            max_messages: 32,
            max_message_size: 1024,
            waiting_receiver: false,
        };
        let json_chan = serde_json::to_string(&chan).expect("serialize ipc channel info");
        let des_chan: IpcChannelInfo = serde_json::from_str(&json_chan).expect("deserialize ipc channel info");
        assert_eq!(chan, des_chan);

        let msg = IpcMessageSummary {
            channel_id: 1,
            sender_pid: 2,
            payload_size: 64,
        };
        let json_msg = serde_json::to_string(&msg).expect("serialize ipc message summary");
        let des_msg: IpcMessageSummary = serde_json::from_str(&json_msg).expect("deserialize ipc message summary");
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
        let des_cfg: LimineBootConfig = serde_json::from_str(&json_cfg).expect("deserialize limine config");
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
        let des_rep: HybridBootImageReport = serde_json::from_str(&json_rep).expect("deserialize hybrid report");
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
        let des_cfg: LiveRamdiskConfig = serde_json::from_str(&json_cfg).expect("deserialize ramdisk config");
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
        let des_m: LiveRamdiskManifest = serde_json::from_str(&json_m).expect("deserialize ramdisk manifest");
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
        assert_eq!(des_status.devices[0].interface, StorageInterfaceKind::AhciSata);
        assert_eq!(des_status.devices[1].interface, StorageInterfaceKind::Nvme);
    }
