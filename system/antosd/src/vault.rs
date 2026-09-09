//! Secrets Vault and Zero Environmental Authority Shielding (T5.2).
//!
//! Protects sensitive development credentials (.env, SSH keys, cloud tokens)
//! requiring explicit, temporary grants before any sub-agent or sandboxed
//! process can access them.
//!
//! ## Cifrado en reposo (T31.6)
//!
//! `vault.json` on disk is ChaCha20-Poly1305 ciphertext, not plaintext JSON
//! — a `grep` of the state directory, a snapshot, or a backup that captures
//! `vault.json` alone finds nothing usable. The AEAD key lives in a
//! sibling file, `vault.key`, generated from OS entropy on first use and
//! written with the same `0600`, atomic guarantee as the vault itself (see
//! [`crate::crypto::write_secret_file`]).
//!
//! **What this does not protect against**, stated plainly rather than
//! implied: an attacker who can read the *whole* state directory gets
//! `vault.key` right alongside `vault.json` and can decrypt it — this is
//! encryption against partial exposure (a stray `vault.json` in a leaked
//! backup, a support bundle, a misconfigured sync), not against a full
//! compromise of the machine or the state directory. Closing that gap needs
//! a key that doesn't live on disk next to what it protects — a
//! user-supplied passphrase (Argon2id) or the platform keychain — and was
//! left out of this correction because either one changes how every secret
//! operation is invoked (a passphrase prompt, or a keychain unlock, on
//! every read), which is a product decision beyond an audit fix. The
//! in-memory hygiene below ([`crate::crypto::SecretValue`]) and the
//! migration path for pre-T31.6 plaintext vaults are unaffected by that
//! limitation.

use crate::crypto::{self, SecretValue};
use crate::grants::Grants;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// On-disk format version, so a future format change can tell an old vault
/// apart from a new one instead of guessing (T31.6).
const VAULT_FORMAT_VERSION: u8 = 1;

/// Summary of a stored secret without exposing its raw plaintext value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretSummary {
    pub key: String,
    pub length: usize,
    pub updated_at: i64,
}

/// The vault, held in memory. Values are [`SecretValue`]s (T31.6): each
/// scrubs its own buffer when dropped, and none of them print through
/// `{:?}` — accidentally logging or debug-printing a `Vault` cannot leak a
/// credential.
#[derive(Debug, Default)]
pub struct Vault {
    pub secrets: BTreeMap<String, SecretValue>,
    pub updated_at: i64,
}

/// The plaintext shape that actually gets encrypted before touching disk —
/// and, unencrypted, exactly what a pre-T31.6 `vault.json` looked like on
/// disk in full. `Vault::load` tries this as a migration path when the new
/// encrypted envelope fails to parse.
#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultData {
    secrets: BTreeMap<String, String>,
    #[serde(default)]
    updated_at: i64,
}

impl From<&Vault> for VaultData {
    fn from(v: &Vault) -> Self {
        VaultData {
            secrets: v.secrets.iter().map(|(k, val)| (k.clone(), val.expose().to_string())).collect(),
            updated_at: v.updated_at,
        }
    }
}

impl From<VaultData> for Vault {
    fn from(d: VaultData) -> Self {
        Vault {
            secrets: d.secrets.into_iter().map(|(k, v)| (k, SecretValue::new(v))).collect(),
            updated_at: d.updated_at,
        }
    }
}

/// On-disk envelope for the encrypted vault (T31.6): `payload` is
/// `nonce || ciphertext` from [`crypto::aead_encrypt`], hex-encoded.
#[derive(Debug, Serialize, Deserialize)]
struct EncryptedVaultFile {
    version: u8,
    payload: String,
}

impl Vault {
    fn vault_path(state_dir: &Path) -> PathBuf {
        state_dir.join("vault.json")
    }

