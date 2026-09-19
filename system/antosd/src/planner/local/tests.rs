#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::ctx::Ctx;

#[test]
fn test_plan_commit_semantico_auth() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let propuesta = planner
        .plan("haz commit con los cambios de auth", &catalog)
        .expect("debe planificar");

    assert_eq!(propuesta.steps.len(), 1);
    let s = &propuesta.steps[0];
    assert_eq!(s.capability, "git.commit_semantic");
    assert_eq!(s.args.get("type").map(String::as_str), Some("feat"));
    assert_eq!(s.args.get("scope").map(String::as_str), Some("auth"));
}

#[test]
fn test_plan_crear_rama() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let propuesta = planner
        .plan("crea rama login-flow", &catalog)
        .expect("debe planificar");

    assert_eq!(propuesta.steps.len(), 1);
    let s = &propuesta.steps[0];
    assert_eq!(s.capability, "git.smart_branch");
    assert_eq!(s.args.get("name").map(String::as_str), Some("login-flow"));
}

#[test]
fn test_plan_worktree_create_and_cleanup() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_crear = planner
        .plan("crea worktree para T3.1", &catalog)
        .expect("debe planificar");
    assert_eq!(p_crear.steps.len(), 1);
    assert_eq!(p_crear.steps[0].capability, "git.worktree_create");
    assert_eq!(
        p_crear.steps[0].args.get("ticket_id").map(String::as_str),
        Some("t3.1")
    );

    let p_limpiar = planner
        .plan("limpia worktree de T3.1", &catalog)
        .expect("debe planificar");
    assert_eq!(p_limpiar.steps.len(), 1);
    assert_eq!(p_limpiar.steps[0].capability, "git.worktree_cleanup");
    assert_eq!(
        p_limpiar.steps[0].args.get("ticket_id").map(String::as_str),
        Some("t3.1")
    );
}

#[test]
fn test_plan_port_diagnose_and_release() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_libera = planner
        .plan("libera el puerto 3000", &catalog)
        .expect("debe planificar");
    assert_eq!(p_libera.steps.len(), 1);
    assert_eq!(p_libera.steps[0].capability, "diag.port_kill");
    assert_eq!(
        p_libera.steps[0].args.get("port").map(String::as_str),
        Some("3000")
    );

    let p_estado = planner.plan("puertos", &catalog).expect("debe planificar");
    assert_eq!(p_estado.steps.len(), 1);
    assert_eq!(p_estado.steps[0].capability, "diag.port_status");
}

/// T35.1: la tecnología sale del catálogo de stacks por sus alias, en
/// cualquier posición; sin nombre se pregunta, no se inventa.
#[test]
fn test_plan_proyecto_por_stack() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;
    let args_of = |intent: &str| {
        let p = planner.plan(intent, &catalog).expect(intent);
        assert_eq!(p.steps[0].capability, "project.scaffold");
        p.steps[0].args.clone()
    };

    let a = args_of("crea un proyecto en nestjs llamado antostest");
    assert_eq!(a["language"], "typescript");
    assert_eq!(a["framework"], "nestjs");
    assert_eq!(a["name"], "antostest");

    let a = args_of("api fastapi llamada demo, nueva");
    assert_eq!(a["language"], "python");
    assert_eq!(a["framework"], "fastapi");
    assert_eq!(a["name"], "demo");

    let a = args_of("crea un servicio axum nombre core");
    assert_eq!(a["language"], "rust");
    assert_eq!(a["framework"], "axum");

    let a = args_of("nuevo proyecto typescript con nest llamado web");
    assert_eq!(a["framework"], "nestjs", "el framework gana al base");

    // Sin tecnología: Rust base, como siempre; y sin `framework` en los args.
    let a = args_of("crea un proyecto llamado demo");
    assert_eq!(a["language"], "rust");
    assert!(!a.contains_key("framework"));

    // T35.2: crear = andamio + install en la misma propuesta; «sin instalar»
    // deja solo el andamio; los verbos sobre un proyecto → project.run.
    let p = planner
        .plan("crea un proyecto en express llamado shop", &catalog)
        .unwrap();
    assert_eq!(p.steps.len(), 3, "andamio + install + verify (T35.3)");
    assert_eq!(p.steps[1].capability, "project.run");
    assert_eq!(p.steps[1].args["project"], "shop");
    assert_eq!(p.steps[1].args["command"], "install");
    assert_eq!(p.steps[2].args["command"], "verify");
    let p = planner
        .plan(
            "crea un proyecto en express llamado shop sin verificar",
            &catalog,
        )
        .unwrap();
    assert_eq!(p.steps.len(), 2);
    let p = planner
        .plan(
            "crea un proyecto en express llamado shop sin instalar",
            &catalog,
        )
        .unwrap();
    assert_eq!(p.steps.len(), 1);
    for (intent, command) in [
        ("instala las dependencias del proyecto shop", "install"),
        ("prueba el proyecto shop", "test"),
        ("compila el proyecto shop", "build"),
    ] {
        let p = planner.plan(intent, &catalog).expect(intent);
        assert_eq!(p.steps[0].capability, "project.run", "{intent}");
        assert_eq!(p.steps[0].args["project"], "shop", "{intent}");
        assert_eq!(p.steps[0].args["command"], command, "{intent}");
    }

    // Sin nombre: pregunta.
    let msg = match planner.plan("crea un proyecto en nestjs", &catalog) {
        Ok(_) => panic!("sin nombre debe preguntar"),
        Err(e) => e.to_string(),
    };
    assert!(msg.contains("llamado"), "{msg}");

    // «inicia el servicio ollama» no es un proyecto.
    let p = planner.plan("inicia el servicio ollama", &catalog).unwrap();
    assert_eq!(p.steps[0].capability, "env.service_up");
}

