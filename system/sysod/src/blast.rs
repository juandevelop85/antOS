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
    /// Rutas dentro de la configuración declarativa del sistema.
    pub system: BTreeSet<PathBuf>,
    /// De las rutas escritas, cuáles son directorios. Se declara con una
    /// barra final en el manifiesto en vez de adivinarse: hay recintos que
    /// necesitan que el directorio exista para poder ponerle una regla, y
    /// deducirlo del nombre sería exactamente el tipo de suposición que este
    /// diseño trata de eliminar.
    pub dirs: BTreeSet<PathBuf>,
    pub declared_tier: Tier,
    pub irreversible: bool,
}

impl Blast {
    pub fn compute(
        plan: &Plan,
        catalog: &Catalog,
        workspace: &Path,
        system_config: &Path,
    ) -> Result<Self> {
        let mut b = Blast { declared_tier: Tier::Auto, ..Default::default() };

        let mut dirs = BTreeSet::new();

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
                    let (resolved, is_dir) = resolve(tpl, step, workspace, system_config);
                    if resolved.starts_with(system_config) {
                        b.system.insert(resolved.clone());
                    } else if !resolved.starts_with(workspace) {
                        b.escapes.insert(resolved.clone());
                    }
                    if is_dir {
                        dirs.insert(resolved.clone());
                    }
                    bucket.insert(resolved);
                }
            }
        }

        // Escribir en una ruta implica crear los directorios que le faltan.
        // No es una excepción concedida a nadie: es lo que «escribir aquí»
        // significa de verdad, y se hace explícito para que aparezca en el
        // radio de impacto, se fotografíe y el recinto lo permita.
        //
        // Sigue estando acotado: solo directorios inexistentes, solo dentro
        // del espacio de trabajo, y solo en la rama de una ruta ya declarada.
        // Crear un fichero dentro de la configuración del sistema exige
        // permiso sobre su directorio. Es el mismo razonamiento que los
        // ancestros del espacio de trabajo, y por eso se declara igual.
        for path in b.system.clone() {
            if let Some(parent) = path.parent() {
                b.writes.insert(parent.to_path_buf());
                dirs.insert(parent.to_path_buf());
            }
        }

        let ancestors = missing_ancestors(&b.writes, workspace);
        // Un ancestro que falta siempre es un directorio: por definición
        // cuelga algo de él.
        dirs.extend(ancestors.iter().cloned());
        b.writes.extend(ancestors);
        b.dirs = dirs.intersection(&b.writes).cloned().collect();
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
        if !self.system.is_empty() {
            if tier < Tier::Grant {
                reasons.push("modifica la configuración del sistema".into());
            }
            tier = tier.max(Tier::Grant);
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

fn missing_ancestors(paths: &BTreeSet<PathBuf>, workspace: &Path) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    for path in paths {
        let mut cursor = path.parent();
        while let Some(dir) = cursor {
            // Al llegar al espacio de trabajo, o a un directorio que ya
            // existe, no hay nada más que crear.
            if dir == workspace || !dir.starts_with(workspace) || dir.exists() {
                break;
            }
            out.insert(dir.to_path_buf());
            cursor = dir.parent();
        }
    }
    out
}

