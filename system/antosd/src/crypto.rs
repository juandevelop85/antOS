//! Crate-wide cryptographic primitives (T31.1, T31.5).
//!
//! Centralizes what used to be duplicated or, worse, faked per module:
//! OS-backed secure randomness, hex encoding, constant-time comparison, and
//! Ed25519 signing built on the audited `ed25519-dalek` crate. antOS does
//! not implement elliptic-curve arithmetic itself — that is exactly the
//! kind of primitive where a hand-rolled version is a liability, not a
//! saving (see `mesh.rs`'s retired `md5_hash`, T31.5).

use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use std::fs;
use std::io::Read;

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
}
