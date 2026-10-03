use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::{ClipboardEntry, Snippet};
use clipboard_history_core::error::Result;
use clipboard_history_core::ipc::{
    read_message, write_message, DaemonStatus, IpcRequest, IpcResponse, QueueStatus,
};
use clipboard_history_core::security::CryptoEngine;
use clipboard_history_core::storage::SqliteRepository;
use clipboard_history_core::transforms::OcrEngine;
use std::collections::VecDeque;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

pub const MAX_PASTE_QUEUE_SIZE: usize = 100;

pub struct DaemonServer {
    repo: SqliteRepository,
    blob_store: BlobStore,
    crypto: Arc<CryptoEngine>,
    config: Arc<RwLock<AppConfig>>,
    is_paused: Arc<AtomicBool>,
    socket_path: PathBuf,
    start_time: Instant,
    paste_queue: Arc<Mutex<VecDeque<ClipboardEntry>>>,
}

impl DaemonServer {
    pub fn new(
        repo: SqliteRepository,
        blob_store: BlobStore,
        crypto: Arc<CryptoEngine>,
        config: Arc<RwLock<AppConfig>>,
        is_paused: Arc<AtomicBool>,
        socket_path: PathBuf,
    ) -> Self {
        Self {
            repo,
            blob_store,
            crypto,
            config,
            is_paused,
            socket_path,
            start_time: Instant::now(),
            paste_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub async fn run(self) -> Result<()> {
        if self.socket_path.exists() {
            debug!("Removing existing socket at {}", self.socket_path.display());
            let _ = fs::remove_file(&self.socket_path);
        }

        if let Some(parent) = self.socket_path.parent() {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }

        #[cfg(unix)]
        let old_umask = unsafe { libc::umask(0o077) };
        let listener_res = UnixListener::bind(&self.socket_path);
        #[cfg(unix)]
        unsafe {
            libc::umask(old_umask)
        };
        let listener = listener_res?;
        let _ = fs::set_permissions(&self.socket_path, fs::Permissions::from_mode(0o600));

        info!("IPC daemon listening on {}", self.socket_path.display());

        let repo = self.repo;
        let blob_store = self.blob_store;
        let crypto = self.crypto;
        let config = self.config;
        let is_paused = self.is_paused;
        let start_time = self.start_time;
        let paste_queue = self.paste_queue;
        let current_uid = unsafe { libc::getuid() };

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    match stream.peer_cred() {
                        Ok(cred) if cred.uid() == current_uid => {}
                        Ok(cred) => {
                            tracing::warn!(
                                "Rejected unauthorized IPC connection from UID {} (expected UID {})",
                                cred.uid(),
                                current_uid
                            );
                            continue;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to query IPC peer credentials: {}", e);
                            continue;
                        }
                    }

                    let repo_cloned = repo.clone();
                    let blob_cloned = blob_store.clone();
                    let crypto_cloned = Arc::clone(&crypto);
                    let config_cloned = Arc::clone(&config);
                    let paused_cloned = Arc::clone(&is_paused);
                    let queue_cloned = Arc::clone(&paste_queue);

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(
                            stream,
                            repo_cloned,
                            blob_cloned,
                            crypto_cloned,
                            config_cloned,
                            paused_cloned,
                            start_time,
                            queue_cloned,
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

    #[allow(clippy::too_many_arguments)]
    async fn handle_client(
        stream: UnixStream,
        repo: SqliteRepository,
        blob_store: BlobStore,
        crypto: Arc<CryptoEngine>,
        config: Arc<RwLock<AppConfig>>,
        is_paused: Arc<AtomicBool>,
        start_time: Instant,
        paste_queue: Arc<Mutex<VecDeque<ClipboardEntry>>>,
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
                    Ok(mut entries) => {
                        for e in &mut entries {
                            decrypt_entry(e, &crypto);
                        }
                        IpcResponse::Entries(entries)
                    }
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::Search {
                    query,
                    limit,
                    offset,
                } => match repo.search(&query, limit, offset) {
                    Ok(mut entries) => {
                        for e in &mut entries {
                            decrypt_entry(e, &crypto);
                        }
                        IpcResponse::Entries(entries)
                    }
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::GetEntry { id } => match repo.get_by_id(&id) {
                    Ok(mut entry) => {
                        decrypt_entry(&mut entry, &crypto);
                        IpcResponse::Entry(Box::new(Some(entry)))
                    }
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

                IpcRequest::GetBlob { hash } => {
                    let res = if config.read().await.security.encryption_enabled {
                        blob_store
                            .read_decrypted(&hash, &crypto)
                            .or_else(|_| blob_store.read(&hash))
                    } else {
                        blob_store.read(&hash)
                    };
                    match res {
                        Ok(bytes) => IpcResponse::Blob(bytes),
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }

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

                IpcRequest::ListSnippets { category } => {
                    match repo.list_snippets(category.as_deref()) {
                        Ok(snippets) => IpcResponse::Snippets(snippets),
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }

                IpcRequest::CreateSnippet {
                    label,
                    content,
                    category,
                } => {
                    let snippet = Snippet::new(label, content, category);
                    match repo.insert_snippet(&snippet) {
                        Ok(s) => IpcResponse::Snippet(Box::new(s)),
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }

                IpcRequest::UpdateSnippet { snippet } => match repo.update_snippet(&snippet) {
                    Ok(_) => IpcResponse::Success,
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::DeleteSnippet { id } => match repo.delete_snippet(&id) {
                    Ok(_) => IpcResponse::Success,
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::UseSnippet { id } => {
                    let _ = repo.touch_snippet(&id);
                    IpcResponse::Success
                }

                IpcRequest::EnqueueItems { ids } => {
                    let mut q = paste_queue.lock().unwrap_or_else(|p| p.into_inner());
                    for id in ids {
                        if let Ok(mut entry) = repo.get_by_id(&id) {
                            decrypt_entry(&mut entry, &crypto);
                            if q.len() >= MAX_PASTE_QUEUE_SIZE {
                                q.pop_front();
                            }
                            q.push_back(entry);
                        }
                    }
                    info!(
                        "Enqueued items into sequential paste queue. Size: {}",
                        q.len()
                    );
                    IpcResponse::Success
                }

                IpcRequest::ClearQueue => {
                    let mut q = paste_queue.lock().unwrap_or_else(|p| p.into_inner());
                    q.clear();
                    info!("Cleared sequential paste queue");
                    IpcResponse::Success
                }

                IpcRequest::GetQueueStatus => {
                    let q = paste_queue.lock().unwrap_or_else(|p| p.into_inner());
                    let remaining_count = q.len();
                    let active = !q.is_empty();
                    let next_preview = q.front().map(|e| e.preview.clone());
                    IpcResponse::QueueStatus(QueueStatus {
                        active,
                        remaining_count,
                        next_preview,
                    })
                }

                IpcRequest::PopAndPasteQueue => {
                    let mut q = paste_queue.lock().unwrap_or_else(|p| p.into_inner());
                    if let Some(entry) = q.pop_front() {
                        let remaining_count = q.len();
                        let text = entry.text_content.unwrap_or(entry.preview);
                        info!(
                            "Popped item from sequential paste queue. Remaining: {}",
                            remaining_count
                        );
                        let hash = BlobStore::compute_hash(text.as_bytes());
                        let popped_entry = ClipboardEntry::new_text(
                            text.clone(),
                            hash,
                            vec!["text/plain".to_string()],
                            Some("PasteQueue".to_string()),
                        );
                        let _ = repo.insert_or_update(&popped_entry);
                        IpcResponse::QueuePopped {
                            remaining_count,
                            pasted: true,
                            text: Some(text),
                        }
                    } else {
                        IpcResponse::QueuePopped {
                            remaining_count: 0,
                            pasted: false,
                            text: None,
                        }
                    }
                }

                IpcRequest::PerformOcr { blob_hash } => match blob_store.read(&blob_hash) {
                    Ok(image_bytes) => match OcrEngine::extract_text(&image_bytes).await {
                        Ok(extracted_text) => {
                            if !extracted_text.trim().is_empty() {
                                let hash = BlobStore::compute_hash(extracted_text.as_bytes());
                                let entry = ClipboardEntry::new_text(
                                    extracted_text.clone(),
                                    hash,
                                    vec!["text/plain".to_string()],
                                    Some("OCR".to_string()),
                                );
                                let _ = repo.insert_or_update(&entry);
                            }
                            IpcResponse::OcrResult {
                                text: extracted_text,
                            }
                        }
                        Err(e) => IpcResponse::Error(e.to_string()),
                    },
                    Err(e) => IpcResponse::Error(format!("Failed to read image blob: {}", e)),
                },

                IpcRequest::BatchDelete { ids } => match repo.batch_delete(&ids) {
                    Ok(count) => IpcResponse::BatchSuccess { count },
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::BatchPin { ids, pinned } => match repo.batch_set_pinned(&ids, pinned) {
                    Ok(count) => IpcResponse::BatchSuccess { count },
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::ComputeDiff { id_a, id_b } => {
                    let text_a = match repo.get_by_id(&id_a) {
                        Ok(entry) => entry.text_content.unwrap_or(entry.preview),
                        Err(_) => String::new(),
                    };
                    let text_b = match repo.get_by_id(&id_b) {
                        Ok(entry) => entry.text_content.unwrap_or(entry.preview),
                        Err(_) => String::new(),
                    };
                    let diff_res = clipboard_history_core::transforms::DiffEngine::compute_diff(
                        &text_a, &text_b,
                    );
                    IpcResponse::DiffResult(Box::new(diff_res))
                }

                IpcRequest::ToggleWindow => {
                    info!("Received ToggleWindow request. Spawning GUI with --toggle...");
                    match std::process::Command::new("clipboard-history-gui")
                        .arg("--toggle")
                        .spawn()
                    {
                        Ok(_) => IpcResponse::Success,
                        Err(e) => IpcResponse::Error(format!("Failed to spawn GUI: {}", e)),
                    }
                }

                IpcRequest::ShowWindow => {
                    info!("Received ShowWindow request. Spawning GUI...");
                    match std::process::Command::new("clipboard-history-gui").spawn() {
                        Ok(_) => IpcResponse::Success,
                        Err(e) => IpcResponse::Error(format!("Failed to spawn GUI: {}", e)),
                    }
                }

                IpcRequest::HideWindow | IpcRequest::SelectAndPaste { .. } => IpcResponse::Success,
            };

            write_message(&mut writer, &response).await?;
        }

        Ok(())
    }
}

fn decrypt_entry(entry: &mut ClipboardEntry, crypto: &CryptoEngine) {
    if let Some(ref text) = entry.text_content {
        if let Ok(dec) = crypto.decrypt_str(text) {
            entry.text_content = Some(dec);
        }
    }
    if let Some(ref html) = entry.html_content {
        if let Ok(dec) = crypto.decrypt_str(html) {
            entry.html_content = Some(dec);
        }
    }
    if let Ok(dec_preview) = crypto.decrypt_str(&entry.preview) {
        entry.preview = dec_preview;
    }
}