fn join(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Expande una plantilla de efecto (`$WORKSPACE/{name}`) con los argumentos
/// del paso y la normaliza contra el espacio de trabajo.
///
/// Devuelve además si la plantilla la declaró como directorio, que es lo que
/// significa la barra final.
fn resolve(tpl: &str, step: &Step, workspace: &Path, system_config: &Path) -> (PathBuf, bool) {
    let expanded = expand_all(tpl, &step.args, workspace, system_config);
    let is_dir = expanded.ends_with('/');
    let p = PathBuf::from(expanded.trim_end_matches('/'));
    let absolute = if p.is_absolute() { p } else { workspace.join(p) };
    (normalize(&absolute), is_dir)
}

pub fn expand_all(
    tpl: &str,
    args: &BTreeMap<String, String>,
    workspace: &Path,
    system_config: &Path,
) -> String {
    let s = tpl.replace("$SYSTEM_CONFIG", &system_config.to_string_lossy());
    expand(&s, args, workspace)
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
        let blast = Blast::compute(&plan_leyendo("../../etc/passwd"), &catalogo_sin_restricciones(), &ws, Path::new("/tmp/sin-configuracion")).unwrap();

        assert!(!blast.escapes.is_empty(), "una ruta fuera del espacio de trabajo debe registrarse como fuga");
        let (tier, reasons) = blast.required_tier();
        assert_eq!(tier, Tier::Grant, "una fuga eleva el nivel al máximo");
        assert!(reasons.iter().any(|r| r.contains("espacio de trabajo")));
    }

    #[test]
    fn una_ruta_de_dentro_no_escala_el_nivel() {
        let ws = PathBuf::from("/tmp/espacio");
        let blast = Blast::compute(&plan_leyendo("proyecto/src/main.rs"), &catalogo_sin_restricciones(), &ws, Path::new("/tmp/sin-configuracion")).unwrap();

        assert!(blast.escapes.is_empty());
        assert_eq!(blast.required_tier().0, Tier::Auto, "solo leer dentro del espacio no requiere permiso");
    }

    #[test]
    fn escribir_hondo_declara_los_directorios_que_hay_que_crear() {
        let ws = std::env::temp_dir().join("syso-prueba-ancestros");
        let _ = std::fs::create_dir_all(&ws);

        let mut catalog = catalogo_sin_restricciones();
        let cap = catalog.caps.get_mut("t.leer").unwrap();
        cap.effects.reads.clear();
        cap.effects.writes = vec!["{path}".into()];

        let blast = Blast::compute(&plan_leyendo("uno/dos/fichero.txt"), &catalog, &ws, Path::new("/tmp/sin-configuracion")).unwrap();

        assert!(blast.writes.contains(&ws.join("uno")), "el ancestro que falta debe declararse");
        assert!(blast.writes.contains(&ws.join("uno/dos")));
        assert!(blast.dirs.contains(&ws.join("uno")), "un ancestro siempre es directorio");
        assert!(!blast.dirs.contains(&ws.join("uno/dos/fichero.txt")), "la hoja no lo es");
        assert!(!blast.writes.contains(&ws), "el espacio de trabajo ya existe: no se declara");
        assert!(blast.escapes.is_empty(), "los ancestros siguen dentro del espacio de trabajo");
    }

    #[test]
    fn la_configuracion_del_sistema_no_es_una_fuga_pero_exige_concesion() {
        let ws = PathBuf::from("/tmp/espacio");
        let sistema = PathBuf::from("/tmp/configuracion");

        let mut catalog = catalogo_sin_restricciones();
        let cap = catalog.caps.get_mut("t.leer").unwrap();
        cap.effects.reads.clear();
        cap.effects.writes = vec!["/tmp/configuracion/paquetes.nix".into()];

        let blast = Blast::compute(&plan_leyendo("da igual"), &catalog, &ws, &sistema).unwrap();

        assert!(
            blast.escapes.is_empty(),
            "una raíz declarada no es una fuga aunque esté fuera del espacio de trabajo"
        );
        assert!(!blast.system.is_empty(), "debe reconocerse como ruta de sistema");

        let (tier, reasons) = blast.required_tier();
        assert_eq!(tier, Tier::Grant, "tocar el sistema siempre exige concesión");
        assert!(reasons.iter().any(|r| r.contains("sistema")));
    }

    #[test]
    fn la_barra_final_declara_un_directorio() {
        let ws = PathBuf::from("/tmp/espacio");
        let mut catalog = catalogo_sin_restricciones();
        let cap = catalog.caps.get_mut("t.leer").unwrap();
        cap.effects.reads.clear();
        cap.effects.writes = vec!["{path}/".into()];

        let blast = Blast::compute(&plan_leyendo("proyecto"), &catalog, &ws, Path::new("/tmp/sin-configuracion")).unwrap();
        assert!(blast.dirs.contains(&ws.join("proyecto")));
        assert!(!blast.dirs.iter().any(|d| d.to_string_lossy().ends_with('/')),
                "la barra es una marca, no parte de la ruta");
    }

    #[test]
    fn los_efectos_declarados_elevan_el_nivel_por_encima_del_manifiesto() {
        // La capacidad se declara "auto", pero escribe: el nivel sube solo.
        let mut catalog = catalogo_sin_restricciones();
        let cap = catalog.caps.get_mut("t.leer").unwrap();
        cap.effects.reads.clear();
        cap.effects.writes = vec!["{path}".into()];

        let ws = PathBuf::from("/tmp/espacio");
        let blast = Blast::compute(&plan_leyendo("dentro.txt"), &catalog, &ws, Path::new("/tmp/sin-configuracion")).unwrap();

        let (tier, reasons) = blast.required_tier();
        assert_eq!(tier, Tier::Confirm, "escribir exige confirmación aunque el manifiesto diga auto");
        assert!(reasons.iter().any(|r| r.contains("escribe")));
    }
}