    /// Where the vault's AEAD key lives — deliberately a separate file from
    /// `vault_path`, so the two can (and, for real protection, should) be
    /// backed up or transmitted separately (T31.6).
    fn key_path(state_dir: &Path) -> PathBuf {
        state_dir.join("vault.key")
    }

    /// Loads and decrypts the vault. Transparently migrates a pre-T31.6
    /// plaintext `vault.json` to the encrypted format the moment it's next
    /// touched, printing a one-line notice — see the module doc comment.
    pub fn load(state_dir: &Path) -> Result<Self> {
        let vault_file = Self::vault_path(state_dir);
        if !vault_file.exists() {
            return Ok(Vault::default());
        }

        let content = fs::read_to_string(&vault_file)
            .with_context(|| format!("reading vault from {}", vault_file.display()))?;

        if let Ok(envelope) = serde_json::from_str::<EncryptedVaultFile>(&content) {
            if envelope.version != VAULT_FORMAT_VERSION {
                bail!("vault.json has an unsupported format version ({})", envelope.version);
            }
            let key = Self::load_or_create_key(state_dir)?;
            let ciphertext = crypto::from_hex(&envelope.payload).context("vault.json payload is not valid hex")?;
            let plaintext = crypto::aead_decrypt(&key, &ciphertext)
                .context("failed to decrypt vault.json — wrong vault.key or corrupted/tampered vault")?;
            let data: VaultData = serde_json::from_slice(&plaintext).context("decrypted vault payload is not valid JSON")?;
            return Ok(data.into());
        }

        // Not the encrypted envelope shape — try the pre-T31.6 plaintext
        // shape and migrate on the spot if it matches.
        let legacy: VaultData = serde_json::from_str(&content)
            .context("vault.json is neither a recognized encrypted vault nor a legacy plaintext one")?;
        let vault: Vault = legacy.into();
        eprintln!(
            "antOS · aviso: se detectó una bóveda de secretos sin cifrar en {} y se migró automáticamente al formato cifrado (T31.6).",
            vault_file.display()
        );
        vault.save(state_dir)?;
        Ok(vault)
    }

    pub fn save(&self, state_dir: &Path) -> Result<()> {
        let vault_file = Self::vault_path(state_dir);
        let key = Self::load_or_create_key(state_dir)?;

        let data = VaultData::from(self);
        let plaintext = serde_json::to_vec(&data)?;
        let ciphertext = crypto::aead_encrypt(&key, &plaintext)?;

        let envelope = EncryptedVaultFile {
            version: VAULT_FORMAT_VERSION,
            payload: crypto::to_hex(&ciphertext),
        };
        let json = serde_json::to_vec_pretty(&envelope)?;
        crypto::write_secret_file(&vault_file, &json)
    }

    /// Loads the vault's AEAD key, generating one from OS entropy the first
    /// time the vault is saved (T31.6). Written the same way as the vault
    /// itself: atomically, `0600` from creation.
    fn load_or_create_key(state_dir: &Path) -> Result<[u8; 32]> {
        let key_file = Self::key_path(state_dir);
        if key_file.exists() {
            let hex = fs::read_to_string(&key_file)
                .with_context(|| format!("reading vault key from {}", key_file.display()))?;
            let bytes = crypto::from_hex(hex.trim()).context("vault.key is not valid hex")?;
            let key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("vault.key must be exactly 32 bytes"))?;
            return Ok(key);
        }

        let key_bytes = crypto::secure_random_bytes(32)?;
        let key: [u8; 32] = key_bytes
            .clone()
            .try_into()
            .map_err(|_| anyhow::anyhow!("secure_random_bytes(32) returned an unexpected length"))?;
        crypto::write_secret_file(&key_file, crypto::to_hex(&key_bytes).as_bytes())?;
        Ok(key)
    }
}

/// Stores or updates a secret key in the vault.
pub fn set_secret(state_dir: &Path, key: &str, value: &str) -> Result<()> {
    let mut vault = Vault::load(state_dir)?;
    vault.secrets.insert(key.to_string(), SecretValue::new(value.to_string()));
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
    Ok(vault.secrets.get(key).map(|v| v.expose().to_string()))
}

