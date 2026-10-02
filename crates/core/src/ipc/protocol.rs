use crate::config::AppConfig;
use crate::domain::{ClipboardEntry, EntryType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
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
    Entries(Vec<ClipboardEntry>),
    Entry(Box<Option<ClipboardEntry>>),
    Status(DaemonStatus),
    Config(Box<AppConfig>),
    Blob(Vec<u8>),
    Error(String),
}
