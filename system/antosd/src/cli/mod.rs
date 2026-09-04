//! antOS CLI orchestrator and command dispatcher.

pub mod args;
pub mod commands;

pub use args::Opts;

use anyhow::Result;
use crate::capability::Catalog;
use crate::ctx::Ctx;
use crate::ipc;

pub fn dispatch(ctx: &Ctx, catalog: &Catalog, raw_args: Vec<String>) -> Result<()> {
    let (opts, rest) = Opts::parse_from(raw_args);

    if opts.help || rest.is_empty() {
        help();
        return Ok(());
    }

    // Apply any explicit --project flag to the context
    let mut effective_ctx = ctx.clone();
    if let Some(ref proj_name) = opts.project {
        let p = if std::path::Path::new(proj_name).is_absolute() {
            std::path::PathBuf::from(proj_name)
        } else {
            ctx.workspace.join(proj_name)
        };
        effective_ctx.current_project = Some(p);
    }
    let ctx = &effective_ctx;

    match rest[0].as_str() {
        "caps" => commands::system::cmd_caps(catalog, ctx),
        "demonio" => ipc::servir(ctx, catalog),
        "doctor" => commands::system::cmd_doctor(ctx),
        "escucha" => commands::run::cmd_escuchar(ctx, catalog, &rest[1..], &opts),
        "log" => commands::system::cmd_log(ctx),
        "undo" => commands::undo::cmd_undo(ctx, &rest[1..]),
        "tickets" | "ticket" => commands::tickets::cmd_tickets(ctx, &rest[1..]),
        "ports" => commands::tools::cmd_ports(&rest[1..]),
        "services" | "service" => commands::tools::cmd_services(ctx, &rest[1..]),
        "secrets" | "secret" => commands::tools::cmd_secrets(ctx, &rest[1..]),
        "agent" | "agents" | "flow" => commands::flow::cmd_agent(ctx, &rest[1..]),
        "panel" | "board" => commands::flow::cmd_panel(ctx, &rest[1..]),
        "llm" | "models" | "model" => commands::tools::cmd_llm(ctx, &rest[1..]),
        "memory" | "memoria" | "search" => commands::tools::cmd_memory(ctx, &rest[1..]),
        "env" | "perfil" => commands::tools::cmd_env(ctx, &rest[1..]),
        "quota" | "cuota" | "cuotas" | "limits" => commands::tools::cmd_quota(ctx, &rest[1..]),
        "diff" | "diffs" => commands::tools::cmd_diff(ctx, &rest[1..]),
        "vte" | "term" | "terminal" => commands::tools::cmd_terminal(&rest[1..]),
        "notify" | "notif" | "notificaciones" => commands::tools::cmd_notify(ctx, &rest[1..]),
        "mesh" | "p2p" => commands::mesh::cmd_mesh(ctx, &rest[1..]),
        "swarm" => commands::flow::cmd_swarm(ctx, &rest[1..]),
        "vfs" | "antfs" => commands::vfs::cmd_vfs(ctx, &rest[1..]),
        "ebpf" | "bpf" => commands::tools::cmd_ebpf(ctx, &rest[1..]),
        "profile" | "perf" | "profiler" => commands::tools::cmd_profile(ctx, &rest[1..]),
        "edit" | "editor" | "nvim" => commands::dev::cmd_edit(&rest[1..]),
        "lsp" => commands::dev::cmd_lsp(ctx, &rest[1..]),
        "pair" | "collab" => commands::dev::cmd_pair(ctx, &rest[1..]),
        "debug" | "dap" => commands::dev::cmd_debug(ctx, &rest[1..]),
        "dev" | "workspace" => commands::dev::cmd_dev(ctx, &rest[1..]),
        "reproduce" | "tdd" => commands::tools::cmd_reproduce(ctx, &rest[1..]),
        "testgen" | "test-gen" => commands::tools::cmd_testgen(ctx, &rest[1..]),
        "ci" => commands::ci::cmd_ci(ctx, &rest[1..]),
        "hook" | "hooks" => commands::ci::cmd_hook(ctx, &rest[1..]),
        "snapshot" | "snapshots" | "tm" => commands::tools::cmd_snapshot(ctx, &rest[1..]),
        "bench" | "benchmark" => commands::tools::cmd_bench(ctx, &rest[1..]),
        "issue" | "issues" => commands::tools::cmd_issue(ctx, &rest[1..]),
        "pr" | "pull-request" => commands::tools::cmd_pr(ctx, &rest[1..]),
        "doc" | "docs" => commands::tools::cmd_doc(ctx, &rest[1..]),
        "desktop" | "wm" => commands::system::cmd_desktop(ctx, &rest[1..]),
        "barra" | "bar" => commands::system::cmd_barra(ctx, &rest[1..]),
        "boot" | "qemu" => commands::system::cmd_boot(ctx, &rest[1..]),
        "plugin" | "plugins" | "wasm" => commands::system::cmd_plugin(ctx, &rest[1..]),
        "screenshot" | "captura" => commands::tools::cmd_screenshot(ctx, &rest[1..]),
        "qa" => commands::tools::cmd_qa(ctx, &rest[1..]),
        "disk" | "storage" | "part" => commands::system::cmd_disk(ctx, &rest[1..]),
        "install" | "installer" => commands::system::cmd_install(ctx, &rest[1..]),
        "bootloader" | "uefi" => commands::system::cmd_bootloader(ctx, &rest[1..]),
        "vm" | "microvm" => commands::system::cmd_vm(ctx, &rest[1..]),
        "pkg" | "antpkg" | "package" => commands::pkg::cmd_pkg(ctx, &rest[1..]),
        "autopilot" | "sentinel" | "centinela" => commands::system::cmd_autopilot(ctx, &rest[1..]),
        "web" | "webconsole" | "remote-console" => commands::system::cmd_web(ctx, &rest[1..]),
        "project" | "projects" | "proyectos" => commands::tools::cmd_project(ctx, &rest[1..]),
        "use" => commands::tools::cmd_use(ctx, &rest[1..]),
        "git" => commands::tools::cmd_git(ctx, &rest[1..]),
        "release" | "dist" => commands::system::cmd_boot(ctx, &["release".into()]),
        "grant" => commands::system::cmd_grant(ctx, catalog, &rest[1..]),
        "revoke" => commands::system::cmd_revoke(ctx, &rest[1..]),
        _ => commands::run::cmd_intent(ctx, catalog, &rest.join(" "), &opts),
    }
}