#[test]
fn test_plan_servicios_locales() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_up = planner
        .plan("levanta un postgres para este proyecto", &catalog)
        .expect("debe planificar postgres");
    assert_eq!(p_up.steps.len(), 1);
    assert_eq!(p_up.steps[0].capability, "env.service_up");
    assert_eq!(
        p_up.steps[0].args.get("service").map(String::as_str),
        Some("postgres")
    );

    let p_redis = planner
        .plan("inicia redis en el puerto 6379", &catalog)
        .expect("debe planificar redis");
    assert_eq!(p_redis.steps.len(), 1);
    assert_eq!(p_redis.steps[0].capability, "env.service_up");
    assert_eq!(
        p_redis.steps[0].args.get("service").map(String::as_str),
        Some("redis")
    );
    assert_eq!(
        p_redis.steps[0].args.get("port").map(String::as_str),
        Some("6379")
    );

    // T34.1: Ollama es un servicio más.
    let p_ollama = planner
        .plan("levanta ollama en el puerto 11500", &catalog)
        .expect("debe planificar ollama");
    assert_eq!(p_ollama.steps[0].capability, "env.service_up");
    assert_eq!(
        p_ollama.steps[0].args.get("service").map(String::as_str),
        Some("ollama")
    );
    assert_eq!(
        p_ollama.steps[0].args.get("port").map(String::as_str),
        Some("11500")
    );

    let p_down = planner
        .plan("apaga el servicio postgres", &catalog)
        .expect("debe planificar down");
    assert_eq!(p_down.steps.len(), 1);
    assert_eq!(p_down.steps[0].capability, "env.service_down");
    assert_eq!(
        p_down.steps[0].args.get("service").map(String::as_str),
        Some("postgres")
    );
}

#[test]
fn test_plan_secrets_and_grants() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_grant = planner
        .plan("concede acceso a .env", &catalog)
        .expect("debe planificar grant");
    assert_eq!(p_grant.steps.len(), 1);
    assert_eq!(p_grant.steps[0].capability, "secret.grant");

    let p_revoke = planner
        .plan("revoca acceso a .env", &catalog)
        .expect("debe planificar revoke");
    assert_eq!(p_revoke.steps.len(), 1);
    assert_eq!(p_revoke.steps[0].capability, "secret.revoke");

    let p_list = planner
        .plan("lista los secretos de la bóveda", &catalog)
        .expect("debe planificar list");
    assert_eq!(p_list.steps.len(), 1);
    assert_eq!(p_list.steps[0].capability, "secret.list");
}

#[test]
fn test_plan_tickets_dinamicos() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_create = planner
        .plan(
            "crea un ticket T6.1 para motor de inferencia local",
            &catalog,
        )
        .expect("plan create");
    assert_eq!(p_create.steps.len(), 1);
    assert_eq!(p_create.steps[0].capability, "spec.create_ticket");
    assert_eq!(
        p_create.steps[0].args.get("ticket_id").map(String::as_str),
        Some("T6.1")
    );

    let p_status = planner
        .plan("actualiza ticket T1.1 a completado", &catalog)
        .expect("plan status");
    assert_eq!(p_status.steps.len(), 1);
    assert_eq!(p_status.steps[0].capability, "spec.update_ticket");
    assert_eq!(
        p_status.steps[0].args.get("ticket_id").map(String::as_str),
        Some("T1.1")
    );
}

