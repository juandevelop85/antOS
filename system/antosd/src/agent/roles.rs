//! Roles antFlow sobre el runtime de agente (T33.3): qué herramientas,
//! qué prompt, qué presupuesto y qué salida tipada tiene cada uno.
//!
//! El Arquitecto y el Auditor solo leen; solo el Coder edita. QA no es un
//! rol con modelo: ejecuta la suite real (`exec::fs::run_tests`) y
//! realimenta el fallo al Coder. Los prompts viven en `prompts/*.txt` (en
//! español: son contenido para el modelo, T31.12).

use super::tools::FinishSpec;
use antos_protocol::{AgentBudget, AgentRole};
use serde_json::json;

/// Especificación de un rol: lo que necesita `RunConfig` para ejecutarlo.
#[derive(Debug, Clone)]
pub struct RoleSpec {
    pub role: AgentRole,
    pub toolset: Vec<String>,
    /// Toolset para modelos pequeños (T34.4): menos herramientas, menos
    /// confusión. El runtime lo elige si el proveedor declara `Small`.
    pub toolset_compact: Vec<String>,
    pub system_prompt: String,
    pub budget: AgentBudget,
    pub finish: FinishSpec,
}

const READ_ONLY: &[&str] = &["fs.read", "fs.list", "memory.search"];
const READ_ONLY_COMPACT: &[&str] = &["fs.read", "fs.list"];
const EDITING: &[&str] = &[
    "fs.read",
    "fs.list",
    "fs.patch",
    "fs.write",
    "test.run",
    "git.status",
    "project.scaffold",
    "project.run",
];
/// Coder con un 7B: leer, parchear, probar. Sin `fs.write` (lo confunde
/// con `fs.patch` y pisa ficheros enteros) ni `git.status`/`memory.search`.
const EDITING_COMPACT: &[&str] = &[
    "fs.read",
    "fs.list",
    "fs.patch",
    "test.run",
    "project.scaffold",
    "project.run",
];

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Presupuesto en pasos por rol si `llm_config.json` no dice otra cosa.
pub fn default_steps(role: AgentRole) -> u32 {
    match role {
        AgentRole::Architect => 8,
        AgentRole::Coder => 30,
        AgentRole::QA | AgentRole::VisualQA => 1,
        AgentRole::Auditor => 8,
    }
}

/// Construye la especificación de un rol con el presupuesto de pasos dado.
pub fn spec_for(role: AgentRole, max_steps: u32) -> RoleSpec {
    let budget = AgentBudget {
        max_steps,
        ..AgentBudget::default()
    };
    match role {
        AgentRole::Architect => RoleSpec {
            role,
            toolset: names(READ_ONLY),
            toolset_compact: names(READ_ONLY_COMPACT),
            system_prompt: include_str!("prompts/architect.txt").to_string(),
            budget,
            finish: FinishSpec {
                description: "Entrega el plan de implementación para el Coder y termina."
                    .into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "resumen": {"type": "string", "description": "Qué se va a hacer y por qué, en una frase."},
                        "files_to_touch": {"type": "array", "items": {"type": "string"},
                            "description": "Rutas relativas que el Coder puede modificar. Vacío si el ticket no es realizable."},
                        "steps": {"type": "array", "items": {"type": "string"},
                            "description": "Pasos concretos, en orden."},
                        "acceptance_checks": {"type": "array", "items": {"type": "string"},
                            "description": "Cómo verificar cada criterio de aceptación."}
                    },
                    "required": ["resumen", "files_to_touch", "steps", "acceptance_checks"]
                }),
            },
        },
        AgentRole::Coder => RoleSpec {
            role,
            toolset: names(EDITING),
            toolset_compact: names(EDITING_COMPACT),
            system_prompt: include_str!("prompts/coder.txt").to_string(),
            budget,
            finish: FinishSpec {
                description: "Da la implementación por terminada (tests en verde) o explica qué no pudiste hacer.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "resumen": {"type": "string", "description": "Qué cambiaste, qué verificaste, qué queda."}
                    },
                    "required": ["resumen"]
                }),
            },
        },
        // QA (y el QA visual de T14.2, que aquí no se orquesta) no usan
        // modelo: la suite se ejecuta directamente.
        AgentRole::QA | AgentRole::VisualQA => RoleSpec {
            role,
            toolset: vec!["test.run".into()],
            toolset_compact: vec!["test.run".into()],
            system_prompt: String::new(),
            budget,
            finish: FinishSpec {
                description: "QA no usa modelo.".into(),
                input_schema: json!({"type": "object", "properties": {"resumen": {"type": "string"}}, "required": ["resumen"]}),
            },
        },
        AgentRole::Auditor => RoleSpec {
            role,
            toolset: names(READ_ONLY),
            toolset_compact: vec!["fs.read".into()],
            system_prompt: include_str!("prompts/auditor.txt").to_string(),
            budget,
            finish: FinishSpec {
                description: "Emite el veredicto sobre el diff y termina.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "resumen": {"type": "string"},
                        "approve": {"type": "boolean"},
                        "findings": {"type": "array", "items": {"type": "string"},
                            "description": "Hallazgos accionables; vacío si apruebas."},
                        "risk": {"type": "string", "enum": ["low", "medium", "high"]}
                    },
                    "required": ["resumen", "approve", "findings", "risk"]
                }),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Criterio de T33.3: ningún rol salvo el Coder puede escribir.
    #[test]
    fn only_the_coder_can_edit() {
        for role in [AgentRole::Architect, AgentRole::Auditor, AgentRole::QA] {
            let spec = spec_for(role, 5);
            assert!(
                !spec
                    .toolset
                    .iter()
                    .any(|t| t == "fs.patch" || t == "fs.write"),
                "{role:?} no debe poder escribir"
            );
        }
        let coder = spec_for(AgentRole::Coder, 5);
        assert!(coder.toolset.iter().any(|t| t == "fs.patch"));
        assert!(coder.toolset.iter().any(|t| t == "test.run"));
        assert_eq!(coder.budget.max_steps, 5);
        // T35.3: crear proyectos también con modelos pequeños — medido con
        // el 7B: 10/10 en `scaffold-and-extend` y sin regresión en el caso
        // Rust (5/5) con `project.*` en el toolset compacto.
        assert!(coder.toolset.iter().any(|t| t == "project.scaffold"));
        assert!(coder.toolset_compact.iter().any(|t| t == "project.run"));
        assert!(crate::agent::tools::DEFAULT_TOOLSET_COMPACT.contains(&"project.scaffold"));
        // Y siguen sin `fs.write`: la razón de ser del compacto.
        assert!(!coder.toolset_compact.iter().any(|t| t == "fs.write"));
    }

    #[test]
    fn typed_finish_schemas_always_carry_a_summary() {
        for role in [AgentRole::Architect, AgentRole::Coder, AgentRole::Auditor] {
            let spec = spec_for(role, 1);
            let required = spec.finish.input_schema["required"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert!(required.iter().any(|r| r == "resumen"), "{role:?}");
        }
        assert_eq!(default_steps(AgentRole::Coder), 30);
    }
}