pub fn help() {
    println!(
        "\
antOS — el sistema hace lo que le pides, y puedes deshacerlo

  antos \"<intención>\"       planifica, enseña el diff y ejecuta
  antos escucha              lo mismo, dictado por voz (transcripción local)
  antos panel                centro de control de agentes y tablero Kanban (Super + A)
  antos tickets [id]         catálogo de tickets y especificaciones
  antos agent run <id>       ejecuta ticket con orquestación multi-agente
  antos agent status [id]    consulta estado y traza de agentes
  antos agents               lista los roles especializados y sus directivas
  antos ports [puerto]       diagnóstico de puertos de red y procesos
  antos services             gestión de servicios efímeros (postgres, redis, mysql)
  antos caps                 catálogo de capacidades y su nivel
  antos log                  bitácora de lo que ha pasado
  antos undo [--ticket id]   revierte el último plan o todos los cambios de un ticket
  antos doctor               comprueba que el recinto es real, atacándolo
  antos diff [proyecto] [ref] visor interactivo de diffs y parches por proyecto
  antos edit [fichero]       abre el fichero en el editor predeterminado (Neovim / Super + E)
  antos dev [--project <p>]  espacio de trabajo TUI multipanel (Neovim + antFlow + Diffs / Super + W)
  antos reproduce <error>    reproducción autónoma de bugs y suite de regresión TDD (T20.2)
  antos testgen [objetivo]   generador autónomo de tests e invariantes unitarias (T20.2)
  antos ci [run|status]      ejecuta matriz de integración continua local paralela en sandboxes (T20.3)
  antos hook [install|check] gestión de hooks de Git pre-commit con auditoría de secretos (T20.3)
  antos snapshot [create|restore|list] Time Machine atómico de código, servicios y estado (T20.4)
  antos bench [run|diff|history] benchmarking continuo y detección de regresiones en worktrees (T21.1)
  antos issue [list|import]  sincronización e importación de issues remotos de GitHub/GitLab (T21.2)
  antos pr [create|status]   publicación y consulta de Pull Requests / Merge Requests certificados (T21.2)
  antos doc [arch|sync|check] diagramas vivos de arquitectura y sincronización Mermaid en markdown (T21.3)
  antos project init <nombre> inicializa repositorio Git aislado y .gitignore en workspace
  antos project list         lista los proyectos y su estado de control de versiones
  antos grant <cap> [--minutos N]
  antos revoke <cap>

opciones
  -p, --planificador <local|claude>
  -s, --si                   no preguntar confirmación
  -n, --seco                 planificar y previsualizar sin ejecutar
      --segundos <N>         escucha: cuánto grabar (por defecto 5)
      --desde <fichero>      escucha: transcribir un audio en vez del micrófono
      --dispositivos         escucha: listar las entradas de audio
      --dispositivo <N>      escucha: cuál usar (por defecto, la del sistema)

entorno
  ANTOS_WORKSPACE  espacio de trabajo (por defecto ./workspace)
  ANTOS_STATE      instantáneas y bitácora (por defecto ./.antos)
  ANTHROPIC_API_KEY  activa el planificador con Claude"
    );
}