#[test]
fn test_plan_memory_and_graph() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_index = planner
        .plan("indexa la memoria del proyecto", &catalog)
        .expect("plan index");
    assert_eq!(p_index.steps.len(), 1);
    assert_eq!(p_index.steps[0].capability, "memory.index");

    let p_search = planner
        .plan("busca codigo sobre gestion de secretos", &catalog)
        .expect("plan search");
    assert_eq!(p_search.steps.len(), 1);
    assert_eq!(p_search.steps[0].capability, "memory.search");

    let p_graph = planner
        .plan("muestra el grafo para ticket:T6.1", &catalog)
        .expect("plan graph");
    assert_eq!(p_graph.steps.len(), 1);
    assert_eq!(p_graph.steps[0].capability, "memory.graph");
}

#[test]
fn test_plan_env_profile_and_sync() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_init = planner
        .plan("configura el entorno para python", &catalog)
        .expect("plan init");
    assert_eq!(p_init.steps.len(), 1);
    assert_eq!(p_init.steps[0].capability, "env.init");
    assert_eq!(
        p_init.steps[0].args.get("profile").map(String::as_str),
        Some("python")
    );

    let p_sync = planner
        .plan("sincroniza el entorno del proyecto", &catalog)
        .expect("plan sync");
    assert_eq!(p_sync.steps.len(), 1);
    assert_eq!(p_sync.steps[0].capability, "env.sync");
}

#[test]
fn test_plan_quota_status_and_set() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_status = planner
        .plan("muestra las cuotas de recursos del sandbox", &catalog)
        .expect("plan status");
    assert_eq!(p_status.steps.len(), 1);
    assert_eq!(p_status.steps[0].capability, "quota.status");

    let p_set = planner
        .plan("establece timeout 60 y memoria 512", &catalog)
        .expect("plan set");
    assert_eq!(p_set.steps.len(), 1);
    assert_eq!(p_set.steps[0].capability, "quota.set");
    assert_eq!(
        p_set.steps[0].args.get("timeout").map(String::as_str),
        Some("60")
    );
    assert_eq!(
        p_set.steps[0].args.get("memory").map(String::as_str),
        Some("512")
    );
}

#[test]
fn test_plan_diff_viewer_and_terminal() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_diff = planner
        .plan("muestra el diff para ticket:T8.1", &catalog)
        .expect("plan diff");
    assert_eq!(p_diff.steps.len(), 1);
    assert_eq!(p_diff.steps[0].capability, "ui.diff_viewer");

    let p_term = planner
        .plan("abre la consola terminal interactiva", &catalog)
        .expect("plan term");
    assert_eq!(p_term.steps.len(), 1);
    assert_eq!(p_term.steps[0].capability, "ui.terminal");
}

#[test]
fn test_plan_notifications_and_approvals() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_list = planner
        .plan("muestra la bandeja de notificaciones", &catalog)
        .expect("plan list notif");
    assert_eq!(p_list.steps.len(), 1);
    assert_eq!(p_list.steps[0].capability, "notify.list");

    let p_approve = planner
        .plan("aprueba la notificación notif-t82", &catalog)
        .expect("plan approve notif");
    assert_eq!(p_approve.steps.len(), 1);
    assert_eq!(p_approve.steps[0].capability, "notify.action");
    assert_eq!(
        p_approve.steps[0].args.get("action").map(String::as_str),
        Some("approve")
    );
}

#[test]
fn test_plan_antmesh_p2p() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_status = planner
        .plan("muestra el estado de la red mesh", &catalog)
        .expect("plan mesh status");
    assert_eq!(p_status.steps.len(), 1);
    assert_eq!(p_status.steps[0].capability, "mesh.status");

    let p_conn = planner
        .plan("conecta al nodo peer 192.168.1.50:9042", &catalog)
        .expect("plan mesh connect");
    assert_eq!(p_conn.steps.len(), 1);
    assert_eq!(p_conn.steps[0].capability, "mesh.connect");
    assert_eq!(
        p_conn.steps[0].args.get("address").map(String::as_str),
        Some("192.168.1.50:9042")
    );

    let p_pair = planner
        .plan("genera un token para emparejar nuevo dispositivo", &catalog)
        .expect("plan mesh pair");
    assert_eq!(p_pair.steps.len(), 1);
    assert_eq!(p_pair.steps[0].capability, "mesh.pair");
}

#[test]
fn test_plan_swarm_distribuido() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_status = planner
        .plan("muestra el estado del swarm", &catalog)
        .expect("plan swarm status");
    assert_eq!(p_status.steps.len(), 1);
    assert_eq!(p_status.steps[0].capability, "flow.swarm_status");

    let p_disp = planner
        .plan(
            "despacha el rol coder del ticket T9.2 al nodo node-gpu-01",
            &catalog,
        )
        .expect("plan swarm dispatch");
    assert_eq!(p_disp.steps.len(), 1);
    assert_eq!(p_disp.steps[0].capability, "flow.dispatch_remote");
    assert_eq!(
        p_disp.steps[0].args.get("ticket_id").map(String::as_str),
        Some("T9.2")
    );
    assert_eq!(
        p_disp.steps[0].args.get("role").map(String::as_str),
        Some("coder")
    );
    assert_eq!(
        p_disp.steps[0].args.get("node").map(String::as_str),
        Some("node-gpu-01")
    );
}

