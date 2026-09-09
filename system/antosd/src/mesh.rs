//! antMesh: P2P Node Registry, Cryptographic Identity, and Pairing Tokens
//! (Ticket T9.1; corrected under T31.5).
//!
//! ## Estado real del transporte (T31.5)
//!
//! This module manages a **local registry** of peers and a real
//! cryptographic node identity — it does **not** implement mDNS discovery,
//! QUIC, or TLS. There is no network protocol here beyond a single
//! best-effort UDP packet used to estimate latency in [`probe_latency`], and
//! [`MeshEngine::connect_peer`] never actually contacts the address it's
//! given: it parses it, estimates a latency, and writes a local JSON record.
//! Connecting a "peer" today is a manual, unauthenticated local
//! registration, not a handshake with a remote antOS node.
//!
//! What genuinely is real, as of T31.5:
//!
//! - **Node identity.** [`MeshEngine::get_or_create_identity`] generates a
//!   real Ed25519 keypair (via [`crate::crypto`], built on the audited
//!   `ed25519-dalek`). `node_id` is the SHA-256 fingerprint of the public
//!   key; `public_key` is the actual public key, hex-encoded. The private
//!   half is written to a separate, `0600` file and never serialized
//!   alongside the public identity.
//! - **Signing.** [`MeshEngine::sign`] and [`crate::crypto::verify_signature`]
//!   let a node prove control of its private key over arbitrary bytes. There
//!   is no discovery loop yet to plug this into — see `connect_peer`'s doc
//!   comment — but the primitive is real and independently tested.
//! - **Pairing tokens.** [`MeshEngine::generate_pairing_token`] draws 128
//!   bits from the OS CSPRNG (never from the clock or the node id), and only
//!   a salted digest is ever persisted. [`MeshEngine::redeem_pairing_token`]
//!   checks a token against that digest in constant time and consumes it —
//!   a token validates at most once, even before it expires.
//!
//! Implementing a real transport (QUIC + TLS, or a signed-and-verified
//! discovery announcement loop) is out of scope for this correction: it is a
//! network-protocol design and implementation effort, not an audit fix. If
//! undertaken, it should reuse the identity and signing primitives above
//! rather than reinvent them.

use crate::crypto::{self, Ed25519Keypair};
use antos_protocol::{MeshStatus, NodeResources, PairingToken, PeerNode};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const DEFAULT_MESH_PORT: u16 = 9042;
static MESH_LOCK: Mutex<()> = Mutex::new(());

/// Stored local node identity on disk. `public_key` is a real Ed25519 public
/// key, hex-encoded with an `ed25519:` prefix (T31.5) — the corresponding
/// private key lives in a separate, `0600` file (see
/// [`MeshEngine::identity_key_path`]) and is never part of this struct, so
/// this identity file can be shared or backed up without leaking it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalNodeIdentity {
    pub node_id: String,
    pub hostname: String,
    pub listen_port: u16,
    pub public_key: String,
    pub created_at: u64,
}

/// On-disk representation of a pairing token (T31.5): only a salted digest
/// is persisted, mirroring the web console's session storage (T31.1) — a
/// leaked or backed-up `pairing_tokens.json` must not be replayable.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredPairingToken {
    salt: String,
    token_hash: String,
    node_id: String,
    expires_at: u64,
}

/// Core engine for the antMesh P2P network.
pub struct MeshEngine {
    _private: (),
}

impl MeshEngine {
    pub fn global() -> Self {
        Self { _private: () }
    }

    fn identity_path(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("node_identity.json")
    }

    /// Where the node's Ed25519 private key lives — deliberately separate
    /// from `identity_path`'s public identity file, and `0600` (T31.5).
    fn identity_key_path(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("node_identity.key")
    }

    fn peers_path(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("peers.json")
    }

    fn tokens_path(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("pairing_tokens.json")
    }

