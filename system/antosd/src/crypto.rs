//! Crate-wide cryptographic primitives (T31.1, T31.5, T31.6).
//!
//! Centralizes what used to be duplicated or, worse, faked per module:
//! OS-backed secure randomness, hex encoding, constant-time comparison,
//! Ed25519 signing built on the audited `ed25519-dalek` crate, authenticated
//! encryption built on `chacha20poly1305`, secret-value memory hygiene via
//! `zeroize`, and a single hardened writer for every sensitive file this
//! crate touches. antOS does not implement elliptic-curve arithmetic or an
//! AEAD cipher itself — that is exactly the kind of primitive where a
//! hand-rolled version is a liability, not a saving (see `mesh.rs`'s retired
//! `md5_hash`, T31.5).

use anyhow::{bail, Context, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use zeroize::Zeroize;

// --------------------------------------------------------------- encoding

/// Reads `n` bytes of OS-provided cryptographic randomness from
/// `/dev/urandom` (present on both macOS and Linux, the two supported
/// hosts). Secrets — tokens, key material — must never be derived from the
/// clock, a hostname, or any other predictable input.
pub fn secure_random_bytes(n: usize) -> Result<Vec<u8>> {
    let mut f = fs::File::open("/dev/urandom").context("opening /dev/urandom for secure randomness")?;
    let mut buf = vec![0u8; n];
    f.read_exact(&mut buf).context("reading secure randomness from /dev/urandom")?;
    Ok(buf)
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decodes a lowercase or uppercase hex string. `None` on malformed input —
/// callers treat that the same as "does not verify" / "not a valid key",
/// never as a reason to fall back to something less strict.
pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Constant-time comparison of two equal-length, hex-encoded digests, so
/// that validating a token or a digest does not leak it through timing.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in ab.iter().zip(bb.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------- Ed25519

/// An Ed25519 keypair. Holds the secret key in memory only for as long as
/// the caller keeps it — nothing in this type persists anything to disk;
/// that is each caller's own responsibility (e.g. `mesh::MeshEngine` writes
/// the secret half to a `0600` file, separate from the public identity).
pub struct Ed25519Keypair {
    signing_key: SigningKey,
}

impl Ed25519Keypair {
    /// Generates a new keypair from 32 bytes of OS-provided entropy (T31.5)
    /// — never from the clock, a hostname, or any other guessable input.
    pub fn generate() -> Result<Self> {
        let seed = secure_random_bytes(32)?;
        let seed_arr: [u8; 32] = seed
            .try_into()
            .map_err(|_| anyhow::anyhow!("secure_random_bytes(32) returned an unexpected length"))?;
        Ok(Self { signing_key: SigningKey::from_bytes(&seed_arr) })
    }

    /// Reconstructs a keypair from its hex-encoded secret half.
    pub fn from_secret_hex(hex: &str) -> Result<Self> {
        let bytes = from_hex(hex).context("invalid hex-encoded Ed25519 secret key")?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Ed25519 secret key must be exactly 32 bytes"))?;
        Ok(Self { signing_key: SigningKey::from_bytes(&arr) })
    }

    /// The secret key, hex-encoded. Callers must treat this the way they
    /// treat any other secret — `0600` on disk, never logged.
    pub fn secret_hex(&self) -> String {
        to_hex(&self.signing_key.to_bytes())
    }

    /// The public key, hex-encoded — this is what's safe to publish.
    pub fn public_hex(&self) -> String {
        to_hex(self.signing_key.verifying_key().as_bytes())
    }

    /// Signs `message`, returning the hex-encoded signature.
    pub fn sign(&self, message: &[u8]) -> String {
        to_hex(&self.signing_key.sign(message).to_bytes())
    }
}

/// Verifies a hex-encoded Ed25519 signature against a hex-encoded public
/// key. Any malformed input — bad hex, wrong length, an invalid point — is
/// simply "does not verify"; this never panics on attacker-controlled data.
pub fn verify_signature(public_key_hex: &str, message: &[u8], signature_hex: &str) -> bool {
    verify_signature_checked(public_key_hex, message, signature_hex).unwrap_or(false)
}

fn verify_signature_checked(public_key_hex: &str, message: &[u8], signature_hex: &str) -> Result<bool> {
    let pk_bytes = from_hex(public_key_hex).context("malformed public key hex")?;
    let pk_arr: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key must be exactly 32 bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&pk_arr).context("not a valid Ed25519 public key")?;

    let sig_bytes = from_hex(signature_hex).context("malformed signature hex")?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must be exactly 64 bytes"))?;
    let signature = Signature::from_bytes(&sig_arr);

    match verifying_key.verify(message, &signature) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Small helper for call sites that want a hard error instead of a bare
/// `bool` when verification fails — e.g. surfacing *why* to a caller that
/// will report it, rather than only "yes/no".
pub fn verify_signature_or_err(public_key_hex: &str, message: &[u8], signature_hex: &str) -> Result<()> {
    if verify_signature(public_key_hex, message, signature_hex) {
        Ok(())
    } else {
        bail!("Ed25519 signature does not verify against the given public key")
    }
}

// -------------------------------------------------------------------- AEAD

/// Encrypts `plaintext` under `key` with ChaCha20-Poly1305 (T31.6), using a
/// fresh 96-bit nonce drawn from OS entropy for every call — reusing a
/// nonce under the same key is the one mistake that breaks this cipher's
/// guarantees, so it's never left to a caller to get right. Returns
/// `nonce || ciphertext`; there is no valid output shorter than the nonce.
pub fn aead_encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce_bytes = secure_random_bytes(12)?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| anyhow::anyhow!("AEAD encryption failed"))?;

    let mut out = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypts data produced by [`aead_encrypt`] under `key`. Fails closed on
/// anything wrong — truncated input, wrong key, or a tampered ciphertext —
/// never returning a partial or best-effort plaintext.
pub fn aead_decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    const NONCE_LEN: usize = 12;
    if data.len() < NONCE_LEN {
        bail!("ciphertext shorter than the nonce — not a valid AEAD payload");
    }
    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("AEAD decryption failed — wrong key or corrupted/tampered data"))
}

