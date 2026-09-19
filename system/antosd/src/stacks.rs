//! Catálogo de stacks de proyecto (T35.1): lo que antOS sabe de una
//! tecnología para crear un proyecto nuevo — lenguaje, framework, alias,
//! plantillas, comandos y toolchain — como datos en `system/stacks/*.toml`,
//! no como brazos de un `match`.
//!
//! Orden de carga: `ANTOS_STACKS=<dir>` → `<raíz de antOS>/system/stacks` →
//! la copia embebida en el binario (la imagen instalada no lleva el árbol).
//! Dentro del árbol un TOML nuevo se ve sin recompilar.
//!
//! ## Estado de implementación
//!
//! Este módulo **lee y renderiza**: valida el catálogo y produce los ficheros
//! de un proyecto. Los `commands` (`install`, `test`, `dev`, `build`) se
//! cargan y validan (vector no vacío, programa en la lista blanca) pero
//! **nadie los ejecuta todavía**: eso es T35.2. `toolchain` y `network` son
//! por ahora información para el usuario y para T35.2.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Programas que un stack puede declarar en sus comandos. T35.2 ejecutará
/// solo estos; validarlo al cargar evita que un TOML cuele `sh`.
pub const ALLOWED_PROGRAMS: &[&str] = &[
    "npm", "npx", "pnpm", "yarn", "node", "cargo", "rustc", "python3", "uv", "pip", "pytest", "go",
    "dotnet", "make",
];

/// Marcador que las plantillas sustituyen por el nombre del proyecto.
pub const NAME_PLACEHOLDER: &str = "{{name}}";

/// Los stacks que viajan con el binario. Mismo contenido que
/// `system/stacks/`; un test lo comprueba.
const EMBEDDED: &[(&str, &str)] = &[
    ("rust", include_str!("../../stacks/rust.toml")),
    ("typescript", include_str!("../../stacks/typescript.toml")),
    ("python", include_str!("../../stacks/python.toml")),
    ("nestjs", include_str!("../../stacks/nestjs.toml")),
    ("express", include_str!("../../stacks/express.toml")),
    ("nextjs", include_str!("../../stacks/nextjs.toml")),
    ("fastapi", include_str!("../../stacks/fastapi.toml")),
    ("axum", include_str!("../../stacks/axum.toml")),
    ("go", include_str!("../../stacks/go.toml")),
];