/// Lists all secret keys present in the vault.
pub fn list_secrets(state_dir: &Path) -> Result<Vec<SecretSummary>> {
    let vault = Vault::load(state_dir)?;
    let mut list = Vec::new();
    for (k, v) in &vault.secrets {
        list.push(SecretSummary {
            key: k.clone(),
            length: v.expose().len(),
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
            cmd.env(k, v.expose());
            count += 1;
        }
    }

    Ok(count)
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

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

    // ---------------------------------------------------------------- T31.6

    fn t31_6_temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("antos_vault_t31_6_{label}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_secret_value_never_appears_in_plaintext_on_disk() {
        let temp = t31_6_temp_dir("no_cleartext_on_disk");
        let secret_value = "sk_live_51ExtremelyDistinctiveTestValue987";

        set_secret(&temp, "STRIPE_KEY", secret_value).unwrap();

        // Every file antOS put in this directory, checked byte-for-byte —
        // this is the literal `grep` the acceptance criterion asks for.
        for entry in walk_files(&temp) {
            let bytes = fs::read(&entry).unwrap();
            assert!(
                !contains_bytes(&bytes, secret_value.as_bytes()),
                "found the secret in cleartext in {}",
                entry.display()
            );
        }

        // Sanity: the vault really does still round-trip the value.
        let mut grants = Grants::default();
        grants.grant_with_reason("secret.read", 10, None);
        assert_eq!(get_secret(&temp, "STRIPE_KEY", &grants).unwrap(), Some(secret_value.to_string()));

        let _ = fs::remove_dir_all(&temp);
    }

    fn walk_files(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    out.extend(walk_files(&path));
                } else {
                    out.push(path);
                }
            }
        }
        out
    }

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    #[cfg(unix)]
    #[test]
    fn test_vault_files_are_never_more_permissive_than_0600_even_with_a_lax_umask() {
        use std::os::unix::fs::PermissionsExt;

        let temp = t31_6_temp_dir("perms");

        // SAFETY: temporarily loosens the process umask to prove vault.json
        // and vault.key don't rely on an already-strict umask — they must
        // be created pre-restricted, not merely chmod'd afterward.
        unsafe {
            let previous = libc::umask(0o000);
            let result = std::panic::catch_unwind(|| {
                set_secret(&temp, "API_KEY", "value").unwrap();

                let vault_mode = fs::metadata(temp.join("vault.json")).unwrap().permissions().mode();
                assert_eq!(vault_mode & 0o777, 0o600, "vault.json must be 0600 even with a permissive umask");

                let key_mode = fs::metadata(temp.join("vault.key")).unwrap().permissions().mode();
                assert_eq!(key_mode & 0o777, 0o600, "vault.key must be 0600 even with a permissive umask");
            });
            libc::umask(previous);
            result.unwrap();
        }

        let _ = fs::remove_dir_all(&temp);
    }

    #[cfg(unix)]
    #[test]
    fn test_a_failed_write_leaves_the_previous_vault_intact() {
        use std::os::unix::fs::PermissionsExt;

        let temp = t31_6_temp_dir("atomic_write");
        set_secret(&temp, "API_KEY", "original-value").unwrap();

        let vault_file = temp.join("vault.json");
        let original_bytes = fs::read(&vault_file).unwrap();

        // Strip write access from the directory so the next save can't even
        // create its temp file — a write that dies before the atomic
        // rename must never have touched the destination in the first
        // place, which is exactly what this proves.
        let mut perms = fs::metadata(&temp).unwrap().permissions();
        perms.set_mode(0o500); // r-x, no write
        fs::set_permissions(&temp, perms).unwrap();

        let attempt = set_secret(&temp, "API_KEY", "attempted-overwrite-that-must-not-land");

        // Restore permissions before asserting/cleaning up, regardless of
        // the outcome above.
        let mut perms = fs::metadata(&temp).unwrap().permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&temp, perms).unwrap();

        assert!(attempt.is_err(), "a write that cannot create its temp file must fail loudly, not silently succeed");
        assert_eq!(
            fs::read(&vault_file).unwrap(),
            original_bytes,
            "the previous vault.json must be byte-for-byte untouched after a failed write"
        );

        let vault = Vault::load(&temp).expect("the previous vault must still load correctly");
        assert_eq!(vault.secrets.get("API_KEY").unwrap().expose(), "original-value");

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_legacy_plaintext_vault_migrates_without_losing_data() {
        let temp = t31_6_temp_dir("migration");
        let vault_file = temp.join("vault.json");

        // Write exactly what a pre-T31.6 vault.json looked like: plaintext
        // JSON, no envelope, no encryption.
        let legacy_json = serde_json::json!({
            "secrets": {
                "OLD_TOKEN": "legacy-plaintext-value-12345",
                "ANOTHER_ONE": "second-legacy-value"
            },
            "updated_at": 1_700_000_000i64
        });
        fs::write(&vault_file, serde_json::to_vec_pretty(&legacy_json).unwrap()).unwrap();

        let migrated = Vault::load(&temp).expect("a legacy plaintext vault must load and migrate");
        assert_eq!(migrated.secrets.get("OLD_TOKEN").unwrap().expose(), "legacy-plaintext-value-12345");
        assert_eq!(migrated.secrets.get("ANOTHER_ONE").unwrap().expose(), "second-legacy-value");
        assert_eq!(migrated.updated_at, 1_700_000_000);

        // The file on disk must now be the encrypted envelope, not the
        // legacy plaintext shape — proving the migration actually wrote
        // back, not just parsed the old format in memory.
        let on_disk = fs::read_to_string(&vault_file).unwrap();
        assert!(!on_disk.contains("legacy-plaintext-value-12345"), "the migrated file must not contain the plaintext");
        let envelope: serde_json::Value = serde_json::from_str(&on_disk).unwrap();
        assert_eq!(envelope["version"], VAULT_FORMAT_VERSION);
        assert!(envelope["payload"].is_string());

        // Loading again must still see the same data, now via the normal
        // encrypted path.
        let reloaded = Vault::load(&temp).unwrap();
        assert_eq!(reloaded.secrets.get("OLD_TOKEN").unwrap().expose(), "legacy-plaintext-value-12345");

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_vault_debug_never_reveals_a_value() {
        let temp = t31_6_temp_dir("debug_redaction");
        set_secret(&temp, "STRIPE_KEY", "sk_live_distinctive_debug_probe_value").unwrap();

        let vault = Vault::load(&temp).unwrap();
        let printed = format!("{vault:?}");

        assert!(
            !printed.contains("sk_live_distinctive_debug_probe_value"),
            "Debug output leaked the secret: {printed}"
        );
        assert!(printed.contains("REDACTED"));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_vault_key_is_reused_across_loads_not_regenerated() {
        // If a new key were generated on every save, every earlier value
        // would become permanently undecryptable — this pins that it isn't.
        let temp = t31_6_temp_dir("stable_key");
        set_secret(&temp, "FIRST", "first-value").unwrap();
        let key_after_first = fs::read(temp.join("vault.key")).unwrap();

        set_secret(&temp, "SECOND", "second-value").unwrap();
        let key_after_second = fs::read(temp.join("vault.key")).unwrap();

        assert_eq!(key_after_first, key_after_second);

        let vault = Vault::load(&temp).unwrap();
        assert_eq!(vault.secrets.get("FIRST").unwrap().expose(), "first-value");
        assert_eq!(vault.secrets.get("SECOND").unwrap().expose(), "second-value");

        let _ = fs::remove_dir_all(&temp);
    }
}