// ------------------------------------------------------------ secret hygiene

/// A secret string that scrubs its buffer on drop and whose `Debug` never
/// prints the value (T31.6) — so a stray `{:?}` on a vault, a struct that
/// embeds one, or an error context built from one can't leak a credential.
/// `Clone`d copies are independently zeroized; `expose()` is the one
/// deliberate escape hatch, named so a reviewer can grep for where a secret
/// actually leaves this wrapper.
#[derive(Clone, Serialize, Deserialize)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The plaintext value. Named loudly on purpose.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretValue([REDACTED]; {} bytes)", self.0.len())
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl PartialEq for SecretValue {
    fn eq(&self, other: &Self) -> bool {
        constant_time_eq(&to_hex(self.0.as_bytes()), &to_hex(other.0.as_bytes()))
    }
}

// --------------------------------------------------------- hardened writer

/// Writes `contents` to `path` atomically, with owner-only (`0600`)
/// permissions from the instant the file exists — there is never a window
/// where a laxer `umask` applies (T31.6, replacing the write-then-`chmod`
/// pattern this module's callers used to duplicate).
///
/// Writes to a sibling temp file first, `fsync`s it, then renames it over
/// `path`. The rename is atomic on the same filesystem, so a crash or kill
/// mid-write can never leave a truncated file where `path` used to be —
/// the previous version stays intact until the new one is fully durable.
pub fn write_secret_file(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| anyhow::anyhow!("refusing to write {}: no parent directory", path.display()))?;
    fs::create_dir_all(parent)?;

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("secret");
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
    // A previous attempt may have died mid-write and left this behind;
    // `create_new` below must start from a clean slate.
    let _ = fs::remove_file(&tmp_path);

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp_path)
            .with_context(|| format!("creating {}", tmp_path.display()))?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        fs::write(&tmp_path, contents).with_context(|| format!("writing {}", tmp_path.display()))?;
    }

    fs::rename(&tmp_path, path).with_context(|| format!("renaming {} to {}", tmp_path.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_roundtrip() {
        let bytes = [0x00, 0x0f, 0xa0, 0xff, 0x7b];
        let hex = to_hex(&bytes);
        assert_eq!(hex, "000fa0ff7b");
        assert_eq!(from_hex(&hex).unwrap(), bytes);
    }

    #[test]
    fn test_from_hex_rejects_malformed_input() {
        assert!(from_hex("abc").is_none()); // odd length
        assert!(from_hex("zz").is_none()); // not hex digits
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("abcd", "abcd"));
        assert!(!constant_time_eq("abcd", "abce"));
        assert!(!constant_time_eq("abc", "abcd"));
    }

    #[test]
    fn test_secure_random_bytes_are_not_all_zero_and_vary() {
        let a = secure_random_bytes(32).unwrap();
        let b = secure_random_bytes(32).unwrap();
        assert_eq!(a.len(), 32);
        assert_ne!(a, vec![0u8; 32]);
        assert_ne!(a, b, "two independent draws must not coincide");
    }

    #[test]
    fn test_ed25519_sign_and_verify_round_trip() {
        let keypair = Ed25519Keypair::generate().unwrap();
        let message = b"antOS mesh handshake";
        let signature = keypair.sign(message);

        assert!(verify_signature(&keypair.public_hex(), message, &signature));
    }

    #[test]
    fn test_ed25519_verify_rejects_tampered_message() {
        let keypair = Ed25519Keypair::generate().unwrap();
        let signature = keypair.sign(b"original message");

        assert!(!verify_signature(&keypair.public_hex(), b"tampered message", &signature));
    }

    #[test]
    fn test_ed25519_verify_rejects_wrong_public_key() {
        let signer = Ed25519Keypair::generate().unwrap();
        let impostor = Ed25519Keypair::generate().unwrap();
        let message = b"who am I really talking to?";
        let signature = signer.sign(message);

        assert!(!verify_signature(&impostor.public_hex(), message, &signature));
    }

    #[test]
    fn test_ed25519_verify_rejects_malformed_input_without_panicking() {
        assert!(!verify_signature("not-hex-zz", b"msg", "also-not-hex"));
        assert!(!verify_signature("ab", b"msg", "cd")); // too short to be a real key/signature
    }

    #[test]
    fn test_ed25519_keypair_from_secret_hex_round_trip() {
        let original = Ed25519Keypair::generate().unwrap();
        let restored = Ed25519Keypair::from_secret_hex(&original.secret_hex()).unwrap();
        assert_eq!(original.public_hex(), restored.public_hex());
    }

    // -------------------------------------------------------------- T31.6

    fn test_key() -> [u8; 32] {
        secure_random_bytes(32).unwrap().try_into().unwrap()
    }

    #[test]
    fn test_aead_round_trip() {
        let key = test_key();
        let plaintext = b"antOS vault secret payload";
        let ciphertext = aead_encrypt(&key, plaintext).unwrap();

        assert_ne!(ciphertext.as_slice(), plaintext, "ciphertext must not equal the plaintext");
        assert!(
            !ciphertext.windows(plaintext.len()).any(|w| w == plaintext),
            "the plaintext must not appear anywhere in the ciphertext"
        );

        let decrypted = aead_decrypt(&key, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aead_two_encryptions_of_the_same_plaintext_differ() {
        // A fresh random nonce every call means identical plaintexts never
        // produce identical ciphertexts — nonce reuse is the one mistake
        // that breaks this cipher, so it must be structurally impossible.
        let key = test_key();
        let a = aead_encrypt(&key, b"same plaintext, twice").unwrap();
        let b = aead_encrypt(&key, b"same plaintext, twice").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn test_aead_decrypt_rejects_wrong_key() {
        let plaintext = b"top secret";
        let ciphertext = aead_encrypt(&test_key(), plaintext).unwrap();
        assert!(aead_decrypt(&test_key(), &ciphertext).is_err());
    }

    #[test]
    fn test_aead_decrypt_rejects_tampered_ciphertext() {
        let key = test_key();
        let mut ciphertext = aead_encrypt(&key, b"do not modify me").unwrap();
        let last = ciphertext.len() - 1;
        ciphertext[last] ^= 0xFF;
        assert!(aead_decrypt(&key, &ciphertext).is_err());
    }

    #[test]
    fn test_aead_decrypt_rejects_truncated_input_without_panicking() {
        let key = test_key();
        assert!(aead_decrypt(&key, b"").is_err());
        assert!(aead_decrypt(&key, b"short").is_err());
    }

    #[test]
    fn test_secret_value_debug_never_prints_the_plaintext() {
        let secret = SecretValue::new("sk_live_super_secret_token".to_string());
        let printed = format!("{secret:?}");
        assert!(!printed.contains("sk_live_super_secret_token"), "got: {printed}");
        assert!(printed.contains("REDACTED"));
    }

    #[test]
    fn test_secret_value_expose_returns_the_plaintext() {
        let secret = SecretValue::new("the-actual-value".to_string());
        assert_eq!(secret.expose(), "the-actual-value");
    }

    #[test]
    fn test_write_secret_file_is_atomic_and_owner_only() {
        let dir = std::env::temp_dir().join(format!("antos_test_write_secret_file_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("secret.bin");

        write_secret_file(&target, b"first version").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"first version");

        write_secret_file(&target, b"second, longer version").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"second, longer version");

        // No leftover temp file after a successful write.
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "temp file left behind: {leftovers:?}");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn test_write_secret_file_is_never_briefly_world_or_group_readable() {
        // SAFETY: this test temporarily loosens the process umask to prove
        // write_secret_file doesn't rely on an already-strict umask for its
        // 0600 guarantee — the file must be created pre-restricted via
        // OpenOptions::mode, not merely chmod'd afterward under a lucky
        // umask. Restored unconditionally before returning.
        unsafe {
            let previous = libc::umask(0o000);
            let result = std::panic::catch_unwind(|| {
                use std::os::unix::fs::PermissionsExt;

                let dir = std::env::temp_dir().join(format!("antos_test_write_secret_file_umask_{}", std::process::id()));
                let _ = fs::remove_dir_all(&dir);
                fs::create_dir_all(&dir).unwrap();
                let target = dir.join("secret.bin");

                write_secret_file(&target, b"sensitive").unwrap();
                let mode = fs::metadata(&target).unwrap().permissions().mode();
                assert_eq!(mode & 0o777, 0o600, "must be 0600 even with a permissive umask");

                let _ = fs::remove_dir_all(&dir);
            });
            libc::umask(previous);
            result.unwrap();
        }
    }
}
