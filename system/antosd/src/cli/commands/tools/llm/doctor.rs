//! `antos llm doctor` (T34.2): una pantalla con el servicio, la RAM, el tier
//! recomendado, los modelos con sus capacidades y el contexto efectivo.

use super::models::client_for;
use crate::ctx::Ctx;
use crate::llm::doctor::{DoctorReport, ModelTable};
use crate::llm::LlmConfig;
use crate::terminal::{paint, BOLD};
use anyhow::Result;

/// Recoge el informe para el Ollama y el modelo activos.
pub fn collect(ctx: &Ctx, config: &LlmConfig) -> DoctorReport {
    let client = client_for(ctx, config);
    let table = ModelTable::load(ctx.antos_root.as_deref());
    let active_model = config
        .get_provider_settings("ollama")
        .and_then(|s| s.model.clone());
    DoctorReport::collect(
        &client,
        &table,
        active_model.as_deref(),
        config.requested_num_ctx(None),
    )
}

pub fn cmd_doctor(ctx: &Ctx, config: &LlmConfig) -> Result<()> {
    println!(
        "\n{}\n",
        paint(
            "antOS · Diagnóstico del motor local (antos llm doctor)",
            BOLD
        )
    );
    let report = collect(ctx, config);
    for line in report.render().lines() {
        println!("  {line}");
    }
    println!();
    Ok(())
}
