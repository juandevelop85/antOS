//! Radio de impacto: qué va a tocar un plan, calculado ANTES de ejecutarlo.
//!
//! Es la pieza central del diseño. Como cada capacidad declara sus efectos,
//! se puede saber el alcance de un plan sin ejecutarlo, y de ahí salen tanto
//! el nivel de permiso como la lista de rutas que hay que fotografiar.

use crate::capability::{Catalog, Reversible, Tier};
use crate::plan::{Plan, Step};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Default)]
pub struct Blast {
    pub reads: BTreeSet<PathBuf>,
    pub writes: BTreeSet<PathBuf>,
    pub deletes: BTreeSet<PathBuf>,
    pub network: BTreeSet<String>,
    /// Rutas que se salen del espacio de trabajo. Nunca deberían existir,
    /// y que existan es exactamente lo que hay que detectar aquí.
    pub escapes: BTreeSet<PathBuf>,
    pub declared_tier: Tier,
    pub irreversible: bool,
}

impl Blast {
    pub fn compute(plan: &Plan, catalog: &Catalog, workspace: &Path) -> Result<Self> {
        let mut b = Blast { declared_tier: Tier::Auto, ..Default::default() };

        for step in &plan.steps {
            let cap = catalog.get(&step.capability)?;
            b.declared_tier = b.declared_tier.max(cap.policy.tier);
            if cap.policy.reversible == Reversible::Never {
                b.irreversible = true;
            }

            b.network.extend(cap.effects.network.iter().cloned());

            for (templates, bucket) in [
                (&cap.effects.reads, &mut b.reads),
                (&cap.effects.writes, &mut b.writes),
                (&cap.effects.deletes, &mut b.deletes),
            ] {
                for tpl in templates {
                    let resolved = resolve(tpl, step, workspace);
                    if !resolved.starts_with(workspace) {
                        b.escapes.insert(resolved.clone());
                    }
                    bucket.insert(resolved);
                }
            }
        }
        Ok(b)
    }

    /// La derivación del nivel de permiso.
    ///
    /// El nivel declarado en el manifiesto es un suelo, no la última palabra:
    /// syso lo eleva según lo que el plan vaya a tocar de verdad. Por eso no
    /// se puede negociar con una frase persuasiva — no lo decide el modelo.
    pub fn required_tier(&self) -> (Tier, Vec<String>) {
        let mut tier = self.declared_tier;
        let mut reasons = Vec::new();

        if !self.writes.is_empty() {
            if tier < Tier::Confirm {
                reasons.push(format!("escribe {} ruta(s)", self.writes.len()));
            }
            tier = tier.max(Tier::Confirm);
        }
        if !self.network.is_empty() {
            if tier < Tier::Confirm {
                reasons.push(format!("sale a la red: {}", join(&self.network)));
            }
            tier = tier.max(Tier::Confirm);
        }
        if !self.deletes.is_empty() {
            if tier < Tier::Grant {
                reasons.push(format!("borra {} ruta(s)", self.deletes.len()));
            }
            tier = tier.max(Tier::Grant);
        }
        if !self.escapes.is_empty() {
            reasons.push(format!(
                "sale del espacio de trabajo: {}",
                self.escapes.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ));
            tier = Tier::Grant;
        }
        if self.irreversible && tier < Tier::Grant {
            reasons.push("hay pasos irreversibles".into());
            tier = Tier::Grant;
        }

        if reasons.is_empty() {
            reasons.push(format!("nivel declarado por el catálogo: {}", tier.label()));
        }
        (tier, reasons)
    }

    /// Lo que hay que fotografiar antes de ejecutar: todo lo que se escribe
    /// o se borra. Lo que solo se lee no necesita instantánea.
    pub fn paths_to_snapshot(&self) -> Vec<PathBuf> {
        self.writes.union(&self.deletes).cloned().collect()
    }
}

