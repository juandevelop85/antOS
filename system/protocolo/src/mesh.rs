//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};

// ============================================================================
// antMesh: P2P Network, Discovery, and Peer Telemetry
// ============================================================================

// ----------------------------------------------------------- p2p network / antMesh (T9.1)

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeResources {
    pub cpu_cores: usize,
    pub memory_mb: u64,
    pub vram_mb: Option<u64>,
    pub available_models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerNode {
    pub id: String,
    pub hostname: String,
    pub address: String,
    pub latency_ms: u64,
    pub connected: bool,
    pub resources: NodeResources,
    pub last_seen_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingToken {
    pub token: String,
    pub node_id: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshStatus {
    pub local_node: PeerNode,
    pub peers: Vec<PeerNode>,
}