#[test]
fn test_plan_vfs_semantico() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_mount = planner
        .plan("monta el sistema de ficheros virtual antfs", &catalog)
        .expect("plan vfs mount");
    assert_eq!(p_mount.steps.len(), 1);
    assert_eq!(p_mount.steps[0].capability, "vfs.mount");

    let p_unmount = planner
        .plan("desmonta el sistema virtual antfs", &catalog)
        .expect("plan vfs unmount");
    assert_eq!(p_unmount.steps.len(), 1);
    assert_eq!(p_unmount.steps[0].capability, "vfs.unmount");

    let p_query = planner
        .plan(
            "consulta los símbolos en vfs /antfs/symbols/structs",
            &catalog,
        )
        .expect("plan vfs query");
    assert_eq!(p_query.steps.len(), 1);
    assert_eq!(p_query.steps[0].capability, "vfs.query");

    let p_val = planner
        .plan("valida la sintaxis del archivo src/lib.rs en vfs", &catalog)
        .expect("plan vfs validate");
    assert_eq!(p_val.steps.len(), 1);
    assert_eq!(p_val.steps[0].capability, "vfs.validate_write");

    let p_guard = planner
        .plan("muestra el estado del guard vfs", &catalog)
        .expect("plan vfs guard status");
    assert_eq!(p_guard.steps.len(), 1);
    assert_eq!(p_guard.steps[0].capability, "vfs.guard_status");

    let p_ebpf = planner
        .plan("muestra el estado del supervisor ebpf", &catalog)
        .expect("plan ebpf status");
    assert_eq!(p_ebpf.steps.len(), 1);
    assert_eq!(p_ebpf.steps[0].capability, "ebpf.status");

    let p_audit = planner
        .plan("audita las trazas de syscalls de ebpf", &catalog)
        .expect("plan ebpf audit");
    assert_eq!(p_audit.steps.len(), 1);
    assert_eq!(p_audit.steps[0].capability, "ebpf.audit_log");

    let p_prof_run = planner
        .plan("perfila el comando cargo test", &catalog)
        .expect("plan profile run");
    assert_eq!(p_prof_run.steps.len(), 1);
    assert_eq!(p_prof_run.steps[0].capability, "profile.run");

    let p_prof_an = planner
        .plan(
            "analiza los hotspots de rendimiento y cuellos de botella",
            &catalog,
        )
        .expect("plan profile analyze");
    assert_eq!(p_prof_an.steps.len(), 1);
    assert_eq!(p_prof_an.steps[0].capability, "profile.analyze");

    let p_lsp_start = planner
        .plan("inicia el servidor lsp sobre stdio", &catalog)
        .expect("plan lsp start");
    assert_eq!(p_lsp_start.steps.len(), 1);
    assert_eq!(p_lsp_start.steps[0].capability, "lsp.start");

    let p_lsp_status = planner
        .plan("muestra el estado del servidor lsp", &catalog)
        .expect("plan lsp status");
    assert_eq!(p_lsp_status.steps.len(), 1);
    assert_eq!(p_lsp_status.steps[0].capability, "lsp.status");

    let p_collab = planner
        .plan(
            "inicia pair programming con coder en src/main.rs para T12.2",
            &catalog,
        )
        .expect("plan collab session");
    assert_eq!(p_collab.steps.len(), 1);
    assert_eq!(p_collab.steps[0].capability, "collab.session");

    let p_dap = planner
        .plan("depura con dap el comando cargo test", &catalog)
        .expect("plan dap attach");
    assert_eq!(p_dap.steps.len(), 1);
    assert_eq!(p_dap.steps[0].capability, "dap.attach");

    let p_desk_status = planner
        .plan("consulta el estado del escritorio wayland", &catalog)
        .expect("plan desktop session status");
    assert_eq!(p_desk_status.steps.len(), 1);
    assert_eq!(p_desk_status.steps[0].capability, "desktop.session");

    let p_desk_start = planner
        .plan("inicia el escritorio wayland", &catalog)
        .expect("plan desktop session start");
    assert_eq!(p_desk_start.steps.len(), 1);
    assert_eq!(p_desk_start.steps[0].capability, "desktop.session");
    assert_eq!(
        p_desk_start.steps[0].args.get("action").map(|s| s.as_str()),
        Some("start")
    );

    let p_desk_keys = planner
        .plan("muestra los atajos de teclado del escritorio", &catalog)
        .expect("plan desktop keys");
    assert_eq!(p_desk_keys.steps.len(), 1);
    assert_eq!(p_desk_keys.steps[0].capability, "desktop.keys");

    let p_barra_status = planner
        .plan("consulta la telemetría de la barra", &catalog)
        .expect("plan barra status");
    assert_eq!(p_barra_status.steps.len(), 1);
    assert_eq!(p_barra_status.steps[0].capability, "barra.status");

    let p_barra_notif = planner
        .plan("envia una alerta a la barra: Violacion detectada", &catalog)
        .expect("plan barra notify");
    assert_eq!(p_barra_notif.steps.len(), 1);
    assert_eq!(p_barra_notif.steps[0].capability, "barra.notify");

    let p_boot_test = planner
        .plan("prueba el arranque del kernel en qemu", &catalog)
        .expect("plan boot test");
    assert_eq!(p_boot_test.steps.len(), 1);
    assert_eq!(p_boot_test.steps[0].capability, "boot.pipeline");
    assert_eq!(
        p_boot_test.steps[0].args.get("action").map(|s| s.as_str()),
        Some("test")
    );

    let p_boot_build = planner
        .plan("compila el kernel y genera la imagen", &catalog)
        .expect("plan boot build");
    assert_eq!(p_boot_build.steps.len(), 1);
    assert_eq!(p_boot_build.steps[0].capability, "boot.pipeline");
    assert_eq!(
        p_boot_build.steps[0].args.get("action").map(|s| s.as_str()),
        Some("build")
    );

    let p_plugin_list = planner
        .plan("lista los plugins wasm instalados", &catalog)
        .expect("plan plugin list");
    assert_eq!(p_plugin_list.steps.len(), 1);
    assert_eq!(p_plugin_list.steps[0].capability, "plugin.list");

    let p_plugin_run = planner
        .plan("ejecuta el plugin json-parser con accion format", &catalog)
        .expect("plan plugin run");
    assert_eq!(p_plugin_run.steps.len(), 1);
    assert_eq!(p_plugin_run.steps[0].capability, "plugin.run");

    let p_screenshot = planner
        .plan(
            "captura la pantalla de la ventana barra guardando en /tmp/bar.png",
            &catalog,
        )
        .expect("plan screenshot");
    assert_eq!(p_screenshot.steps.len(), 1);
    assert_eq!(p_screenshot.steps[0].capability, "ui.screenshot");

    let p_visual = planner
        .plan(
            "inspecciona el diseño visual de la ventana antos-barra",
            &catalog,
        )
        .expect("plan visual inspection");
    assert_eq!(p_visual.steps.len(), 1);
    assert_eq!(p_visual.steps[0].capability, "ui.inspect_visual");

    let p_disks = planner
        .plan("lista los discos del sistema", &catalog)
        .expect("plan disk list");
    assert_eq!(p_disks.steps.len(), 1);
    assert_eq!(p_disks.steps[0].capability, "disk.list");

    let p_inspect = planner
        .plan("inspecciona el disco /dev/nvme0n1", &catalog)
        .expect("plan disk inspect");
    assert_eq!(p_inspect.steps.len(), 1);
    assert_eq!(p_inspect.steps[0].capability, "disk.inspect");
    assert_eq!(
        p_inspect.steps[0].args.get("device").map(|s| s.as_str()),
        Some("/dev/nvme0n1")
    );

    let p_part = planner
        .plan("particiona el disco /dev/sda en modo limpio", &catalog)
        .expect("plan disk partition");
    assert_eq!(p_part.steps.len(), 1);
    assert_eq!(p_part.steps[0].capability, "disk.partition");
    assert_eq!(
        p_part.steps[0].args.get("device").map(|s| s.as_str()),
        Some("/dev/sda")
    );
    assert_eq!(
        p_part.steps[0].args.get("clean").map(|s| s.as_str()),
        Some("true")
    );

    let p_install = planner
        .plan(
            "instala antos en el disco /dev/nvme0n1 en modo dual boot",
            &catalog,
        )
        .expect("plan install deploy");
    assert_eq!(p_install.steps.len(), 1);
    assert_eq!(p_install.steps[0].capability, "install.deploy");
    assert_eq!(
        p_install.steps[0]
            .args
            .get("target_device")
            .map(|s| s.as_str()),
        Some("/dev/nvme0n1")
    );
    assert_eq!(
        p_install.steps[0].args.get("clean").map(|s| s.as_str()),
        Some("false")
    );

    let p_probe = planner
        .plan(
            "sondea los sistemas operativos en /boot/efi para dual boot",
            &catalog,
        )
        .expect("plan bootloader probe");
    assert_eq!(p_probe.steps.len(), 1);
    assert_eq!(p_probe.steps[0].capability, "bootloader.probe");

    let p_bootloader = planner
        .plan(
            "instala el gestor de arranque uefi en el disco /dev/nvme0n1",
            &catalog,
        )
        .expect("plan bootloader install");
    assert_eq!(p_bootloader.steps.len(), 1);
    assert_eq!(p_bootloader.steps[0].capability, "bootloader.install");
    assert_eq!(
        p_bootloader.steps[0]
            .args
            .get("target_device")
            .map(|s| s.as_str()),
        Some("/dev/nvme0n1")
    );

    let p_vm_spawn = planner
        .plan(
            "arranca una microvm aislada con 4 cpus y 1024 memoria",
            &catalog,
        )
        .expect("plan vm spawn");
    assert_eq!(p_vm_spawn.steps.len(), 1);
    assert_eq!(p_vm_spawn.steps[0].capability, "microvm.spawn");
    assert_eq!(
        p_vm_spawn.steps[0].args.get("cpus").map(|s| s.as_str()),
        Some("4")
    );
    assert_eq!(
        p_vm_spawn.steps[0].args.get("memory").map(|s| s.as_str()),
        Some("1024")
    );

    let p_vm_exec = planner
        .plan(
            "ejecuta en la microvm vm-test el comando uname -a",
            &catalog,
        )
        .expect("plan vm exec");
    assert_eq!(p_vm_exec.steps.len(), 1);
    assert_eq!(p_vm_exec.steps[0].capability, "microvm.exec");
    assert_eq!(
        p_vm_exec.steps[0].args.get("vm_id").map(|s| s.as_str()),
        Some("vm-test")
    );

    let p_vm_kill = planner
        .plan("destruye la microvm vm-test", &catalog)
        .expect("plan vm destroy");
    assert_eq!(p_vm_kill.steps.len(), 1);
    assert_eq!(p_vm_kill.steps[0].capability, "microvm.destroy");
    assert_eq!(
        p_vm_kill.steps[0].args.get("vm_id").map(|s| s.as_str()),
        Some("vm-test")
    );

    // antpkg planning tests (T16.2)
    let p_pkg_inst = planner
        .plan("instala el paquete ripgrep", &catalog)
        .expect("plan pkg install");
    assert_eq!(p_pkg_inst.steps.len(), 1);
    assert_eq!(p_pkg_inst.steps[0].capability, "pkg.install");
    assert_eq!(
        p_pkg_inst.steps[0].args.get("package").map(|s| s.as_str()),
        Some("ripgrep")
    );

    let p_pkg_rm = planner
        .plan("desinstala el paquete curl", &catalog)
        .expect("plan pkg remove");
    assert_eq!(p_pkg_rm.steps.len(), 1);
    assert_eq!(p_pkg_rm.steps[0].capability, "pkg.remove");
    assert_eq!(
        p_pkg_rm.steps[0].args.get("package").map(|s| s.as_str()),
        Some("curl")
    );

    let p_pkg_rb = planner
        .plan("haz rollback de paquetes a la generacion 2", &catalog)
        .expect("plan pkg rollback");
    assert_eq!(p_pkg_rb.steps.len(), 1);
    assert_eq!(p_pkg_rb.steps[0].capability, "pkg.rollback");
    assert_eq!(
        p_pkg_rb.steps[0].args.get("generation").map(|s| s.as_str()),
        Some("2")
    );

    let p_pkg_ls = planner
        .plan("lista los paquetes instalados", &catalog)
        .expect("plan pkg list");
    assert_eq!(p_pkg_ls.steps.len(), 1);
    assert_eq!(p_pkg_ls.steps[0].capability, "pkg.list");

    let p_pkg_vf = planner
        .plan("verifica los paquetes antpkg", &catalog)
        .expect("plan pkg verify");
    assert_eq!(p_pkg_vf.steps.len(), 1);
    assert_eq!(p_pkg_vf.steps[0].capability, "pkg.verify");
}