fn join(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Expande una plantilla de efecto (`$WORKSPACE/{name}`) con los argumentos
/// del paso y la normaliza contra el espacio de trabajo.
fn resolve(tpl: &str, step: &Step, workspace: &Path) -> PathBuf {
    let expanded = expand(tpl, &step.args, workspace);
    let p = PathBuf::from(expanded);
    let absolute = if p.is_absolute() { p } else { workspace.join(p) };
    normalize(&absolute)
}

pub fn expand(tpl: &str, args: &BTreeMap<String, String>, workspace: &Path) -> String {
    let mut s = tpl.replace("$WORKSPACE", &workspace.to_string_lossy());
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Normalización puramente léxica: resuelve `.` y `..` sin tocar el disco.
///
/// Tiene que ser léxica porque las rutas de un plan aún no existen. Si
/// dependiéramos de canonicalize(), un `../../etc/passwd` pasaría la
/// comprobación por el simple hecho de no existir todavía.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{Capability, Effects, Policy, Reversible};
    use crate::plan::Plan;

    /// Capacidad de prueba con un parámetro de ruta SIN `within`: la
    /// validación temprana no tiene nada que comprobar, así que la única
    /// defensa que queda es el cálculo del radio de impacto.
    fn catalogo_sin_restricciones() -> Catalog {
        let cap = Capability {
            name: "t.leer".into(),
            summary: String::new(),
            params: BTreeMap::new(),
            effects: Effects {
                reads: vec!["{path}".into()],
                ..Default::default()
            },
            policy: Policy {
                tier: Tier::Auto,
                reversible: Reversible::Unnecessary,
            },
        };
        let mut caps = BTreeMap::new();
        caps.insert(cap.name.clone(), cap);
        Catalog { caps }
    }

    fn plan_leyendo(path: &str) -> Plan {
        Plan {
            id: "prueba".into(),
            intent: String::new(),
            planner: "prueba".into(),
            steps: vec![Step {
                capability: "t.leer".into(),
                args: [("path".to_string(), path.to_string())].into_iter().collect(),
            }],
        }
    }

    #[test]
    fn detecta_la_fuga_aunque_el_parametro_no_este_restringido() {
        let ws = PathBuf::from("/tmp/espacio");
        let blast = Blast::compute(&plan_leyendo("../../etc/passwd"), &catalogo_sin_restricciones(), &ws).unwrap();

        assert!(!blast.escapes.is_empty(), "una ruta fuera del espacio de trabajo debe registrarse como fuga");
        let (tier, reasons) = blast.required_tier();
        assert_eq!(tier, Tier::Grant, "una fuga eleva el nivel al máximo");
        assert!(reasons.iter().any(|r| r.contains("espacio de trabajo")));
    }

    #[test]
    fn una_ruta_de_dentro_no_escala_el_nivel() {
        let ws = PathBuf::from("/tmp/espacio");
        let blast = Blast::compute(&plan_leyendo("proyecto/src/main.rs"), &catalogo_sin_restricciones(), &ws).unwrap();

        assert!(blast.escapes.is_empty());
        assert_eq!(blast.required_tier().0, Tier::Auto, "solo leer dentro del espacio no requiere permiso");
    }

    #[test]
    fn los_efectos_declarados_elevan_el_nivel_por_encima_del_manifiesto() {
        // La capacidad se declara "auto", pero escribe: el nivel sube solo.
        let mut catalog = catalogo_sin_restricciones();
        let cap = catalog.caps.get_mut("t.leer").unwrap();
        cap.effects.reads.clear();
        cap.effects.writes = vec!["{path}".into()];

        let ws = PathBuf::from("/tmp/espacio");
        let blast = Blast::compute(&plan_leyendo("dentro.txt"), &catalog, &ws).unwrap();

        let (tier, reasons) = blast.required_tier();
        assert_eq!(tier, Tier::Confirm, "escribir exige confirmación aunque el manifiesto diga auto");
        assert!(reasons.iter().any(|r| r.contains("escribe")));
    }
}