pub const STACKS_RELATIVE_PATH: &str = "system/stacks";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Toolchain {
    #[serde(default)]
    pub nix: Vec<String>,
    #[serde(default)]
    pub binaries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Commands {
    #[serde(default)]
    pub install: Vec<String>,
    #[serde(default)]
    pub test: Vec<String>,
    #[serde(default)]
    pub dev: Vec<String>,
    #[serde(default)]
    pub build: Vec<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Network {
    #[serde(default)]
    pub hosts: Vec<String>,
}

/// Un fichero de la plantilla: contenido inline o cargado de `from`
/// (ruta relativa al directorio del catálogo; solo en disco).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateFile {
    pub path: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stack {
    pub id: String,
    pub language: String,
    #[serde(default)]
    pub framework: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub toolchain: Toolchain,
    #[serde(default)]
    pub commands: Commands,
    #[serde(default)]
    pub network: Network,
    #[serde(default, rename = "file")]
    pub files: Vec<TemplateFile>,
}

impl Stack {
    /// `true` para el stack base de un lenguaje (sin framework).
    pub fn is_base(&self) -> bool {
        self.framework.is_empty()
    }

    /// `language` o `language/framework`, para mensajes.
    pub fn label(&self) -> String {
        if self.is_base() {
            self.language.clone()
        } else {
            format!("{}/{}", self.language, self.framework)
        }
    }

    /// Los ficheros del proyecto con `{{name}}` sustituido, como
    /// `(ruta relativa, contenido)`.
    pub fn render(&self, name: &str) -> Vec<(String, String)> {
        self.files
            .iter()
            .map(|f| {
                let content = f.content.clone().unwrap_or_default();
                (
                    f.path.replace(NAME_PLACEHOLDER, name),
                    content.replace(NAME_PLACEHOLDER, name),
                )
            })
            .collect()
    }

    /// El `.antos/project.toml` que el andamio deja en el proyecto: la
    /// fuente de verdad del stack para `test.run`, `ci` y los agentes
    /// (T35.3). Se escribe con `toml`, no a mano.
    pub fn project_manifest(&self, name: &str) -> Result<String> {
        let manifest = ProjectManifest {
            name: name.to_string(),
            stack: self.id.clone(),
            language: self.language.clone(),
            framework: self.framework.clone(),
            commands: Commands {
                install: substitute_all(&self.commands.install, name),
                test: substitute_all(&self.commands.test, name),
                dev: substitute_all(&self.commands.dev, name),
                build: substitute_all(&self.commands.build, name),
                port: self.commands.port,
            },
            toolchain: self.toolchain.clone(),
            created_by: format!("antos project.scaffold (T35.1) · stack {}", self.id),
        };
        let body = toml::to_string_pretty(&manifest).context("serializando project.toml")?;
        Ok(format!(
            "# Proyecto creado por antOS. Lo leen `test.run`, `ci` y los agentes\n\
             # para no adivinar el stack (T35.3). Editarlo a mano es correcto.\n{body}"
        ))
    }

    fn validate(&self, source: &str) -> Result<()> {
        if self.id.is_empty() || !is_slug(&self.id) {
            bail!("{source}: `id` «{}» no es un identificador válido", self.id);
        }
        if self.language.is_empty() || !is_slug(&self.language) {
            bail!("{source}: `language` «{}» no es válido", self.language);
        }
        if !self.framework.is_empty() && !is_slug(&self.framework) {
            bail!("{source}: `framework` «{}» no es válido", self.framework);
        }
        for a in &self.aliases {
            if a.trim().is_empty() {
                bail!("{source}: alias vacío");
            }
        }
        if self.files.is_empty() {
            bail!("{source}: un stack necesita al menos un [[file]]");
        }
        for f in &self.files {
            let p = Path::new(&f.path);
            if f.path.is_empty()
                || p.is_absolute()
                || p.components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                bail!(
                    "{source}: [[file]] path «{}» debe ser relativo y sin `..`",
                    f.path
                );
            }
            if f.content.is_none() && f.from.is_none() {
                bail!("{source}: [[file]] «{}» sin `content` ni `from`", f.path);
            }
        }
        for (what, cmd) in [
            ("install", &self.commands.install),
            ("test", &self.commands.test),
            ("dev", &self.commands.dev),
            ("build", &self.commands.build),
        ] {
            if cmd.is_empty() {
                continue;
            }
            let program = cmd[0].as_str();
            if !ALLOWED_PROGRAMS.contains(&program) {
                bail!(
                    "{source}: commands.{what} usa «{program}», que no está en la lista blanca ({})",
                    ALLOWED_PROGRAMS.join(", ")
                );
            }
            if cmd.iter().any(|a| {
                a.chars()
                    .any(|c| matches!(c, ';' | '|' | '&' | '`' | '$' | '\n'))
            }) {
                bail!("{source}: commands.{what} lleva metacaracteres de shell; los argumentos son literales");
            }
        }
        Ok(())
    }
}

fn substitute_all(cmd: &[String], name: &str) -> Vec<String> {
    cmd.iter()
        .map(|a| a.replace(NAME_PLACEHOLDER, name))
        .collect()
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Lo que queda escrito en `.antos/project.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub name: String,
    pub stack: String,
    pub language: String,
    #[serde(default)]
    pub framework: String,
    #[serde(default)]
    pub commands: Commands,
    #[serde(default)]
    pub toolchain: Toolchain,
    #[serde(default)]
    pub created_by: String,
}

/// De dónde salió el catálogo, para decirlo en `antos project stacks`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogSource {
    Env(PathBuf),
    Tree(PathBuf),
    Embedded,
}

#[derive(Debug, Clone)]
pub struct StackCatalog {
    stacks: Vec<Stack>,
    pub source: CatalogSource,
}

impl StackCatalog {
    /// Parsea un TOML y lo valida. `source` es solo para los mensajes.
    pub fn parse_one(text: &str, source: &str) -> Result<Stack> {
        let stack: Stack =
            toml::from_str(text).with_context(|| format!("{source}: TOML ilegible"))?;
        stack.validate(source)?;
        Ok(stack)
    }

    /// Construye el catálogo comprobando que ids y alias no colisionan.
    pub fn from_stacks(mut stacks: Vec<Stack>, source: CatalogSource) -> Result<Self> {
        stacks.sort_by(|a, b| a.id.cmp(&b.id));
        let mut seen_ids = std::collections::BTreeSet::new();
        let mut seen_aliases: BTreeMap<String, String> = BTreeMap::new();
        for s in &stacks {
            if !seen_ids.insert(s.id.clone()) {
                bail!("stack «{}» definido dos veces", s.id);
            }
            for a in &s.aliases {
                let key = a.to_ascii_lowercase();
                if let Some(owner) = seen_aliases.insert(key.clone(), s.id.clone()) {
                    if owner != s.id {
                        bail!("alias «{a}» repetido en los stacks «{owner}» y «{}»", s.id);
                    }
                }
            }
        }
        Ok(Self { stacks, source })
    }

    /// Carga un directorio de `.toml` (resolviendo `from` contra él).
    pub fn load_dir(dir: &Path) -> Result<Self> {
        let mut stacks = Vec::new();
        let read = std::fs::read_dir(dir)
            .with_context(|| format!("leyendo el catálogo de stacks en {}", dir.display()))?;
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("leyendo {}", path.display()))?;
            let mut stack = Self::parse_one(&text, &path.display().to_string())?;
            for f in &mut stack.files {
                if let Some(from) = &f.from {
                    let src = dir.join(from);
                    if !src.starts_with(dir) {
                        bail!("{}: `from` «{from}» sale del catálogo", path.display());
                    }
                    f.content = Some(
                        std::fs::read_to_string(&src)
                            .with_context(|| format!("leyendo la plantilla {}", src.display()))?,
                    );
                }
            }
            stacks.push(stack);
        }
        Self::from_stacks(stacks, CatalogSource::Tree(dir.to_path_buf()))
    }

    /// La copia embebida en el binario.
    pub fn embedded() -> Result<Self> {
        let stacks = EMBEDDED
            .iter()
            .map(|(id, text)| Self::parse_one(text, &format!("embebido:{id}")))
            .collect::<Result<Vec<_>>>()?;
        Self::from_stacks(stacks, CatalogSource::Embedded)
    }

    /// `ANTOS_STACKS` → árbol de antOS → embebido. Un directorio que existe
    /// pero no carga es un error (no se cae en silencio al embebido: sería
    /// esconder un TOML roto).
    pub fn load(antos_root: Option<&Path>) -> Result<Self> {
        if let Some(dir) = std::env::var_os("ANTOS_STACKS") {
            let dir = PathBuf::from(dir);
            let mut cat = Self::load_dir(&dir)?;
            cat.source = CatalogSource::Env(dir);
            return Ok(cat);
        }
        if let Some(root) = antos_root {
            let dir = root.join(STACKS_RELATIVE_PATH);
            if dir.is_dir() {
                return Self::load_dir(&dir);
            }
        }
        Self::embedded()
    }

    pub fn stacks(&self) -> &[Stack] {
        &self.stacks
    }

    pub fn get(&self, id: &str) -> Option<&Stack> {
        self.stacks.iter().find(|s| s.id == id)
    }

    /// Lenguajes distintos, ordenados (para el `enum` del manifiesto y los
    /// mensajes).
    pub fn languages(&self) -> Vec<String> {
        let mut v: Vec<String> = self.stacks.iter().map(|s| s.language.clone()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// El stack de `language` + `framework` (vacío = base del lenguaje).
    pub fn find(&self, language: &str, framework: &str) -> Option<&Stack> {
        let language = language.to_ascii_lowercase();
        let framework = framework.to_ascii_lowercase();
        self.stacks
            .iter()
            .find(|s| s.language == language && s.framework == framework)
    }

    /// El stack cuyo alias aparece en el texto (por palabras normalizadas).
    /// Con varios candidatos gana el más específico: un stack con framework
    /// antes que uno base («proyecto typescript con nest» → nestjs).
    pub fn find_by_alias_in(&self, words: &[String]) -> Option<&Stack> {
        let mut best: Option<&Stack> = None;
        for s in &self.stacks {
            let hit = s
                .aliases
                .iter()
                .any(|a| words.contains(&a.to_ascii_lowercase()));
            if !hit {
                continue;
            }
            best = match best {
                None => Some(s),
                Some(b) if b.is_base() && !s.is_base() => Some(s),
                Some(b) => Some(b),
            };
        }
        best
    }

    /// Los frameworks disponibles para un lenguaje, para los mensajes de error.
    pub fn frameworks_for(&self, language: &str) -> Vec<String> {
        self.stacks
            .iter()
            .filter(|s| s.language == language && !s.is_base())
            .map(|s| s.framework.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn repo_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks")
    }

    #[test]
    fn embedded_and_tree_catalogs_are_the_same_and_valid() {
        let embedded = StackCatalog::embedded().unwrap();
        let tree = StackCatalog::load_dir(&repo_dir()).unwrap();
        assert_eq!(
            embedded.stacks(),
            tree.stacks(),
            "system/stacks/ y el binario divergen"
        );
        assert!(embedded.stacks().len() >= 9);
        assert_eq!(
            embedded.languages(),
            vec!["go", "javascript", "python", "rust", "typescript"]
        );
        for s in embedded.stacks() {
            assert!(!s.commands.test.is_empty(), "{}: sin comando de test", s.id);
            if !s.is_base() {
                // Un fichero de test aparte, o tests inline (`#[cfg(test)]`
                // en Rust).
                assert!(
                    s.files.iter().any(|f| f.path.contains("test")
                        || f.path.contains("spec")
                        || f.content
                            .as_deref()
                            .is_some_and(|c| c.contains("#[cfg(test)]"))),
                    "{}: un stack con framework lleva un test",
                    s.id
                );
            }
        }
    }

    #[test]
    fn base_stacks_render_exactly_what_scaffold_wrote_before() {
        let cat = StackCatalog::embedded().unwrap();
        let rust = cat.find("rust", "").unwrap().render("demo");
        assert_eq!(
            rust,
            vec![
                (
                    "Cargo.toml".to_string(),
                    "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n".to_string()
                ),
                (
                    "src/main.rs".to_string(),
                    "fn main() {\n    println!(\"demo en marcha\");\n}\n".to_string()
                ),
            ]
        );
        let ts = cat.find("typescript", "").unwrap().render("web");
        assert_eq!(ts[0].0, "package.json");
        assert!(ts[0].1.contains("\"name\": \"web\""));
        assert_eq!(
            ts[2],
            (
                "src/index.ts".to_string(),
                "console.log(\"web en marcha\");\n".to_string()
            )
        );
        let py = cat.find("python", "").unwrap().render("tool");
        assert_eq!(py[1].0, "main.py");
        assert!(py[1].1.contains("print(\"tool en marcha\")"));
    }

    #[test]
    fn aliases_resolve_and_prefer_frameworks_over_bases() {
        let cat = StackCatalog::embedded().unwrap();
        let words = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            cat.find_by_alias_in(&words("un proyecto en nestjs llamado x"))
                .unwrap()
                .id,
            "nestjs"
        );
        assert_eq!(
            cat.find_by_alias_in(&words("api con fastapi")).unwrap().id,
            "fastapi"
        );
        assert_eq!(
            cat.find_by_alias_in(&words("proyecto typescript con nest"))
                .unwrap()
                .id,
            "nestjs"
        );
        assert_eq!(cat.find_by_alias_in(&words("algo en go")).unwrap().id, "go");
        assert!(cat.find_by_alias_in(&words("nada conocido aquí")).is_none());
        assert_eq!(cat.frameworks_for("typescript"), vec!["nestjs", "nextjs"]);
    }

    #[test]
    fn project_manifest_carries_stack_commands_and_name() {
        let cat = StackCatalog::embedded().unwrap();
        let text = cat.get("go").unwrap().project_manifest("svc").unwrap();
        // Los comentarios de cabecera los ignora el parser.
        let m: ProjectManifest = toml::from_str(&text).unwrap();
        assert_eq!(m.stack, "go");
        assert_eq!(m.commands.test, vec!["go", "test", "./..."]);
        assert_eq!(m.commands.build, vec!["go", "build", "-o", "bin/svc", "."]);
        assert_eq!(m.commands.port, Some(8080));
    }

    #[test]
    fn invalid_stacks_are_rejected_with_a_reason() {
        let bad = |extra: &str| {
            format!(
                "id = \"x\"\nlanguage = \"rust\"\n{extra}\n[[file]]\npath = \"a\"\ncontent = \"b\"\n"
            )
        };
        let shell = bad("[commands]\ninstall = [\"sh\", \"-c\", \"echo\"]");
        let err = StackCatalog::parse_one(&shell, "t")
            .unwrap_err()
            .to_string();
        assert!(err.contains("lista blanca"), "{err}");

        let meta = bad("[commands]\ninstall = [\"npm\", \"install\", \"&&\", \"rm\"]");
        assert!(StackCatalog::parse_one(&meta, "t")
            .unwrap_err()
            .to_string()
            .contains("metacaracteres"));

        let escape =
            "id = \"x\"\nlanguage = \"rust\"\n[[file]]\npath = \"../fuera\"\ncontent = \"b\"\n";
        assert!(StackCatalog::parse_one(escape, "t")
            .unwrap_err()
            .to_string()
            .contains(".."));

        let empty = "id = \"x\"\nlanguage = \"rust\"\n";
        assert!(StackCatalog::parse_one(empty, "t")
            .unwrap_err()
            .to_string()
            .contains("[[file]]"));

        let a = StackCatalog::parse_one("id = \"a\"\nlanguage = \"rust\"\naliases = [\"same\"]\n[[file]]\npath = \"f\"\ncontent = \"c\"\n", "a").unwrap();
        let b = StackCatalog::parse_one("id = \"b\"\nlanguage = \"rust\"\naliases = [\"same\"]\n[[file]]\npath = \"f\"\ncontent = \"c\"\n", "b").unwrap();
        let err = StackCatalog::from_stacks(vec![a, b], CatalogSource::Embedded).unwrap_err();
        assert!(err.to_string().contains("alias «same» repetido"));
    }

    /// Un TOML nuevo en un directorio queda disponible sin recompilar.
    #[test]
    fn a_new_stack_in_a_directory_is_picked_up() {
        let dir = std::env::temp_dir().join(format!("antos_stacks_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tpl")).unwrap();
        std::fs::write(dir.join("tpl/hello.txt"), "hola {{name}}\n").unwrap();
        std::fs::write(
            dir.join("hello.toml"),
            "id = \"hello\"\nlanguage = \"text\"\nframework = \"greeting\"\naliases = [\"hello\"]\n\
             [commands]\ntest = [\"make\", \"test\"]\n\
             [[file]]\npath = \"README.txt\"\nfrom = \"tpl/hello.txt\"\n",
        )
        .unwrap();
        let cat = StackCatalog::load_dir(&dir).unwrap();
        let s = cat.find("text", "greeting").unwrap();
        assert_eq!(
            s.render("mundo"),
            vec![("README.txt".to_string(), "hola mundo\n".to_string())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
