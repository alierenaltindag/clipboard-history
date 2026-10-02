pub mod blob;
pub mod cache;
pub mod config;
pub mod domain;
pub mod error;
pub mod ipc;
pub mod security;
pub mod storage;

pub use blob::{BlobStore, ThumbnailGenerator};
pub use cache::BoundedCache;
pub use config::{AppConfig, GeneralConfig, HotkeyConfig, PasteConfig, SecurityConfig, UiConfig};
pub use domain::{ClipboardEntry, EntryType};
pub use error::{CoreError, Result};
pub use ipc::{read_message, write_message, DaemonStatus, IpcClient, IpcRequest, IpcResponse};
pub use security::{PasswordManagerGuard, SecretFilter, SecretHandlingPolicy};
pub use storage::SqliteRepository;
