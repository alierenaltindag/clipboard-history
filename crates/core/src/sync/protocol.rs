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
    },
    /// Ping/pong heartbeat
    Ping,
    Pong,
}
