//! Secrets Vault and Zero Environmental Authority Shielding (T5.2).
//!
//! Protects sensitive development credentials (.env, SSH keys, cloud tokens)
//! requiring explicit, temporary grants before any sub-agent or sandboxed
//! process can access them.

use crate::grants::Grants;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Summary of a stored secret without exposing its raw plaintext value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretSummary {
    pub key: String,
    pub length: usize,
    pub updated_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Vault {
    pub secrets: BTreeMap<String, String>,
    pub updated_at: i64,
}

impl Vault {
    pub fn load(state_dir: &Path) -> Result<Self> {
        let vault_file = state_dir.join("vault.json");
        if !vault_file.exists() {
            return Ok(Vault::default());
        }
        let content = fs::read_to_string(&vault_file)
            .with_context(|| format!("reading vault from {}", vault_file.display()))?;
        let vault: Vault = serde_json::from_str(&content)?;
        Ok(vault)
    }

    pub fn save(&self, state_dir: &Path) -> Result<()> {
        let vault_file = state_dir.join("vault.json");
        if let Some(parent) = vault_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(&vault_file, json)?;

        // Restrict file permissions to user-only (0600 on Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&vault_file)?.permissions();
            perms.set_mode(0o600);
            let _ = fs::set_permissions(&vault_file, perms);
        }

        Ok(())
    }
}

/// Stores or updates a secret key in the vault.
pub fn set_secret(state_dir: &Path, key: &str, value: &str) -> Result<()> {
    let mut vault = Vault::load(state_dir)?;
    vault.secrets.insert(key.to_string(), value.to_string());
    vault.updated_at = chrono::Local::now().timestamp();
    vault.save(state_dir)?;
    Ok(())
}

/// Retrieves a secret value, requiring an active grant for `secret.read` or `secret.<key>`.
pub fn get_secret(state_dir: &Path, key: &str, grants: &Grants) -> Result<Option<String>> {
    let specific_grant = format!("secret.{key}");
    if !grants.is_granted("secret.read") && !grants.is_granted(&specific_grant) {
        bail!(
            "acceso denegado al secreto «{key}». Se requiere una concesión activa: antos grant secret.{key} --minutos 10"
        );
    }

    let vault = Vault::load(state_dir)?;
    Ok(vault.secrets.get(key).cloned())
}

/// Lists all secret keys present in the vault.
pub fn list_secrets(state_dir: &Path) -> Result<Vec<SecretSummary>> {
    let vault = Vault::load(state_dir)?;
    let mut list = Vec::new();
    for (k, v) in &vault.secrets {
        list.push(SecretSummary {
            key: k.clone(),
            length: v.len(),
            updated_at: vault.updated_at,
        });
    }
    list.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(list)
}

/// Checks if a given file name corresponds to sensitive credentials.
pub fn is_sensitive_filename(name: &str) -> bool {
    let n = name.to_lowercase();
    n == ".env"
        || n.starts_with(".env.")
        || n.ends_with(".pem")
        || n.ends_with(".key")
        || n.starts_with("id_rsa")
        || n.starts_with("id_ed25519")
        || n.starts_with("id_ecdsa")
        || n == "credentials"
}

/// Checks if a given path is considered a shielded secret path.
pub fn is_sensitive_path(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if is_sensitive_filename(name) {
            return true;
        }
    }
    let s = path.to_string_lossy();
    s.contains("/.ssh/") || s.contains("/.aws/") || s.contains("/.config/gcloud/")
}

/// Verifies whether the process or agent has active grants to read a sensitive path.
pub fn check_secret_access(path: &Path, grants: &Grants) -> Result<()> {
    if !is_sensitive_path(path) {
        return Ok(());
    }

    let p_str = path.to_string_lossy();
    if p_str.contains(".env") {
        if !grants.is_granted("secret.env") && !grants.is_granted("secret.read") {
            bail!("acceso bloqueado por Zero Environmental Authority: lectura de .env requiere concesión activa («antos grant secret.env»)");
        }
    } else if p_str.contains(".ssh") {
        if !grants.is_granted("secret.ssh") && !grants.is_granted("secret.read") {
            bail!("acceso bloqueado por Zero Environmental Authority: lectura de claves SSH requiere concesión activa («antos grant secret.ssh»)");
        }
    } else if p_str.contains(".aws") {
        if !grants.is_granted("secret.aws") && !grants.is_granted("secret.read") {
            bail!("acceso bloqueado por Zero Environmental Authority: lectura de credenciales AWS requiere concesión activa («antos grant secret.aws»)");
        }
    } else if !grants.is_granted("secret.read") {
        bail!("acceso bloqueado por Zero Environmental Authority a archivo sensible {}", path.display());
    }

    Ok(())
}

/// Injects granted vault secrets in-memory into a command without writing to disk.
pub fn inject_granted_secrets(
    cmd: &mut std::process::Command,
    state_dir: &Path,
    grants: &Grants,
) -> Result<usize> {
    let vault = Vault::load(state_dir)?;
    let mut count = 0;

    for (k, v) in &vault.secrets {
        let specific_grant = format!("secret.{k}");
        if grants.is_granted("secret.read")
            || grants.is_granted(&specific_grant)
            || grants.is_granted("secret.env")
        {
            cmd.env(k, v);
            count += 1;
        }
    }

    Ok(count)
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_path_detection() {
        assert!(is_sensitive_filename(".env"));
        assert!(is_sensitive_filename(".env.local"));
        assert!(is_sensitive_filename(".env.production"));
        assert!(is_sensitive_filename("id_rsa"));
        assert!(is_sensitive_filename("id_ed25519"));
        assert!(is_sensitive_filename("server.key"));
        assert!(!is_sensitive_filename("main.rs"));
        assert!(!is_sensitive_filename("Cargo.toml"));

        assert!(is_sensitive_path(Path::new("/workspace/.env")));
        assert!(is_sensitive_path(Path::new("/home/user/.ssh/id_rsa")));
        assert!(is_sensitive_path(Path::new("/home/user/.aws/credentials")));
        assert!(!is_sensitive_path(Path::new("/workspace/src/lib.rs")));
    }

    #[test]
    fn test_vault_and_grant_access_control() {
        let temp = std::env::temp_dir().join(format!(
            "vault_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::create_dir_all(&temp);

        // Store secret
        set_secret(&temp, "STRIPE_KEY", "sk_test_12345").expect("must set secret");

        let mut grants = Grants::default();
        // Access without grant fails
        let err = get_secret(&temp, "STRIPE_KEY", &grants);
        assert!(err.is_err(), "must deny without grant");

        // Sensitive path access check
        let path_err = check_secret_access(Path::new("/workspace/.env"), &grants);
        assert!(path_err.is_err());

        // Grant access
        grants.grant_with_reason("secret.STRIPE_KEY", 10, Some("unit test".into()));
        grants.grant_with_reason("secret.env", 10, Some("unit test env".into()));

        let val = get_secret(&temp, "STRIPE_KEY", &grants).expect("must allow with grant");
        assert_eq!(val, Some("sk_test_12345".to_string()));

        let path_ok = check_secret_access(Path::new("/workspace/.env"), &grants);
        assert!(path_ok.is_ok(), "must allow path when secret.env is granted");

        let _ = fs::remove_dir_all(&temp);
    }
}