#[test]
fn test_plan_autopilot_sentinela() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_start = planner
        .plan(
            "inicia el modo autopilot continuo con intervalo 10",
            &catalog,
        )
        .expect("plan autopilot start");
    assert_eq!(p_start.steps.len(), 1);
    assert_eq!(p_start.steps[0].capability, "autopilot.start");
    assert_eq!(
        p_start.steps[0].args.get("interval").map(|s| s.as_str()),
        Some("10")
    );

    let p_status = planner
        .plan("muestra el estado de autopilot", &catalog)
        .expect("plan autopilot status");
    assert_eq!(p_status.steps.len(), 1);
    assert_eq!(p_status.steps[0].capability, "autopilot.status");

    let p_scan = planner
        .plan("escanea el workspace con el centinela", &catalog)
        .expect("plan autopilot scan");
    assert_eq!(p_scan.steps.len(), 1);
    assert_eq!(p_scan.steps[0].capability, "autopilot.scan");

    let p_resolve = planner
        .plan("aprueba la propuesta del incidente inc-42", &catalog)
        .expect("plan autopilot resolve");
    assert_eq!(p_resolve.steps.len(), 1);
    assert_eq!(p_resolve.steps[0].capability, "autopilot.resolve");
    assert_eq!(
        p_resolve.steps[0]
            .args
            .get("incident_id")
            .map(|s| s.as_str()),
        Some("inc-42")
    );
    assert_eq!(
        p_resolve.steps[0].args.get("approve").map(|s| s.as_str()),
        Some("true")
    );

    let p_stop = planner
        .plan("deten el modo autopilot", &catalog)
        .expect("plan autopilot stop");
    assert_eq!(p_stop.steps.len(), 1);
    assert_eq!(p_stop.steps[0].capability, "autopilot.stop");
}

