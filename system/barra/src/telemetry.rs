//! Consulta periódica de telemetría consolidada del demonio (eBPF, Profiler,
//! Pair Programming, antMesh) para los badges de la cabecera de la barra.

use crate::session::run_offthread;
use crate::socket_path;
use antos_protocol::{Event, Request};
use gtk4::prelude::*;
use gtk4::Label;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

/// Asynchronously queries consolidated system telemetry (eBPF, Profiler, Pair, Mesh).
pub(crate) fn query_telemetry_async(
    ebpf_badge: Label,
    profiler_badge: Label,
    pair_badge: Label,
    mesh_badge: Label,
) {
    run_offthread(
        || -> Option<antos_protocol::BarraTelemetry> {
            let path = socket_path();
            let mut stream = UnixStream::connect(&path).ok()?;

            let req = Request::QueryBarraTelemetry;
            let json = serde_json::to_string(&req).ok()?;
            writeln!(stream, "{json}").ok()?;
            stream.flush().ok()?;

            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).ok()?;

            match serde_json::from_str::<Event>(line.trim()) {
                Ok(Event::BarraTelemetryStatus(t)) => Some(t),
                _ => None,
            }
        },
        move |telemetry| {
            if let Some(t) = telemetry {
                update_telemetry_badges(&ebpf_badge, &profiler_badge, &pair_badge, &mesh_badge, &t);
            }
        },
    );
}

fn update_telemetry_badges(
    ebpf_badge: &Label,
    profiler_badge: &Label,
    pair_badge: &Label,
    mesh_badge: &Label,
    telemetry: &antos_protocol::BarraTelemetry,
) {
    // 1. eBPF LSM Guard
    if telemetry.ebpf_violations_count > 0 {
        ebpf_badge.set_text(&format!("🛡️ eBPF: !{}", telemetry.ebpf_violations_count));
        ebpf_badge.remove_css_class("active");
        ebpf_badge.add_css_class("alert");
    } else if telemetry.ebpf_lsm_active {
        ebpf_badge.set_text("🛡️ LSM");
        ebpf_badge.remove_css_class("alert");
        ebpf_badge.add_css_class("active");
    } else {
        ebpf_badge.set_text("🛡️ eBPF");
        ebpf_badge.remove_css_class("alert");
        ebpf_badge.remove_css_class("active");
    }

    // 2. Profiler Memory & CPU
    let rss_mb = telemetry.profiler_rss_bytes as f64 / (1024.0 * 1024.0);
    profiler_badge.set_text(&format!("⚡ {:.0}M", rss_mb));
    if telemetry.profiler_cpu_percent > 50.0 {
        profiler_badge.add_css_class("alert");
    } else {
        profiler_badge.remove_css_class("alert");
    }

    // 3. Pair Programming / Coder
    if let Some(ref session) = telemetry.active_pair_session {
        pair_badge.set_text(&format!("👥 Coder ({session})"));
        pair_badge.add_css_class("active");
    } else {
        pair_badge.set_text("👥 Coder");
        pair_badge.remove_css_class("active");
    }

    // 4. antMesh P2P Nodes
    if telemetry.mesh_peers_count > 0 {
        mesh_badge.set_text(&format!("🌐 {} peers", telemetry.mesh_peers_count));
        mesh_badge.add_css_class("active");
    } else {
        mesh_badge.set_text("🌐 Mesh");
        mesh_badge.remove_css_class("active");
    }
}
