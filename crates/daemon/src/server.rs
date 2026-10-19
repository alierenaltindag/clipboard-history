use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::{ClipboardEntry, Snippet};
use clipboard_history_core::error::Result;
use clipboard_history_core::ipc::{
    read_message, write_message, DaemonStatus, IpcRequest, IpcResponse, PendingPairingInfo,
};
use clipboard_history_core::security::CryptoEngine;
use clipboard_history_core::storage::SqliteRepository;
use clipboard_history_core::transforms::OcrEngine;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

pub struct PendingPairingItem {
    pub info: PendingPairingInfo,
    pub tx: tokio::sync::oneshot::Sender<bool>,
}

pub type PendingPairingsMap = Arc<RwLock<HashMap<String, PendingPairingItem>>>;

pub struct DaemonServer {
    repo: SqliteRepository,
    blob_store: BlobStore,
    crypto: Arc<CryptoEngine>,
    config: Arc<RwLock<AppConfig>>,
    is_paused: Arc<AtomicBool>,
    socket_path: PathBuf,
    start_time: Instant,
    peer_tracker: crate::peer_discovery::PeerTracker,
    pending_pairings: PendingPairingsMap,
    echo_tracker: crate::sync_service::SyncedEchoTracker,
    auth_token: Arc<String>,
}