#[test]
fn test_plan_web_console() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_start = planner
        .plan("inicia el servidor web en puerto 9000", &catalog)
        .expect("plan web start");
    assert_eq!(p_start.steps.len(), 1);
    assert_eq!(p_start.steps[0].capability, "web.start");
    assert_eq!(
        p_start.steps[0].args.get("port").map(|s| s.as_str()),
        Some("9000")
    );

    let p_status = planner
        .plan("muestra el estado del servidor web", &catalog)
        .expect("plan web status");
    assert_eq!(p_status.steps.len(), 1);
    assert_eq!(p_status.steps[0].capability, "web.status");

    let p_token = planner
        .plan("genera un token web para laptop", &catalog)
        .expect("plan web token");
    assert_eq!(p_token.steps.len(), 1);
    assert_eq!(p_token.steps[0].capability, "web.token");
    assert_eq!(
        p_token.steps[0].args.get("label").map(|s| s.as_str()),
        Some("laptop")
    );

    let p_stop = planner
        .plan("deten la consola web", &catalog)
        .expect("plan web stop");
    assert_eq!(p_stop.steps.len(), 1);
    assert_eq!(p_stop.steps[0].capability, "web.stop");
}

#[test]
fn test_plan_open_default_editor_neovim() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_edit = planner
        .plan("abre el editor para src/lib.rs", &catalog)
        .expect("plan editor");
    assert_eq!(p_edit.steps.len(), 1);
    assert_eq!(p_edit.steps[0].capability, "ui.terminal");
    assert!(p_edit.steps[0]
        .args
        .get("command")
        .unwrap()
        .contains("nvim src/lib.rs"));

    let p_nvim = planner
        .plan("editar en neovim", &catalog)
        .expect("plan nvim");
    assert_eq!(p_nvim.steps.len(), 1);
    assert_eq!(p_nvim.steps[0].capability, "ui.terminal");
    assert!(p_nvim.steps[0]
        .args
        .get("command")
        .unwrap()
        .contains("nvim"));
}

