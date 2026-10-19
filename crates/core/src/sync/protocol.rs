use crate::domain::ClipboardEntry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// Authentication handshake with pairing PIN
    AuthRequest {
        device_name: String,
        pairing_pin: String,
        nonce: u64,
    },
    AuthResponse {
        success: bool,
        device_name: String,
        message: String,
    },
    /// Encrypted clipboard entry sync
    EntrySync {
        origin_device: String,
        entry: Box<ClipboardEntry>,
        #[serde(default)]
        blob_payload: Option<Vec<u8>>,
        #[serde(default)]
        thumbnail_payload: Option<Vec<u8>>,
        #[serde(default)]
        is_history_batch: bool,
    },
    /// Ping/pong heartbeat
    Ping,
    Pong,
    /// Explicit user-approved mutual pairing request
    PairingRequest {
        origin_device: String,
        pairing_pin: String,
        request_id: String,
    },
    PairingResponse {
        request_id: String,
        accepted: bool,
        device_name: String,
        message: String,
    },
    UnpairRequest {
        origin_device: String,
    },
    UnpairResponse {
        success: bool,
        message: String,
    },
    /// Request mutual history sync from peer
    HistorySyncRequest {
        origin_device: String,
        limit: usize,
    },
    /// Acknowledge completion of history sync stream
    HistorySyncComplete {
        origin_device: String,
        count: usize,
    },
    /// Request clearing of history across paired devices
    ClearHistorySync {
        origin_device: String,
        include_pinned: bool,
    },
    /// Sync pin/unpin status for an entry across paired devices (with optional payload for newly pinned items)
    PinSync {
        origin_device: String,
        content_hash: String,
        is_pinned: bool,
        #[serde(default)]
        entry: Option<Box<ClipboardEntry>>,
        #[serde(default)]
        blob_payload: Option<Vec<u8>>,
        #[serde(default)]
        thumbnail_payload: Option<Vec<u8>>,
    },
}
