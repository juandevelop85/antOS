//! Tabla de servicios conocidos: qué puerto usan, qué variable inyectan, cómo
//! se arrancan y cómo se comprueba que están vivos.
//!
//! Todo lo que distingue a un servicio de otro vive aquí; `backend.rs` no
//! sabe qué es PostgreSQL, solo sabe lanzar un `LaunchPlan` y sondear un
//! `Probe`.

use std::path::{Path, PathBuf};

/// Servicios que antOS sabe aprovisionar. Un nombre fuera de esta lista es
/// un error claro, no un `8080` genérico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    Ollama,
    Postgres,
    Redis,
    MariaDb,
    Meilisearch,
    RabbitMq,
}

/// Cómo se decide que el servicio está escuchando de verdad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// Basta con que el puerto acepte una conexión TCP.
    Tcp,
    /// Un `GET` a esta ruta debe devolver 2xx (el puerto puede aceptar TCP
    /// mientras el proceso aún carga; Ollama tarda unos segundos).
    Http { path: &'static str },
}

/// Un comando a ejecutar: programa (nombre de binario, sin ruta) y
/// argumentos como vector. Nunca se interpola en un intérprete (T31.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    pub program: &'static str,
    pub args: Vec<String>,
}

/// Todo lo necesario para arrancar un servicio en un directorio concreto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    /// Pasos previos que terminan solos (p. ej. `initdb`). Se saltan si
    /// `setup_marker` ya existe.
    pub setup: Vec<Cmd>,
    /// Fichero cuya existencia indica que el `setup` ya se hizo.
    pub setup_marker: Option<PathBuf>,
    /// El proceso de larga vida.
    pub main: Cmd,
    /// Variables de entorno específicas del servicio (además del entorno
    /// mínimo que pone `backend.rs`).
    pub env: Vec<(String, String)>,
    /// Pasos a ejecutar una vez el servicio responde (p. ej. `createdb`).
    /// Un fallo aquí se registra en el log pero no tumba el servicio.
    pub post_start: Vec<Cmd>,
}

/// Directorios que un `LaunchPlan` puede usar; los crea `mod.rs` antes de
/// pedir el plan.
#[derive(Debug, Clone)]
pub struct LaunchDirs {
    /// `$STATE/services/<svc>/data` — persiste entre arranques.
    pub data: PathBuf,
    /// `$STATE/services/<svc>/run` — sockets y temporales.
    pub run: PathBuf,
    /// `$STATE/services/<svc>/home` — `HOME` del proceso, para que lo que
    /// escriba «en casa» (claves de Ollama, historiales) quede aquí.
    pub home: PathBuf,
}