#[test]
fn test_plan_dev_workspace() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_dev = planner
        .plan("inicia el espacio de trabajo para api-service", &catalog)
        .expect("plan dev workspace");
    assert_eq!(p_dev.steps.len(), 1);
    assert_eq!(p_dev.steps[0].capability, "dev.workspace");
    assert_eq!(
        p_dev.steps[0].args.get("project").map(|s| s.as_str()),
        Some("api-service")
    );

    let p_tui = planner
        .plan("abrir dev tui", &catalog)
        .expect("plan dev tui");
    assert_eq!(p_tui.steps.len(), 1);
    assert_eq!(p_tui.steps[0].capability, "dev.workspace");
}

#[test]
fn test_plan_reproduce_and_testgen() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_rep = planner
        .plan(
            "reproduce el error: thread 'main' panicked at 'index out of bounds', src/lib.rs:12:4",
            &catalog,
        )
        .expect("plan reproduce");
    assert_eq!(p_rep.steps.len(), 1);
    assert_eq!(p_rep.steps[0].capability, "test.reproduce");
    assert!(p_rep.steps[0]
        .args
        .get("error")
        .unwrap()
        .contains("panicked at"));

    let p_gen = planner
        .plan("genera tests para src/service.rs", &catalog)
        .expect("plan testgen");
    assert_eq!(p_gen.steps.len(), 1);
    assert_eq!(p_gen.steps[0].capability, "test.gen");
    assert_eq!(
        p_gen.steps[0].args.get("target").map(|s| s.as_str()),
        Some("src/service.rs")
    );
}

