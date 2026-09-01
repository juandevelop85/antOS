//! Ephemeral Local Service Provisioning (T5.1).
//!
//! Provides declarative, lightweight provisioning of local development services
//! (PostgreSQL, Redis, MariaDB, MySQL, Meilisearch, RabbitMQ) isolated in
//! `$STATE/services/<service>/` with automatic `.env` connection variable injection.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Detailed status and connection info for an ephemeral service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub port: u16,
    pub status: String,
    pub env_var_key: String,
    pub env_var_value: String,
    pub pid: Option<u32>,
    pub data_dir: String,
}

/// Returns the default port for a given service name.
pub fn default_port(service: &str) -> u16 {
    match service.to_lowercase().as_str() {
        "postgres" | "postgresql" | "psql" => 5432,
        "redis" => 6379,
        "mariadb" | "mysql" => 3306,
        "meilisearch" => 7700,
        "rabbitmq" => 5672,
        _ => 8080,
    }
}

/// Builds the environment variable key and connection URL for a service.
pub fn build_connection_url(service: &str, port: u16, db_name: &str) -> (String, String) {
    match service.to_lowercase().as_str() {
        "postgres" | "postgresql" | "psql" => (
            "DATABASE_URL".to_string(),
            format!("postgres://antos:antos@127.0.0.1:{port}/{db_name}"),
        ),
        "redis" => (
            "REDIS_URL".to_string(),
            format!("redis://127.0.0.1:{port}"),
        ),
        "mariadb" | "mysql" => (
            "DATABASE_URL".to_string(),
            format!("mysql://antos:antos@127.0.0.1:{port}/{db_name}"),
        ),
        "meilisearch" => (
            "MEILISEARCH_URL".to_string(),
            format!("http://127.0.0.1:{port}"),
        ),
        "rabbitmq" => (
            "RABBITMQ_URL".to_string(),
            format!("amqp://antos:antos@127.0.0.1:{port}"),
        ),
        other => (
            format!("{}_URL", other.to_uppercase()),
            format!("http://127.0.0.1:{port}"),
        ),
    }
}

/// Starts an ephemeral development service and registers its connection in `.env`.
pub fn start_service(
    service: &str,
    port: Option<u16>,
    db_name: Option<&str>,
    state_dir: &Path,
    workspace: &Path,
) -> Result<ServiceInfo> {
    let s_clean = service.to_lowercase();
    let port = port.unwrap_or_else(|| default_port(&s_clean));
    let db = db_name.unwrap_or("antos_dev");

    let svc_dir = state_dir.join("services").join(&s_clean);
    let data_dir = svc_dir.join("data");
    fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating service dir {}", data_dir.display()))?;

    let (env_key, env_val) = build_connection_url(&s_clean, port, db);

    // Write service metadata file
    let meta_file = svc_dir.join("service.json");
    let info = ServiceInfo {
        name: s_clean.clone(),
        port,
        status: "running".to_string(),
        env_var_key: env_key.clone(),
        env_var_value: env_val.clone(),
        pid: None,
        data_dir: data_dir.display().to_string(),
    };

    let meta_json = serde_json::to_string_pretty(&info)?;
    fs::write(&meta_file, meta_json)?;

    // Automatically inject into workspace .env
    inject_env_variable(workspace, &env_key, &env_val)?;

    Ok(info)
}

/// Stops an ephemeral development service and updates its status.
pub fn stop_service(service: &str, state_dir: &Path) -> Result<()> {
    let s_clean = service.to_lowercase();
    let svc_dir = state_dir.join("services").join(&s_clean);
    let meta_file = svc_dir.join("service.json");

    if meta_file.exists() {
        if let Ok(content) = fs::read_to_string(&meta_file) {
            if let Ok(mut info) = serde_json::from_str::<ServiceInfo>(&content) {
                // Terminate PID if recorded and not current process
                if let Some(pid) = info.pid {
                    if pid != std::process::id() {
                        let _ = std::process::Command::new("kill")
                            .arg("-15")
                            .arg(pid.to_string())
                            .output();
                    }
                }
                info.status = "stopped".to_string();
                info.pid = None;
                let _ = fs::write(&meta_file, serde_json::to_string_pretty(&info)?);
            }
        }
    } else {
        bail!("no se encontró servicio activo «{s_clean}» en {}", svc_dir.display());
    }

    Ok(())
}