impl DaemonServer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: SqliteRepository,
        blob_store: BlobStore,
        crypto: Arc<CryptoEngine>,
        config: Arc<RwLock<AppConfig>>,
        is_paused: Arc<AtomicBool>,
        socket_path: PathBuf,
        peer_tracker: crate::peer_discovery::PeerTracker,
        pending_pairings: PendingPairingsMap,
        echo_tracker: crate::sync_service::SyncedEchoTracker,
        auth_token: Arc<String>,
    ) -> Self {
        Self {
            repo,
            blob_store,
            crypto,
            config,
            is_paused,
            socket_path,
            start_time: Instant::now(),
            peer_tracker,
            pending_pairings,
            echo_tracker,
            auth_token,
        }
    }

    pub async fn run(self) -> Result<()> {
        if self.socket_path.exists() {
            if let Ok(mut stream) = tokio::net::UnixStream::connect(&self.socket_path).await {
                if let Some(token) = clipboard_history_core::ipc::IpcClient::get_auth_token() {
                    let _ =
                        write_message(&mut stream, &IpcRequest::Authenticate { auth_token: token })
                            .await;
                    let _ = read_message::<_, IpcResponse>(&mut stream).await;
                }
                if write_message(&mut stream, &IpcRequest::GetStatus)
                    .await
                    .is_ok()
                {
                    let timeout_res = tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        read_message::<_, IpcResponse>(&mut stream),
                    )
                    .await;
                    if let Ok(Ok(IpcResponse::Status(_))) = timeout_res {
                        info!("Active clipboard-history-daemon instance is already running. Exiting cleanly.");
                        std::process::exit(0);
                    }
                }
            }
            debug!("Removing stale socket at {}", self.socket_path.display());
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
        let peer_tracker = self.peer_tracker;
        let pending_pairings = self.pending_pairings;
        let echo_tracker = self.echo_tracker;
        let auth_token = self.auth_token;
        let current_uid = unsafe { libc::getuid() };

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    match stream.peer_cred() {
                        Ok(cred) if cred.uid() == current_uid => {
                            // Verify peer executable identity to block unauthorized scripts running under same UID
                            if let Some(pid) = cred.pid() {
                                if let Ok(exe_path) =
                                    std::fs::read_link(format!("/proc/{}/exe", pid))
                                {
                                    let allowed = [
                                        "clipboard-history-gui",
                                        "clipboard-history",
                                        "clipboard-history-daemon",
                                        "bwrap",
                                        "flatpak-session-helper",
                                        "snap-confine",
                                        "snap",
                                    ];
                                    let path_str = exe_path.to_string_lossy();
                                    let is_allowed = exe_path
                                        .file_name()
                                        .and_then(|f| f.to_str())
                                        .map(|name| {
                                            allowed
                                                .iter()
                                                .any(|&a| name == a || name.starts_with(a))
                                        })
                                        .unwrap_or(false)
                                        || path_str.starts_with("/app/")
                                        || path_str.starts_with("/snap/")
                                        || path_str.contains(".var/app/")
                                        || std::path::Path::new("/.flatpak-info").exists();
                                    if !is_allowed {
                                        tracing::warn!(
                                            "Rejected unauthorized process attempting to access clipboard IPC: {:?} (PID {})",
                                            exe_path,
                                            pid
                                        );
                                        continue;
                                    }
                                }
                            }
                        }
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
                    let tracker_cloned = peer_tracker.clone();
                    let pending_cloned = Arc::clone(&pending_pairings);
                    let echo_tracker_cloned = echo_tracker.clone();
                    let auth_token_cloned = Arc::clone(&auth_token);

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(
                            stream,
                            repo_cloned,
                            blob_cloned,
                            crypto_cloned,
                            config_cloned,
                            paused_cloned,
                            start_time,
                            tracker_cloned,
                            pending_cloned,
                            echo_tracker_cloned,
                            auth_token_cloned,
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
        peer_tracker: crate::peer_discovery::PeerTracker,
        pending_pairings: PendingPairingsMap,
        echo_tracker: crate::sync_service::SyncedEchoTracker,
        auth_token: Arc<String>,
    ) -> Result<()> {
        let mut authenticated = auth_token.is_empty();
        let (mut reader, mut writer) = stream.into_split();

        while let Ok(request) = read_message::<_, IpcRequest>(&mut reader).await {
            if let IpcRequest::Authenticate {
                auth_token: ref token,
            } = request
            {
                if token == auth_token.as_str() {
                    authenticated = true;
                    write_message(&mut writer, &IpcResponse::Authenticated).await?;
                } else {
                    tracing::warn!("Rejected IPC client: invalid ephemeral auth_token");
                    write_message(
                        &mut writer,
                        &IpcResponse::Error(
                            "Authentication failed: invalid session token".to_string(),
                        ),
                    )
                    .await?;
                    break;
                }
                continue;
            }

            if !authenticated {
                tracing::warn!(
                    "Rejected unauthenticated IPC request: ephemeral handshake required"
                );
                write_message(
                    &mut writer,
                    &IpcResponse::Error(
                        "Unauthenticated: ephemeral auth_token handshake required".to_string(),
                    ),
                )
                .await?;
                break;
            }
            let response = match request {
                IpcRequest::GetEntries {
                    limit,
                    offset,
                    filter,
                    pinned_only,
                } => match repo.list(limit, offset, filter, pinned_only) {
                    Ok(mut entries) => {
                        for e in &mut entries {
                            decrypt_entry(e, &repo, &crypto);
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
                            decrypt_entry(e, &repo, &crypto);
                        }
                        IpcResponse::Entries(entries)
                    }
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::GetEntry { id } => match repo.get_by_id(&id) {
                    Ok(mut entry) => {
                        decrypt_entry(&mut entry, &repo, &crypto);
                        IpcResponse::Entry(Box::new(Some(entry)))
                    }
                    Err(_) => IpcResponse::Entry(Box::new(None)),
                },

                IpcRequest::PinEntry { id } => {
                    let hash_opt = repo.get_by_id(&id).ok().map(|e| e.content_hash);
                    match repo.set_pinned(&id, true) {
                        Ok(_) => {
                            if let Some(content_hash) = hash_opt {
                                let sync_cfg = config.read().await.sync.clone();
                                if sync_cfg.enabled {
                                    let active_peers = peer_tracker.get_all_peers().await;
                                    let msg = clipboard_history_core::sync::SyncMessage::PinSync {
                                        origin_device: sync_cfg.device_name.clone(),
                                        content_hash,
                                        is_pinned: true,
                                    };
                                    crate::sync_service::broadcast_sync_message(
                                        &sync_cfg,
                                        &active_peers,
                                        msg,
                                    );
                                }
                            }
                            IpcResponse::Success
                        }
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }

                IpcRequest::UnpinEntry { id } => {
                    let hash_opt = repo.get_by_id(&id).ok().map(|e| e.content_hash);
                    match repo.set_pinned(&id, false) {
                        Ok(_) => {
                            if let Some(content_hash) = hash_opt {
                                let sync_cfg = config.read().await.sync.clone();
                                if sync_cfg.enabled {
                                    let active_peers = peer_tracker.get_all_peers().await;
                                    let msg = clipboard_history_core::sync::SyncMessage::PinSync {
                                        origin_device: sync_cfg.device_name.clone(),
                                        content_hash,
                                        is_pinned: false,
                                    };
                                    crate::sync_service::broadcast_sync_message(
                                        &sync_cfg,
                                        &active_peers,
                                        msg,
                                    );
                                }
                            }
                            IpcResponse::Success
                        }
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }

                IpcRequest::DeleteEntry { id } => match repo.delete(&id) {
                    Ok(_) => IpcResponse::Success,
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::ClearHistory { include_pinned } => match repo.clear(include_pinned) {
                    Ok(_) => {
                        if let Ok(active_hashes) = repo.get_all_blob_hashes() {
                            let _ = blob_store.cleanup_orphans(&active_hashes);
                        }

                        // If Network Sync is enabled, broadcast ClearHistorySync to all paired devices
                        let sync_cfg = config.read().await.sync.clone();
                        if sync_cfg.enabled {
                            let active_peers = peer_tracker.get_all_peers().await;
                            let clear_msg =
                                clipboard_history_core::sync::SyncMessage::ClearHistorySync {
                                    origin_device: sync_cfg.device_name.clone(),
                                    include_pinned,
                                };
                            crate::sync_service::broadcast_sync_message(
                                &sync_cfg,
                                &active_peers,
                                clear_msg,
                            );
                        }

                        IpcResponse::Success
                    }
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

                IpcRequest::GetSyncPeers => {
                    let peers = peer_tracker.get_peer_descriptions().await;
                    IpcResponse::SyncPeers(peers)
                }

                IpcRequest::TriggerSyncScan => {
                    peer_tracker.trigger_scan().await;

                    let trusted = config.read().await.sync.trusted_peers.clone();
                    let my_dev = config.read().await.sync.device_name.clone();
                    let my_pin = config.read().await.sync.pairing_pin.clone();
                    let (enc_enabled, max_entries) = {
                        let c = config.read().await;
                        (c.security.encryption_enabled, c.general.max_entries)
                    };
                    let repo_clone = repo.clone();
                    let blob_clone = blob_store.clone();
                    let crypto_clone = crypto.clone();
                    let echo_tracker_clone = echo_tracker.clone();

                    tokio::spawn(async move {
                        for peer in trusted {
                            if peer.contains('.') || peer.contains(':') {
                                let _ = crate::sync_service::perform_mutual_history_sync(
                                    &peer,
                                    &my_pin,
                                    &my_dev,
                                    repo_clone.clone(),
                                    blob_clone.clone(),
                                    crypto_clone.clone(),
                                    enc_enabled,
                                    max_entries,
                                    echo_tracker_clone.clone(),
                                )
                                .await;
                            }
                        }
                    });

                    IpcResponse::Success
                }

                IpcRequest::GetTrustedPeers => {
                    let peers = config.read().await.sync.trusted_peers.clone();
                    IpcResponse::TrustedPeers(peers)
                }

                IpcRequest::GetPendingPairingRequests => {
                    let map = pending_pairings.read().await;
                    let list = map.values().map(|v| v.info.clone()).collect();
                    IpcResponse::PendingPairings(list)
                }

                IpcRequest::RespondPairingRequest { request_id, accept } => {
                    let mut map = pending_pairings.write().await;
                    if let Some(item) = map.remove(&request_id) {
                        let _ = item.tx.send(accept);
                    }
                    IpcResponse::Success
                }

                IpcRequest::UnpairPeer { peer_identifier } => {
                    let clean_id = if let Some(idx) = peer_identifier.find(':') {
                        peer_identifier[..idx].to_string()
                    } else {
                        peer_identifier.clone()
                    };

                    let all_peers = peer_tracker.get_all_peers().await;
                    let mut matching_names = Vec::new();
                    let mut matching_addrs = Vec::new();

                    for (addr, name) in &all_peers {
                        let clean_a = if let Some(idx) = addr.find(':') {
                            &addr[..idx]
                        } else {
                            addr.as_str()
                        };
                        if addr == &peer_identifier
                            || clean_a == clean_id
                            || name == &peer_identifier
                            || (!clean_id.is_empty() && name == &clean_id)
                        {
                            matching_names.push(name.clone());
                            matching_addrs.push(addr.clone());
                            matching_addrs.push(clean_a.to_string());
                        }
                    }

                    let mut cfg = config.write().await;
                    cfg.sync.trusted_peers.retain(|p| {
                        let clean_p = if let Some(idx) = p.find(':') {
                            &p[..idx]
                        } else {
                            p.as_str()
                        };
                        p != &peer_identifier
                            && clean_p != clean_id
                            && !clean_p.starts_with(&clean_id)
                            && !matching_names.contains(p)
                            && !matching_addrs.contains(p)
                    });
                    cfg.sync.peer_addresses.retain(|p| {
                        let clean_p = if let Some(idx) = p.find(':') {
                            &p[..idx]
                        } else {
                            p.as_str()
                        };
                        p != &peer_identifier
                            && clean_p != clean_id
                            && !clean_p.starts_with(&clean_id)
                            && !matching_names.contains(p)
                            && !matching_addrs.contains(p)
                    });
                    let _ = cfg.save(AppConfig::config_path());
                    let my_dev = cfg.sync.device_name.clone();
                    drop(cfg);

                    let mut candidate_addrs = Vec::new();
                    if peer_identifier.contains(':') {
                        let parts: Vec<&str> = peer_identifier.split(':').collect();
                        let ip = parts[0];
                        let port: u16 = parts[1].parse().unwrap_or(54123);
                        let alt_port = if port == 54123 { 54124 } else { 54123 };
                        candidate_addrs.push(peer_identifier.clone());
                        candidate_addrs.push(format!("{}:{}", ip, alt_port));
                    } else if peer_identifier.contains('.') {
                        candidate_addrs.push(format!("{}:54123", peer_identifier));
                        candidate_addrs.push(format!("{}:54124", peer_identifier));
                    }

                    for a in matching_addrs {
                        if a.contains(':') {
                            let (ip, alt_port) = {
                                let parts: Vec<&str> = a.split(':').collect();
                                let ip = parts[0].to_string();
                                let port: u16 = parts[1].parse().unwrap_or(54123);
                                let alt = if port == 54123 { 54124 } else { 54123 };
                                (ip, alt)
                            };
                            let alt = format!("{}:{}", ip, alt_port);
                            if !candidate_addrs.contains(&a) {
                                candidate_addrs.push(a);
                            }
                            if !candidate_addrs.contains(&alt) {
                                candidate_addrs.push(alt);
                            }
                        } else if a.contains('.') {
                            let p1 = format!("{}:54123", a);
                            let p2 = format!("{}:54124", a);
                            if !candidate_addrs.contains(&p1) {
                                candidate_addrs.push(p1);
                            }
                            if !candidate_addrs.contains(&p2) {
                                candidate_addrs.push(p2);
                            }
                        }
                    }

                    tokio::spawn(async move {
                        use clipboard_history_core::sync::{read_sync_message, send_sync_message, SyncMessage};
                        use std::time::Duration;

                        for target_addr in candidate_addrs {
                            if let Ok(Ok(mut stream)) = tokio::time::timeout(
                                Duration::from_secs(3),
                                tokio::net::TcpStream::connect(&target_addr),
                            )
                            .await
                            {
                                let (mut reader, mut writer) = stream.split();
                                let unpair_req = SyncMessage::UnpairRequest {
                                    origin_device: my_dev.clone(),
                                };
                                if send_sync_message(&mut writer, "CH_PAIRING_V1", &unpair_req).await.is_ok() {
                                    let _ = tokio::time::timeout(
                                        Duration::from_secs(3),
                                        read_sync_message(&mut reader, "CH_PAIRING_V1"),
                                    )
                                    .await;
                                    break;
                                }
                            }
                        }
                    });

                    IpcResponse::Success
                }

                IpcRequest::SendPairingRequest { peer_address } => {
                    use clipboard_history_core::sync::{
                        read_sync_message, send_sync_message, SyncMessage,
                    };
                    use std::time::Duration;

                    let current = config.read().await.sync.clone();
                    let my_dev = current.device_name.clone();
                    let my_pin = current.pairing_pin.clone();
                    let request_id = format!("{:x}", rand::random::<u128>());

                    let candidate_addrs = if peer_address.contains(':') {
                        let parts: Vec<&str> = peer_address.split(':').collect();
                        let ip = parts[0];
                        let port: u16 = parts[1].parse().unwrap_or(54123);
                        let alt_port = if port == 54123 { 54124 } else { 54123 };
                        vec![peer_address.clone(), format!("{}:{}", ip, alt_port)]
                    } else {
                        vec![
                            format!("{}:54123", peer_address),
                            format!("{}:54124", peer_address),
                        ]
                    };

                    let mut connected_stream = None;
                    for addr in &candidate_addrs {
                        if let Ok(Ok(stream)) = tokio::time::timeout(
                            Duration::from_secs(4),
                            tokio::net::TcpStream::connect(addr),
                        )
                        .await
                        {
                            connected_stream = Some(stream);
                            break;
                        }
                    }

                    if let Some(mut stream) = connected_stream {
                        let (mut reader, mut writer) = stream.split();
                        let pair_req = SyncMessage::PairingRequest {
                            origin_device: my_dev.clone(),
                            pairing_pin: my_pin.clone(),
                            request_id: request_id.clone(),
                        };

                        if let Err(e) =
                            send_sync_message(&mut writer, "CH_PAIRING_V1", &pair_req).await
                        {
                            IpcResponse::PairingResult {
                                success: false,
                                message: format!("Failed to send pairing request: {}", e),
                                peer_address: peer_address.clone(),
                                device_name: String::new(),
                            }
                        } else {
                            // Wait up to 45 seconds for peer's user to accept/reject
                            let response_fut = read_sync_message(&mut reader, "CH_PAIRING_V1");
                            match tokio::time::timeout(Duration::from_secs(45), response_fut).await
                            {
                                Ok(Ok(SyncMessage::PairingResponse {
                                    accepted: true,
                                    device_name,
                                    ..
                                })) => {
                                    let mut cfg = config.write().await;
                                    let clean_ip = if let Some(idx) = peer_address.find(':') {
                                        peer_address[..idx].to_string()
                                    } else {
                                        peer_address.clone()
                                    };
                                    if !cfg.sync.trusted_peers.contains(&clean_ip) {
                                        cfg.sync.trusted_peers.push(clean_ip);
                                    }
                                    if !device_name.is_empty()
                                        && !cfg.sync.trusted_peers.contains(&device_name)
                                    {
                                        cfg.sync.trusted_peers.push(device_name.clone());
                                    }
                                    if !cfg.sync.peer_addresses.contains(&peer_address) {
                                        cfg.sync.peer_addresses.push(peer_address.clone());
                                    }
                                    cfg.sync.enabled = true;
                                    let _ = cfg.save(AppConfig::config_path());
                                    drop(cfg);

                                    // Trigger immediate mutual history sync with newly paired peer
                                    let repo_sync = repo.clone();
                                    let blob_sync = blob_store.clone();
                                    let crypto_sync = crypto.clone();
                                    let peer_sync = peer_address.clone();
                                    let my_dev_sync = my_dev.clone();
                                    let my_pin_sync = my_pin.clone();
                                    let (enc_enabled, max_entries) = {
                                        let c = config.read().await;
                                        (c.security.encryption_enabled, c.general.max_entries)
                                    };
                                    let echo_tracker_sync = echo_tracker.clone();

                                    tokio::spawn(async move {
                                         crate::sync_service::perform_mutual_history_sync(
                                             &peer_sync,
                                             &my_pin_sync,
                                             &my_dev_sync,
                                             repo_sync,
                                             blob_sync,
                                             crypto_sync,
                                             enc_enabled,
                                             max_entries,
                                             echo_tracker_sync,
                                         )
                                         .await;
                                    });

                                    IpcResponse::PairingResult {
                                        success: true,
                                        message: format!(
                                            "Successfully paired with '{}'!",
                                            device_name
                                        ),
                                        peer_address: peer_address.clone(),
                                        device_name,
                                    }
                                }
                                Ok(Ok(SyncMessage::PairingResponse {
                                    accepted: false,
                                    message,
                                    ..
                                })) => IpcResponse::PairingResult {
                                    success: false,
                                    message: format!("Pairing rejected: {}", message),
                                    peer_address: peer_address.clone(),
                                    device_name: String::new(),
                                },
                                Ok(Err(e)) => IpcResponse::PairingResult {
                                    success: false,
                                    message: format!(
                                        "Protocol error while receiving response: {}",
                                        e
                                    ),
                                    peer_address: peer_address.clone(),
                                    device_name: String::new(),
                                },
                                Err(_) => IpcResponse::PairingResult {
                                    success: false,
                                    message: "No response received from remote device (timeout). Ensure the device is powered on and connected to the network.".to_string(),
                                    peer_address: peer_address.clone(),
                                    device_name: String::new(),
                                },
                                _ => IpcResponse::PairingResult {
                                    success: false,
                                    message: "Unknown response type received.".to_string(),
                                    peer_address: peer_address.clone(),
                                    device_name: String::new(),
                                },
                            }
                        }
                    } else {
                        IpcResponse::PairingResult {
                            success: false,
                            message: format!(
                                "Failed to connect to target device ({}). Ensure the device is powered on and on the same network.",
                                peer_address
                            ),
                            peer_address: peer_address.clone(),
                            device_name: String::new(),
                        }
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

                IpcRequest::CheckFirewall => {
                    let result = clipboard_history_core::check_firewall_status();
                    IpcResponse::FirewallStatus(result)
                }

                IpcRequest::PerformOcr { blob_hash } => {
                    let is_encrypted = config.read().await.security.encryption_enabled;
                    let image_res = if is_encrypted {
                        blob_store
                            .read_decrypted(&blob_hash, &crypto)
                            .or_else(|_| blob_store.read(&blob_hash))
                    } else {
                        blob_store.read(&blob_hash)
                    };

                    match image_res {
                        Ok(image_bytes) => match OcrEngine::extract_text(&image_bytes).await {
                            Ok(extracted_text) => {
                                if !extracted_text.trim().is_empty() {
                                    let hash = BlobStore::compute_hash(extracted_text.as_bytes());
                                    let mut entry = ClipboardEntry::new_text(
                                        extracted_text.clone(),
                                        hash,
                                        vec!["text/plain".to_string()],
                                        Some("OCR".to_string()),
                                    );

                                    if is_encrypted {
                                        if let Ok(wrapped_dek) =
                                            crypto.encrypt_entry_envelope(&mut entry)
                                        {
                                            let _ =
                                                repo.insert_entry_dek(&entry.id, &wrapped_dek, &[]);
                                        } else {
                                            entry.preview = crypto
                                                .encrypt_str(&entry.preview)
                                                .unwrap_or(entry.preview);
                                            entry.text_content = entry
                                                .text_content
                                                .and_then(|t| crypto.encrypt_str(&t).ok());
                                            entry.html_content = entry
                                                .html_content
                                                .and_then(|h| crypto.encrypt_str(&h).ok());
                                        }
                                    }

                                    let _ = repo.insert_or_update(&entry);
                                }
                                IpcResponse::OcrResult {
                                    text: extracted_text,
                                }
                            }
                            Err(e) => IpcResponse::Error(e.to_string()),
                        },
                        Err(e) => IpcResponse::Error(format!("Failed to read image blob: {}", e)),
                    }
                }

                IpcRequest::BatchDelete { ids } => match repo.batch_delete(&ids) {
                    Ok(count) => IpcResponse::BatchSuccess { count },
                    Err(e) => IpcResponse::Error(e.to_string()),
                },

                IpcRequest::BatchPin { ids, pinned } => {
                    let mut hashes = Vec::new();
                    for id in &ids {
                        if let Ok(entry) = repo.get_by_id(id) {
                            hashes.push(entry.content_hash);
                        }
                    }
                    match repo.batch_set_pinned(&ids, pinned) {
                        Ok(count) => {
                            let sync_cfg = config.read().await.sync.clone();
                            if sync_cfg.enabled && !hashes.is_empty() {
                                let active_peers = peer_tracker.get_all_peers().await;
                                for content_hash in hashes {
                                    let msg = clipboard_history_core::sync::SyncMessage::PinSync {
                                        origin_device: sync_cfg.device_name.clone(),
                                        content_hash,
                                        is_pinned: pinned,
                                    };
                                    crate::sync_service::broadcast_sync_message(
                                        &sync_cfg,
                                        &active_peers,
                                        msg,
                                    );
                                }
                            }
                            IpcResponse::BatchSuccess { count }
                        }
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                },

                IpcRequest::ComputeDiff { id_a, id_b } => {
                    let text_a = match repo.get_by_id(&id_a) {
                        Ok(mut entry) => {
                            decrypt_entry(&mut entry, &repo, &crypto);
                            entry.text_content.unwrap_or(entry.preview)
                        }
                        Err(_) => String::new(),
                    };
                    let text_b = match repo.get_by_id(&id_b) {
                        Ok(mut entry) => {
                            decrypt_entry(&mut entry, &repo, &crypto);
                            entry.text_content.unwrap_or(entry.preview)
                        }
                        Err(_) => String::new(),
                    };
                    let diff_res = clipboard_history_core::transforms::DiffEngine::compute_diff(
                        &text_a, &text_b,
                    );
                    IpcResponse::DiffResult(Box::new(diff_res))
                }

                IpcRequest::PreparePaste { content_hash } => {
                    echo_tracker.record_echo(content_hash).await;
                    IpcResponse::Success
                }

                IpcRequest::TouchEntry { id } => {
                    let _ = repo.touch_entry(&id);
                    IpcResponse::Success
                }

                IpcRequest::ToggleWindow => {
                    info!("Received ToggleWindow request. Spawning GUI with --toggle...");
                    let bin = crate::tray::get_executable_path("clipboard-history-gui");
                    match std::process::Command::new(bin).arg("--toggle").spawn() {
                        Ok(_) => IpcResponse::Success,
                        Err(e) => IpcResponse::Error(format!("Failed to spawn GUI: {}", e)),
                    }
                }

                IpcRequest::ShowWindow => {
                    info!("Received ShowWindow request. Spawning GUI...");
                    let bin = crate::tray::get_executable_path("clipboard-history-gui");
                    match std::process::Command::new(bin).spawn() {
                        Ok(_) => IpcResponse::Success,
                        Err(e) => IpcResponse::Error(format!("Failed to spawn GUI: {}", e)),
                    }
                }

                IpcRequest::HideWindow | IpcRequest::SelectAndPaste { .. } => IpcResponse::Success,
                IpcRequest::Authenticate { .. } => IpcResponse::Authenticated,
            };

            write_message(&mut writer, &response).await?;
        }

        Ok(())
    }
}

fn decrypt_entry(entry: &mut ClipboardEntry, repo: &SqliteRepository, crypto: &CryptoEngine) {
    if let Ok(Some((wrapped_dek, _))) = repo.get_entry_dek(&entry.id) {
        if crypto.decrypt_entry_envelope(entry, &wrapped_dek).is_ok() {
            return;
        }
    }
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
