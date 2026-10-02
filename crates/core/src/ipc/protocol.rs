use crate::config::AppConfig;
use crate::domain::{ClipboardEntry, EntryType, Snippet};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueStatus {
    pub active: bool,
    pub remaining_count: usize,
    pub next_preview: Option<String>,
}

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
    // Sequential Paste Queue requests
    EnqueueItems {
        ids: Vec<String>,
    },
    ClearQueue,
    GetQueueStatus,
    PopAndPasteQueue,
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
    Snippets(Vec<Snippet>),
    Snippet(Box<Snippet>),
    QueueStatus(QueueStatus),
    QueuePopped {
        remaining_count: usize,
        pasted: bool,
        text: Option<String>,
    },
    OcrResult {
        text: String,
    },
    BatchSuccess {
        count: usize,
    },
    DiffResult(Box<crate::transforms::DiffResult>),
    Error(String),
}