#[test]
fn test_plan_ci_and_git_hooks() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_ci = planner
        .plan("ejecuta ci rápido", &catalog)
        .expect("plan ci");
    assert_eq!(p_ci.steps.len(), 1);
    assert_eq!(p_ci.steps[0].capability, "ci.run");
    assert_eq!(
        p_ci.steps[0].args.get("fast").map(|s| s.as_str()),
        Some("true")
    );

    let p_hook = planner
        .plan("instala pre-commit hook", &catalog)
        .expect("plan hook");
    assert_eq!(p_hook.steps.len(), 1);
    assert_eq!(p_hook.steps[0].capability, "git.hook");
    assert_eq!(
        p_hook.steps[0].args.get("action").map(|s| s.as_str()),
        Some("install")
    );
}

#[test]
fn test_plan_dev_snapshots() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_create = planner
        .plan("crea snapshot llamado pre-refactor", &catalog)
        .expect("plan snapshot create");
    assert_eq!(p_create.steps.len(), 1);
    assert_eq!(p_create.steps[0].capability, "snapshot.create");
    assert_eq!(
        p_create.steps[0].args.get("label").map(|s| s.as_str()),
        Some("pre-refactor")
    );

    let p_list = planner
        .plan("lista snapshots del time machine", &catalog)
        .expect("plan snapshot list");
    assert_eq!(p_list.steps.len(), 1);
    assert_eq!(p_list.steps[0].capability, "snapshot.list");

    let p_restore = planner
        .plan("restaura snapshot pre-refactor", &catalog)
        .expect("plan snapshot restore");
    assert_eq!(p_restore.steps.len(), 1);
    assert_eq!(p_restore.steps[0].capability, "snapshot.restore");
    assert_eq!(
        p_restore.steps[0].args.get("id").map(|s| s.as_str()),
        Some("pre-refactor")
    );
}

#[test]
fn test_plan_benchmarking() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_run = planner
        .plan("ejecuta benchmarks del proyecto", &catalog)
        .expect("plan bench run");
    assert_eq!(p_run.steps.len(), 1);
    assert_eq!(p_run.steps[0].capability, "bench.run");

    let p_diff = planner
        .plan("compara rendimiento contra master", &catalog)
        .expect("plan bench diff");
    assert_eq!(p_diff.steps.len(), 1);
    assert_eq!(p_diff.steps[0].capability, "bench.diff");
    assert_eq!(
        p_diff.steps[0].args.get("against").map(|s| s.as_str()),
        Some("master")
    );

    let p_hist = planner
        .plan("muestra historial de rendimiento", &catalog)
        .expect("plan bench history");
    assert_eq!(p_hist.steps.len(), 1);
    assert_eq!(p_hist.steps[0].capability, "bench.history");
}

#[test]
fn test_plan_forge_issues_and_prs() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_list = planner
        .plan("lista issues remotos", &catalog)
        .expect("plan issue list");
    assert_eq!(p_list.steps.len(), 1);
    assert_eq!(p_list.steps[0].capability, "issue.list");

    let p_import = planner
        .plan("importa issue 42", &catalog)
        .expect("plan issue import");
    assert_eq!(p_import.steps.len(), 1);
    assert_eq!(p_import.steps[0].capability, "issue.import");
    assert_eq!(
        p_import.steps[0].args.get("id").map(|s| s.as_str()),
        Some("42")
    );

    let p_pr = planner
        .plan("crea pull request", &catalog)
        .expect("plan pr create");
    assert_eq!(p_pr.steps.len(), 1);
    assert_eq!(p_pr.steps[0].capability, "pr.create");

    let p_status = planner
        .plan("revisa estado del pull request", &catalog)
        .expect("plan pr status");
    assert_eq!(p_status.steps.len(), 1);
    assert_eq!(p_status.steps[0].capability, "pr.status");
}

#[test]
fn test_plan_doc_arch_and_sync() {
    let ctx = Ctx::discover().expect("ctx");
    let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
    let planner = LocalPlanner;

    let p_arch = planner
        .plan("genera diagrama de arquitectura", &catalog)
        .expect("plan doc arch");
    assert_eq!(p_arch.steps.len(), 1);
    assert_eq!(p_arch.steps[0].capability, "doc.arch");

    let p_sync = planner
        .plan("sincroniza documentacion de arquitectura", &catalog)
        .expect("plan doc sync");
    assert_eq!(p_sync.steps.len(), 1);
    assert_eq!(p_sync.steps[0].capability, "doc.sync");

    let p_check = planner
        .plan("verifica documentacion de arquitectura", &catalog)
        .expect("plan doc check");
    assert_eq!(p_check.steps.len(), 1);
    assert_eq!(p_check.steps[0].capability, "doc.check");
}
