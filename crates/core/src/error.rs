use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),

    #[error("Entry not found: {0}")]
    NotFound(String),

    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("Security rejection: {0}")]
    SecurityRejected(String),

    #[error("IPC communication error: {0}")]
    Ipc(String),

    #[error("Blob not found: {0}")]
    BlobNotFound(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