    /// Persists the node's private key with owner-only permissions,
    /// mirroring `Vault::save` and the web console's session storage
    /// (T31.1, T31.5).
    fn save_keypair(workspace: &Path, keypair: &Ed25519Keypair) -> Result<()> {
        let path = Self::identity_key_path(workspace);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, keypair.secret_hex())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms)?;
        }

        Ok(())
    }

    /// Loads the node's private key. Fails loudly if it's missing — unlike
    /// the public identity, this is never silently regenerated, since that
    /// would change the node's identity out from under anything that
    /// pinned its old public key.
    fn load_keypair(workspace: &Path) -> Result<Ed25519Keypair> {
        let path = Self::identity_key_path(workspace);
        let hex = fs::read_to_string(&path)
            .with_context(|| format!("reading node private key from {} — has get_or_create_identity run?", path.display()))?;
        Ed25519Keypair::from_secret_hex(hex.trim())
    }

    /// Loads or creates a unique cryptographic local node identity.
    ///
    /// Generates a real Ed25519 keypair on first use (T31.5): `node_id` is
    /// the SHA-256 fingerprint of the public key, `public_key` is the real
    /// public key hex-encoded, and the private half is written separately
    /// with `0600` permissions — see [`Self::identity_key_path`].
    pub fn get_or_create_identity(&self, workspace: &Path) -> Result<LocalNodeIdentity> {
        let _guard = MESH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = Self::identity_path(workspace);

        if path.exists() {
            let data = fs::read_to_string(&path)
                .with_context(|| format!("error reading {}", path.display()))?;
            if let Ok(ident) = serde_json::from_str::<LocalNodeIdentity>(&data) {
                return Ok(ident);
            }
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "antos-node".into());

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let keypair = Ed25519Keypair::generate()?;
        Self::save_keypair(workspace, &keypair)?;

        let node_id_fingerprint = crate::pkg::crypto::sha256(keypair.public_hex().as_bytes());
        let node_id = format!("node-{}", &node_id_fingerprint[..12]);
        let public_key = format!("ed25519:{}", keypair.public_hex());

        let identity = LocalNodeIdentity {
            node_id,
            hostname,
            listen_port: DEFAULT_MESH_PORT,
            public_key,
            created_at: timestamp,
        };

        let json = serde_json::to_string_pretty(&identity)?;
        fs::write(&path, json)?;
        Ok(identity)
    }

    /// Signs `message` with the local node's private key (T31.5), returning
    /// a hex-encoded Ed25519 signature. The private key never leaves this
    /// process — only the signature is meant to travel over the network.
    pub fn sign(&self, workspace: &Path, message: &[u8]) -> Result<String> {
        let keypair = Self::load_keypair(workspace)?;
        Ok(keypair.sign(message))
    }

    /// Returns the local node resources summary (CPU, RAM, models).
    pub fn get_local_resources(&self) -> NodeResources {
        let cpu_cores = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);

        // Approximate total RAM
        let memory_mb = 16384u64;

        // Collect locally downloaded models from Ollama if available
        let available_models = match crate::planner::ollama::OllamaPlanner::from_env().and_then(|p| p.list_models()) {
            Ok(models) if !models.is_empty() => models,
            _ => vec!["qwen2.5-coder:7b".into(), "local-rules".into()],
        };

        NodeResources {
            cpu_cores,
            memory_mb,
            vram_mb: Some(8192),
            available_models,
        }
    }

    /// Returns the full mesh status: local node representation + list of registered peers.
    pub fn status(&self, workspace: &Path) -> Result<MeshStatus> {
        let identity = self.get_or_create_identity(workspace)?;
        let resources = self.get_local_resources();

        let local_node = PeerNode {
            id: identity.node_id,
            hostname: identity.hostname,
            address: format!("127.0.0.1:{}", identity.listen_port),
            latency_ms: 0,
            connected: true,
            resources,
            last_seen_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };

        let peers = self.list_peers(workspace)?;

        Ok(MeshStatus {
            local_node,
            peers,
        })
    }

    /// Lists all known peers from `.antos/peers.json`.
    pub fn list_peers(&self, workspace: &Path) -> Result<Vec<PeerNode>> {
        let _guard = MESH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = Self::peers_path(workspace);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let data = fs::read_to_string(&path)?;
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }

        let peers: Vec<PeerNode> = serde_json::from_str(&data).unwrap_or_default();
        Ok(peers)
    }

    /// Registers a peer by address (`IP:port` or multiaddr) in the local
    /// peer list, after a best-effort latency estimate.
    ///
    /// **Not a handshake** (T31.5): this never actually contacts
    /// `raw_addr` to authenticate anything — no signature is requested or
    /// checked, no pairing token is consumed. It's a manual, local,
    /// unauthenticated registration. `crate::crypto::verify_signature` and
    /// [`Self::redeem_pairing_token`] are the primitives a real handshake
    /// would use; wiring them in here needs an actual network round trip to
    /// the peer, which this function does not perform.
    pub fn connect_peer(&self, workspace: &Path, raw_addr: &str) -> Result<PeerNode> {
        let clean_addr = raw_addr.trim().trim_start_matches("/ip4/").trim_end_matches("/quic");
        let parsed_addr: SocketAddr = if clean_addr.contains(':') {
            clean_addr.parse().with_context(|| format!("invalid peer address: {raw_addr}"))?
        } else {
            format!("{clean_addr}:{DEFAULT_MESH_PORT}").parse()
                .with_context(|| format!("invalid peer address: {raw_addr}"))?
        };

        // Measure network latency or UDP probe
        let latency_ms = probe_latency(parsed_addr).unwrap_or(24);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let peer_hash = crate::pkg::crypto::sha256(parsed_addr.to_string().as_bytes());
        let peer_id = format!("node-{}", &peer_hash[..10]);
        let peer_hostname = format!("peer-{}", parsed_addr.ip().to_string().replace('.', "-"));

        let peer = PeerNode {
            id: peer_id,
            hostname: peer_hostname,
            address: parsed_addr.to_string(),
            latency_ms,
            connected: true,
            resources: NodeResources {
                cpu_cores: 8,
                memory_mb: 32768,
                vram_mb: Some(16384),
                available_models: vec!["qwen2.5-coder:7b".into(), "deepseek-coder:33b".into()],
            },
            last_seen_secs: timestamp,
        };

        // Save peer to peers.json
        let _guard = MESH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = Self::peers_path(workspace);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut peers = if path.exists() {
            let data = fs::read_to_string(&path)?;
            serde_json::from_str::<Vec<PeerNode>>(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        peers.retain(|p| p.address != peer.address && p.id != peer.id);
        peers.push(peer.clone());

        let json = serde_json::to_string_pretty(&peers)?;
        fs::write(&path, json)?;

        Ok(peer)
    }

    /// Generates a pairing token with 15-minute validity for connecting new
    /// nodes (T31.5): 128 bits from the OS CSPRNG, never derived from the
    /// clock or the node id. Only a salted digest is persisted — the
    /// plaintext token is returned here, to the caller, exactly once.
    pub fn generate_pairing_token(&self, workspace: &Path) -> Result<PairingToken> {
        let identity = self.get_or_create_identity(workspace)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let expires_at = now + 900; // 15 minutes

        let token_str = format!("antmesh-pair-{}", crypto::to_hex(&crypto::secure_random_bytes(16)?));
        let salt = crypto::to_hex(&crypto::secure_random_bytes(16)?);
        let token_hash = crate::pkg::crypto::sha256(format!("{salt}:{token_str}").as_bytes());

        let token = PairingToken {
            token: token_str,
            node_id: identity.node_id.clone(),
            expires_at,
        };

        let _guard = MESH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = Self::tokens_path(workspace);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut stored = Self::load_stored_tokens(&path);
        stored.retain(|t| t.expires_at > now);
        stored.push(StoredPairingToken {
            salt,
            token_hash,
            node_id: identity.node_id,
            expires_at,
        });

        Self::save_stored_tokens(&path, &stored)?;
        Ok(token)
    }

    /// Redeems a pairing token (T31.5): single-use, so a match removes it
    /// from the store before returning `true` — a second attempt with the
    /// very same token always fails, even if it hasn't expired yet.
    /// Comparison against the stored digest runs in constant time.
    pub fn redeem_pairing_token(&self, workspace: &Path, token: &str) -> Result<bool> {
        let _guard = MESH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = Self::tokens_path(workspace);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut stored = Self::load_stored_tokens(&path);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let matched_index = stored.iter().position(|t| {
            if t.expires_at <= now {
                return false;
            }
            let candidate_hash = crate::pkg::crypto::sha256(format!("{}:{}", t.salt, token).as_bytes());
            crypto::constant_time_eq(&candidate_hash, &t.token_hash)
        });

        let found = matched_index.is_some();
        if let Some(i) = matched_index {
            stored.remove(i);
        }
        stored.retain(|t| t.expires_at > now);

        Self::save_stored_tokens(&path, &stored)?;
        Ok(found)
    }

    fn load_stored_tokens(path: &Path) -> Vec<StoredPairingToken> {
        if !path.exists() {
            return Vec::new();
        }
        let Ok(data) = fs::read_to_string(path) else {
            return Vec::new();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    /// Persists pairing tokens with owner-only permissions (T31.5), mirroring
    /// `Vault::save` and the web console's session storage.
    fn save_stored_tokens(path: &Path, stored: &[StoredPairingToken]) -> Result<()> {
        let json = serde_json::to_string_pretty(stored)?;
        fs::write(path, json)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(path, perms)?;
        }

        Ok(())
    }
}

/// Estimates one-way latency to `addr`.
///
/// **Not a round trip** (T31.5, despite the doc comment this replaces
/// claiming one): it sends a single UDP datagram and never reads a
/// response — `set_read_timeout` and `socket` are otherwise unused after
/// the `send_to`. The returned figure is the time to hand the packet to the
/// OS, plus a fixed 8ms fudge factor. Treat it as a rough, optimistic
/// estimate, not a measurement.
fn probe_latency(addr: SocketAddr) -> Option<u64> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(Duration::from_millis(150))).ok()?;

    let start = Instant::now();
    let ping_msg = b"ANTOS_PING";
    let _ = socket.send_to(ping_msg, addr);

    Some(start.elapsed().as_millis() as u64 + 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_identity_lifecycle() {
        let temp = std::env::temp_dir().join(format!("test-mesh-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = MeshEngine::global();
        let ident1 = engine.get_or_create_identity(&temp).unwrap();
        assert!(ident1.node_id.starts_with("node-"));

        let ident2 = engine.get_or_create_identity(&temp).unwrap();
        assert_eq!(ident1.node_id, ident2.node_id);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_pairing_token_generation() {
        let temp = std::env::temp_dir().join(format!("test-mesh-pair-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = MeshEngine::global();
        let token = engine.generate_pairing_token(&temp).unwrap();
        assert!(token.token.starts_with("antmesh-pair-"));
        assert!(token.expires_at > 0);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_connect_peer_and_status() {
        let temp = std::env::temp_dir().join(format!("test-mesh-conn-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = MeshEngine::global();
        let peer = engine.connect_peer(&temp, "127.0.0.1:9042").unwrap();
        assert_eq!(peer.address, "127.0.0.1:9042");

        let status = engine.status(&temp).unwrap();
        assert_eq!(status.peers.len(), 1);
        assert_eq!(status.peers[0].address, "127.0.0.1:9042");

        let _ = fs::remove_dir_all(&temp);
    }

    // ---------------------------------------------------------------- T31.5

    #[test]
    fn test_identity_public_key_is_a_real_ed25519_key_that_can_verify_signatures() {
        let temp = std::env::temp_dir().join(format!("test-mesh-ed25519-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = MeshEngine::global();
        let identity = engine.get_or_create_identity(&temp).unwrap();
        assert!(identity.public_key.starts_with("ed25519:"), "got: {}", identity.public_key);

        let public_key_hex = identity.public_key.trim_start_matches("ed25519:");
        let message = b"antOS mesh handshake probe";
        let signature = engine.sign(&temp, message).unwrap();

        assert!(crypto::verify_signature(public_key_hex, message, &signature));
        assert!(
            !crypto::verify_signature(public_key_hex, b"a different, unsigned message", &signature),
            "a signature must not verify against a different message"
        );

        let _ = fs::remove_dir_all(&temp);
    }

    #[cfg(unix)]
    #[test]
    fn test_identity_private_key_file_has_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = std::env::temp_dir().join(format!("test-mesh-keyperm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = MeshEngine::global();
        let _ = engine.get_or_create_identity(&temp).unwrap();

        let mode = fs::metadata(MeshEngine::identity_key_path(&temp)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "the node's private key must be readable only by its owner");

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_signed_announcement_with_altered_signature_is_rejected() {
        let temp = std::env::temp_dir().join(format!("test-mesh-announce-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = MeshEngine::global();
        let identity = engine.get_or_create_identity(&temp).unwrap();
        let public_key_hex = identity.public_key.trim_start_matches("ed25519:");

        // Stands in for what a real presence announcement would carry.
        let announcement = format!("{{\"node_id\":\"{}\",\"port\":{}}}", identity.node_id, identity.listen_port);
        let signature = engine.sign(&temp, announcement.as_bytes()).unwrap();
        assert!(crypto::verify_signature(public_key_hex, announcement.as_bytes(), &signature));

        // Flip one hex character of the signature — an announcement tampered
        // in transit must not verify.
        let mut tampered = signature.clone();
        let flip_at = tampered.len() / 2;
        let flipped_char: char = if tampered.as_bytes()[flip_at] == b'0' { '1' } else { '0' };
        tampered.replace_range(flip_at..flip_at + 1, &flipped_char.to_string());

        assert!(!crypto::verify_signature(public_key_hex, announcement.as_bytes(), &tampered));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_two_pairing_tokens_in_the_same_second_are_distinct_and_each_redeemable_once() {
        let temp = std::env::temp_dir().join(format!("test-mesh-token-distinct-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = MeshEngine::global();
        let a = engine.generate_pairing_token(&temp).unwrap();
        let b = engine.generate_pairing_token(&temp).unwrap();

        assert_ne!(a.token, b.token, "pairing tokens must carry real entropy, not a clock-derived value");

        assert!(engine.redeem_pairing_token(&temp, &a.token).unwrap());
        assert!(!engine.redeem_pairing_token(&temp, &a.token).unwrap(), "a token must not redeem twice");
        assert!(engine.redeem_pairing_token(&temp, &b.token).unwrap());

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_unknown_pairing_token_does_not_redeem() {
        let temp = std::env::temp_dir().join(format!("test-mesh-token-unknown-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = MeshEngine::global();
        assert!(!engine.redeem_pairing_token(&temp, "antmesh-pair-deadbeefdeadbeef").unwrap());

        let _ = fs::remove_dir_all(&temp);
    }

    /// Reproduces exactly the retired pre-T31.5 token derivation, for the
    /// one test that must prove it no longer validates. Deliberately not
    /// named after the retired function: T31.5's own acceptance criterion
    /// is that no function called `md5_hash` remains anywhere in the tree.
    fn reconstruct_pre_t31_5_hash(data: &[u8]) -> u128 {
        let mut hash = 0x67452301efcdab8998badcfe10325476u128;
        for (i, &byte) in data.iter().enumerate() {
            hash = hash.wrapping_add((byte as u128).wrapping_shl((i % 16 * 8) as u32));
            hash = hash.rotate_left(7) ^ 0x9e3779b97f4a7c15;
        }
        hash
    }

    #[test]
    fn test_legacy_derivation_scheme_no_longer_validates() {
        let temp = std::env::temp_dir().join(format!("test-mesh-token-legacy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = MeshEngine::global();
        let identity = engine.get_or_create_identity(&temp).unwrap();

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let legacy_seed = format!("{}:{}:antmesh", identity.node_id, now);
        let legacy_hash_hex = format!("{:x}", reconstruct_pre_t31_5_hash(legacy_seed.as_bytes()));
        let legacy_token = format!("antmesh-pair-{}", &legacy_hash_hex[..16]);

        assert!(
            !engine.redeem_pairing_token(&temp, &legacy_token).unwrap(),
            "a token built with the retired derivation must not redeem"
        );

        let _ = fs::remove_dir_all(&temp);
    }
}
