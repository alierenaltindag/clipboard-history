pub mod blob;
pub mod clipboard;
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
pub use clipboard::copy_to_system_clipboard;
pub use config::{
    AppConfig, AppFilterMode, GeneralConfig, HotkeyConfig, PasteConfig, SecurityConfig, SyncConfig,
    UiConfig,
};
pub use domain::{ClipboardEntry, EntryType, Snippet};
pub use error::{CoreError, Result};
pub use injector::{InjectorCascade, PasteInjector};
pub use ipc::{
    read_message, write_message, DaemonStatus, FirewallCheckResult, IpcClient, IpcRequest,
    IpcResponse,
};
pub use search::ParsedSearchQuery;
pub use security::{
    apply_process_memory_shield, lock_process_memory, protect_sensitive_buffer, wipe_and_protect,
    CryptoEngine, PasswordManagerGuard, SecretFilter, SecretHandlingPolicy,
};
pub use storage::{SqliteRepository, Storage};
pub use sync::check_firewall_status;
pub use transforms::{
    ColorInfo, ConcatDelimiter, DiffEngine, DiffLine, DiffResult, DiffTag, OcrEngine,
    SnippetExpander, TextTransforms, UrlCleaner,
};