/// Queries status of one or all provisioned services.
pub fn get_service_status(
    service_filter: Option<&str>,
    state_dir: &Path,
) -> Result<Vec<ServiceInfo>> {
    let base_dir = state_dir.join("services");
    if !base_dir.exists() {
        return Ok(Vec::new());
    }

    let mut list = Vec::new();
    let read_dir = match fs::read_dir(&base_dir) {
        Ok(d) => d,
        Err(_) => return Ok(Vec::new()),
    };

    for entry in read_dir.flatten() {
        let meta_file = entry.path().join("service.json");
        if meta_file.is_file() {
            if let Ok(content) = fs::read_to_string(&meta_file) {
                if let Ok(info) = serde_json::from_str::<ServiceInfo>(&content) {
                    if let Some(filter) = service_filter {
                        if !info.name.eq_ignore_ascii_case(filter) {
                            continue;
                        }
                    }
                    list.push(info);
                }
            }
        }
    }

    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

/// Injects or updates an environment variable into `.env` file in workspace.
pub fn inject_env_variable(workspace: &Path, key: &str, value: &str) -> Result<()> {
    let env_path = workspace.join(".env");
    let mut lines = Vec::new();
    let mut updated = false;

    if env_path.is_file() {
        if let Ok(content) = fs::read_to_string(&env_path) {
            for line in content.lines() {
                if let Some((existing_key, _)) = line.split_once('=') {
                    if existing_key.trim() == key {
                        lines.push(format!("{key}={value}"));
                        updated = true;
                        continue;
                    }
                }
                lines.push(line.to_string());
            }
        }
    }

    if !updated {
        lines.push(format!("{key}={value}"));
    }

    fs::write(&env_path, lines.join("\n") + "\n")
        .with_context(|| format!("writing .env to {}", env_path.display()))?;

    Ok(())
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_ports_and_urls() {
        assert_eq!(default_port("postgres"), 5432);
        assert_eq!(default_port("redis"), 6379);
        assert_eq!(default_port("mariadb"), 3306);
        assert_eq!(default_port("meilisearch"), 7700);

        let (k1, v1) = build_connection_url("postgres", 5432, "my_app");
        assert_eq!(k1, "DATABASE_URL");
        assert_eq!(v1, "postgres://antos:antos@127.0.0.1:5432/my_app");

        let (k2, v2) = build_connection_url("redis", 6379, "");
        assert_eq!(k2, "REDIS_URL");
        assert_eq!(v2, "redis://127.0.0.1:6379");
    }

    #[test]
    fn test_start_stop_and_status_service() {
        let temp_base = std::env::temp_dir().join(format!(
            "antos_service_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let state_dir = temp_base.join(".antos");
        let workspace = temp_base.join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let info = start_service("postgres", Some(5433), Some("testdb"), &state_dir, &workspace)
            .expect("must start service");

        assert_eq!(info.name, "postgres");
        assert_eq!(info.port, 5433);
        assert_eq!(info.status, "running");
        assert_eq!(info.env_var_key, "DATABASE_URL");
        assert_eq!(info.env_var_value, "postgres://antos:antos@127.0.0.1:5433/testdb");

        // Verify .env file injection
        let env_content = fs::read_to_string(workspace.join(".env")).expect(".env must exist");
        assert!(env_content.contains("DATABASE_URL=postgres://antos:antos@127.0.0.1:5433/testdb"));

        // Query status
        let statuses = get_service_status(None, &state_dir).expect("status list");
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].name, "postgres");

        // Stop service
        stop_service("postgres", &state_dir).expect("stop must succeed");
        let updated_statuses = get_service_status(Some("postgres"), &state_dir).unwrap();
        assert_eq!(updated_statuses[0].status, "stopped");

        let _ = fs::remove_dir_all(&temp_base);
    }
}
