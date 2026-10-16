use crate::config::AppConfig;
use crate::domain::{ClipboardEntry, EntryType, Snippet};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallCheckResult {
    pub is_firewall_active: bool,
    pub firewall_name: String,
    pub has_issue: bool,
    pub description: String,
    pub recommended_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    Authenticate {
        auth_token: String,
    },
    ToggleWindow,
    ShowWindow,
    HideWindow,
    GetEntries {
        limit: usize,
        offset: usize,
        filter: Option<EntryType>,
        pinned_only: bool,
    },
    Search {
        query: String,
        limit: usize,
        offset: usize,
    },
    GetEntry {
        id: String,
    },
    SelectAndPaste {
        id: String,
    },
    PinEntry {
        id: String,
    },
    UnpinEntry {
        id: String,
    },
    DeleteEntry {
        id: String,
    },
    ClearHistory {
        include_pinned: bool,
    },
    GetStatus,
    SetPause {
        paused: bool,
    },
    GetConfig,
    UpdateConfig {
        config: Box<AppConfig>,
    },
    GetBlob {
        hash: String,
    },
    AddManualEntry {
        text: String,
    },
    GetSyncPeers,
    TriggerSyncScan,
    CheckFirewall,
    SendPairingRequest {
        peer_address: String,
    },
    RespondPairingRequest {
        request_id: String,
        accept: bool,
    },
    UnpairPeer {
        peer_identifier: String,
    },
    GetPendingPairingRequests,
    GetTrustedPeers,
    // Snippets requests
    ListSnippets {
        category: Option<String>,
    },
    CreateSnippet {
        label: String,
        content: String,
        category: Option<String>,
    },
    UpdateSnippet {
        snippet: Box<Snippet>,
    },
    DeleteSnippet {
        id: String,
    },
    UseSnippet {
        id: String,
    },
    // OCR request
    PerformOcr {
        blob_hash: String,
    },
    // Batch operations
    BatchDelete {
        ids: Vec<String>,
    },
    BatchPin {
        ids: Vec<String>,
        pinned: bool,
    },
    // Diff comparison
    ComputeDiff {
        id_a: String,
        id_b: String,
    },
    // Paste echo suppression
    PreparePaste {
        content_hash: String,
    },
    // Touch entry (update last_used_at to move to top)
    TouchEntry {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPairingInfo {
    pub request_id: String,
    pub device_name: String,
    pub peer_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub total_entries: usize,
    pub is_paused: bool,
    pub uptime_secs: u64,
    pub db_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    Success,
    Authenticated,
    Entries(Vec<ClipboardEntry>),
    Entry(Box<Option<ClipboardEntry>>),
    Status(DaemonStatus),
    Config(Box<AppConfig>),
    Blob(Vec<u8>),
    Snippets(Vec<Snippet>),
    Snippet(Box<Snippet>),
    FirewallStatus(FirewallCheckResult),
    OcrResult {
        text: String,
    },
    BatchSuccess {
        count: usize,
    },
    SyncPeers(Vec<String>),
    TrustedPeers(Vec<String>),
    PendingPairings(Vec<PendingPairingInfo>),
    PairingResult {
        success: bool,
        message: String,
        peer_address: String,
        device_name: String,
    },
    DiffResult(Box<crate::transforms::DiffResult>),
    Error(String),
}