impl ServiceKind {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "ollama" => Self::Ollama,
            "postgres" | "postgresql" | "psql" => Self::Postgres,
            "redis" => Self::Redis,
            "mariadb" | "mysql" => Self::MariaDb,
            "meilisearch" => Self::Meilisearch,
            "rabbitmq" => Self::RabbitMq,
            _ => return None,
        })
    }

    pub const ALL: [ServiceKind; 6] = [
        Self::Ollama,
        Self::Postgres,
        Self::Redis,
        Self::MariaDb,
        Self::Meilisearch,
        Self::RabbitMq,
    ];

    /// Nombre bajo el que se registra en `$STATE/services/`.
    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::Postgres => "postgres",
            Self::Redis => "redis",
            Self::MariaDb => "mariadb",
            Self::Meilisearch => "meilisearch",
            Self::RabbitMq => "rabbitmq",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Ollama => 11434,
            Self::Postgres => 5432,
            Self::Redis => 6379,
            Self::MariaDb => 3306,
            Self::Meilisearch => 7700,
            Self::RabbitMq => 5672,
        }
    }

    /// Variable de entorno y valor que se inyectan en el `.env` del workspace.
    pub fn connection(self, port: u16, db_name: &str) -> (String, String) {
        match self {
            Self::Ollama => (
                "OLLAMA_HOST".to_string(),
                format!("http://127.0.0.1:{port}"),
            ),
            Self::Postgres => (
                "DATABASE_URL".to_string(),
                format!("postgres://antos:antos@127.0.0.1:{port}/{db_name}"),
            ),
            Self::Redis => ("REDIS_URL".to_string(), format!("redis://127.0.0.1:{port}")),
            Self::MariaDb => (
                "DATABASE_URL".to_string(),
                format!("mysql://antos:antos@127.0.0.1:{port}/{db_name}"),
            ),
            Self::Meilisearch => (
                "MEILISEARCH_URL".to_string(),
                format!("http://127.0.0.1:{port}"),
            ),
            Self::RabbitMq => (
                "RABBITMQ_URL".to_string(),
                format!("amqp://antos:antos@127.0.0.1:{port}"),
            ),
        }
    }

    pub fn probe(self) -> Probe {
        match self {
            Self::Ollama => Probe::Http { path: "/api/tags" },
            Self::Meilisearch => Probe::Http { path: "/health" },
            _ => Probe::Tcp,
        }
    }

    /// Cuánto se espera a que la sonda pase tras arrancar.
    pub fn startup_timeout_secs(self) -> u64 {
        match self {
            // Carga el runtime y, si hay GPU, la inicializa.
            Self::Ollama => 40,
            _ => 20,
        }
    }

    /// Binario principal, en orden de preferencia (MariaDB cambió de nombre).
    pub fn binaries(self) -> &'static [&'static str] {
        match self {
            Self::Ollama => &["ollama"],
            Self::Postgres => &["postgres"],
            Self::Redis => &["redis-server"],
            Self::MariaDb => &["mariadbd", "mysqld"],
            Self::Meilisearch => &["meilisearch"],
            Self::RabbitMq => &["rabbitmq-server"],
        }
    }

    /// Directorios donde las instalaciones habituales dejan el binario sin
    /// ponerlo en `PATH`. Un `*` en un componente se expande contra el
    /// directorio real (Debian versiona PostgreSQL, Homebrew también).
    pub fn extra_binary_dirs(self) -> &'static [&'static str] {
        match self {
            Self::Ollama => &[
                "/usr/local/bin",
                "/Applications/Ollama.app/Contents/Resources",
            ],
            Self::Postgres => &[
                "/usr/lib/postgresql/*/bin",
                "/opt/homebrew/opt/postgresql@*/bin",
                "/opt/homebrew/opt/postgresql/bin",
                "/usr/local/opt/postgresql@*/bin",
                "/Applications/Postgres.app/Contents/Versions/*/bin",
            ],
            Self::Redis => &["/opt/homebrew/opt/redis/bin", "/usr/local/opt/redis/bin"],
            Self::MariaDb => &[
                "/opt/homebrew/opt/mariadb/bin",
                "/usr/local/opt/mariadb/bin",
            ],
            Self::Meilisearch => &["/usr/local/bin"],
            Self::RabbitMq => &["/opt/homebrew/opt/rabbitmq/sbin", "/usr/lib/rabbitmq/bin"],
        }
    }

    /// Atributo de `nixpkgs` para el backend `Nix`.
    pub fn nix_package(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::Postgres => "postgresql",
            Self::Redis => "redis",
            Self::MariaDb => "mariadb",
            Self::Meilisearch => "meilisearch",
            Self::RabbitMq => "rabbitmq-server",
        }
    }

    /// Plan de arranque, o `None` si antOS todavía no sabe arrancar este
    /// servicio (solo puede adoptarlo si ya está escuchando).
    ///
    /// Los programas van por nombre; `backend.rs` los resuelve al binario
    /// del sistema o los envuelve en `nix shell … -c`.
    pub fn launch_plan(self, port: u16, db_name: &str, dirs: &LaunchDirs) -> Option<LaunchPlan> {
        let data = dirs.data.to_string_lossy().to_string();
        Some(match self {
            Self::Ollama => LaunchPlan {
                setup: Vec::new(),
                setup_marker: None,
                main: Cmd {
                    program: "ollama",
                    args: vec!["serve".into()],
                },
                env: vec![
                    ("OLLAMA_HOST".into(), format!("127.0.0.1:{port}")),
                    ("OLLAMA_MODELS".into(), data),
                ],
                post_start: Vec::new(),
            },
            Self::Postgres => LaunchPlan {
                // `--auth=trust` en loopback: la URL lleva `antos:antos` por
                // convención de T5.1, pero la contraseña no se comprueba.
                setup: vec![Cmd {
                    program: "initdb",
                    args: vec![
                        "-D".into(),
                        data.clone(),
                        "-U".into(),
                        "antos".into(),
                        "--auth=trust".into(),
                        "-E".into(),
                        "UTF8".into(),
                    ],
                }],
                setup_marker: Some(dirs.data.join("PG_VERSION")),
                // Solo TCP en loopback: sin socket Unix, que además tiene
                // un límite de ~100 caracteres de ruta que un `$STATE`
                // profundo supera.
                main: Cmd {
                    program: "postgres",
                    args: vec![
                        "-D".into(),
                        data,
                        "-p".into(),
                        port.to_string(),
                        "-c".into(),
                        "listen_addresses=127.0.0.1".into(),
                        "-c".into(),
                        "unix_socket_directories=".into(),
                    ],
                },
                env: Vec::new(),
                post_start: vec![Cmd {
                    program: "createdb",
                    args: vec![
                        "-h".into(),
                        "127.0.0.1".into(),
                        "-p".into(),
                        port.to_string(),
                        "-U".into(),
                        "antos".into(),
                        db_name.to_string(),
                    ],
                }],
            },
            Self::Redis => LaunchPlan {
                setup: Vec::new(),
                setup_marker: None,
                main: Cmd {
                    program: "redis-server",
                    args: vec![
                        "--port".into(),
                        port.to_string(),
                        "--bind".into(),
                        "127.0.0.1".into(),
                        "--dir".into(),
                        data,
                        "--daemonize".into(),
                        "no".into(),
                    ],
                },
                env: Vec::new(),
                post_start: Vec::new(),
            },
            Self::Meilisearch => LaunchPlan {
                setup: Vec::new(),
                setup_marker: None,
                main: Cmd {
                    program: "meilisearch",
                    args: vec![
                        "--http-addr".into(),
                        format!("127.0.0.1:{port}"),
                        "--db-path".into(),
                        data,
                        "--no-analytics".into(),
                    ],
                },
                env: Vec::new(),
                post_start: Vec::new(),
            },
            // Sin plan verificado: MariaDB necesita `mariadb-install-db` con
            // opciones que cambian entre versiones y RabbitMQ un árbol de
            // Mnesia y cookie de Erlang. Se adoptan si ya escuchan; arrancarlos
            // es un ticket aparte, no una promesa rota aquí.
            Self::MariaDb | Self::RabbitMq => return None,
        })
    }

    /// Nombres de ejecutable que puede tener el proceso principal en `ps`:
    /// el binario del servicio (con `nix shell -c` el proceso final es el
    /// mismo binario, no `nix`). Sirve para no confundir un PID reutilizado
    /// con el nuestro.
    pub fn expected_process_names(self) -> &'static [&'static str] {
        self.binaries()
    }

    /// Unidad systemd con la que la imagen NixOS puede traer este servicio
    /// de serie (T34.3). Si está activa y no hay registro propio, `antos
    /// services` lo lista como externo gestionado por systemd.
    pub fn systemd_unit(self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some("ollama"),
            Self::Postgres => Some("postgresql"),
            Self::Redis => Some("redis"),
            Self::MariaDb => Some("mariadb"),
            Self::Meilisearch => Some("meilisearch"),
            Self::RabbitMq => Some("rabbitmq"),
        }
    }

    /// Los binarios auxiliares que el plan usa, para localizarlos junto al
    /// principal (Debian pone `initdb` y `createdb` en el mismo `bin/`).
    pub fn helper_binaries(self) -> &'static [&'static str] {
        match self {
            Self::Postgres => &["initdb", "createdb"],
            _ => &[],
        }
    }
}

/// Puerto por defecto para un nombre de servicio; `None` si no es conocido.
pub fn default_port(service: &str) -> Option<u16> {
    ServiceKind::parse(service).map(ServiceKind::default_port)
}

/// Variable de entorno y URL de conexión; `None` si el servicio no es conocido.
pub fn build_connection_url(service: &str, port: u16, db_name: &str) -> Option<(String, String)> {
    ServiceKind::parse(service).map(|k| k.connection(port, db_name))
}

/// Lista legible de los servicios conocidos, para mensajes de error.
pub fn known_services_list() -> String {
    ServiceKind::ALL
        .iter()
        .map(|k| k.canonical_name())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Ruta del registro de un servicio dentro del directorio de estado.
pub fn service_dir(state_dir: &Path, kind: ServiceKind) -> PathBuf {
    state_dir.join("services").join(kind.canonical_name())
}
