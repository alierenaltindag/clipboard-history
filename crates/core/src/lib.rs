pub mod blob;
pub mod config;
pub mod domain;
pub mod error;
pub mod injector;
pub mod ipc;
pub mod search;
pub mod security;
pub mod storage;
pub mod sync;
pub mod transforms;

pub use blob::{BlobStore, ThumbnailGenerator};
pub use config::{
    AppConfig, AppFilterMode, GeneralConfig, HotkeyConfig, PasteConfig, SecurityConfig, SyncConfig,
    UiConfig,
};
pub use domain::{ClipboardEntry, EntryType, Snippet};
pub use error::{CoreError, Result};
pub use injector::{InjectorCascade, PasteInjector};
pub use ipc::{
    read_message, write_message, DaemonStatus, IpcClient, IpcRequest, IpcResponse, QueueStatus,
};
pub use search::ParsedSearchQuery;
pub use security::{CryptoEngine, PasswordManagerGuard, SecretFilter, SecretHandlingPolicy};
pub use storage::{SqliteRepository, Storage};
pub use transforms::{
    ColorInfo, ConcatDelimiter, DiffEngine, DiffLine, DiffResult, DiffTag, OcrEngine,
    SnippetExpander, TextTransforms, UrlCleaner,
};
