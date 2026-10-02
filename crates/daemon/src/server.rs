use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::error::Result;
use clipboard_history_core::ipc::{
    read_message, write_message, DaemonStatus, IpcRequest, IpcResponse,
};
use clipboard_history_core::storage::SqliteRepository;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

pub struct DaemonServer {
    repo: SqliteRepository,
    blob_store: BlobStore,
    config: Arc<RwLock<AppConfig>>,
    is_paused: Arc<AtomicBool>,
    socket_path: PathBuf,
    start_time: Instant,
}

impl DaemonServer {
    pub fn new(
        repo: SqliteRepository,
        blob_store: BlobStore,
        config: Arc<RwLock<AppConfig>>,
        is_paused: Arc<AtomicBool>,
        socket_path: PathBuf,
    ) -> Self {
        Self {
            repo,
            blob_store,
            config,
            is_paused,
            socket_path,
            start_time: Instant::now(),
        }
    }

    pub async fn run(self) -> Result<()> {
        if self.socket_path.exists() {
            debug!("Removing existing socket at {}", self.socket_path.display());
            let _ = fs::remove_file(&self.socket_path);
        }

        if let Some(parent) = self.socket_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        let _ = fs::set_permissions(&self.socket_path, fs::Permissions::from_mode(0o600));

        info!("IPC daemon listening on {}", self.socket_path.display());

        let repo = self.repo;
        let blob_store = self.blob_store;
        let config = self.config;
        let is_paused = self.is_paused;
        let start_time = self.start_time;

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let repo_cloned = repo.clone();
                    let blob_cloned = blob_store.clone();
                    let config_cloned = Arc::clone(&config);
                    let paused_cloned = Arc::clone(&is_paused);

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(
                            stream,
                            repo_cloned,
                            blob_cloned,
                            config_cloned,
                            paused_cloned,
                            start_time,
                        )
                        .await
                        {
                            debug!("Client connection terminated: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Error accepting IPC connection: {}", e);
                }
            }
        }
    }

    async fn handle_client(
        stream: UnixStream,
        repo: SqliteRepository,
        blob_store: BlobStore,
        config: Arc<RwLock<AppConfig>>,
        is_paused: Arc<AtomicBool>,
        start_time: Instant,
    ) -> Result<()> {
        let (mut reader, mut writer) = stream.into_split();

        while let Ok(request) = read_message::<_, IpcRequest>(&mut reader).await {
            let response = match request {
                IpcRequest::GetEntries {
                    limit,
                    offset,
                    filter,
                    pinned_only,
                } => match repo.list(limit, offset, filter, pinned_only) {
                    Ok(entries) => IpcResponse::Entries(entries),
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::Search {
                    query,
                    limit,
                    offset,
                } => match repo.search(&query, limit, offset) {
                    Ok(entries) => IpcResponse::Entries(entries),
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::GetEntry { id } => match repo.get_by_id(&id) {
                    Ok(entry) => IpcResponse::Entry(Box::new(Some(entry))),
                    Err(_) => IpcResponse::Entry(Box::new(None)),
                },

                IpcRequest::PinEntry { id } => match repo.set_pinned(&id, true) {
                    Ok(_) => IpcResponse::Success,
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::UnpinEntry { id } => match repo.set_pinned(&id, false) {
                    Ok(_) => IpcResponse::Success,
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::DeleteEntry { id } => match repo.delete(&id) {
                    Ok(_) => IpcResponse::Success,
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::ClearHistory { include_pinned } => match repo.clear(include_pinned) {
                    Ok(_) => IpcResponse::Success,
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::GetStatus => {
                    let total = repo.count().unwrap_or(0);
                    let db_size = fs::metadata(repo.db_path()).map(|m| m.len()).unwrap_or(0);
                    let status = DaemonStatus {
                        total_entries: total,
                        is_paused: is_paused.load(Ordering::Relaxed),
                        uptime_secs: start_time.elapsed().as_secs(),
                        db_size_bytes: db_size,
                    };
                    IpcResponse::Status(status)
                }

                IpcRequest::SetPause { paused } => {
                    is_paused.store(paused, Ordering::Relaxed);
                    info!("Clipboard monitoring set to paused={}", paused);
                    IpcResponse::Success
                }

                IpcRequest::GetConfig => {
                    let cfg = config.read().await.clone();
                    IpcResponse::Config(Box::new(cfg))
                }

                IpcRequest::UpdateConfig { config: new_cfg } => {
                    let mut guard = config.write().await;
                    *guard = *new_cfg.clone();
                    let save_res = guard.save(AppConfig::config_path());
                    if let Err(e) = save_res {
                        IpcResponse::Error(e.to_string())
                    } else {
                        IpcResponse::Success
                    }
                }

                IpcRequest::GetBlob { hash } => match blob_store.read(&hash) {
                    Ok(bytes) => IpcResponse::Blob(bytes),
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::AddManualEntry { text } => {
                    let hash = BlobStore::compute_hash(text.as_bytes());
                    let entry = ClipboardEntry::new_text(
                        text,
                        hash,
                        vec!["text/plain".to_string()],
                        Some("cli".to_string()),
                    );
                    match repo.insert_or_update(&entry) {
                        Ok(_) => IpcResponse::Success,
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }

                IpcRequest::ToggleWindow
                | IpcRequest::ShowWindow
                | IpcRequest::HideWindow
                | IpcRequest::SelectAndPaste { .. } => {
                    // Window toggling requests are usually handled by GUI listener or relayed
                    IpcResponse::Success
                }
            };

            write_message(&mut writer, &response).await?;
        }

        Ok(())
    }
}
