mod circuit_breaker;
mod eviction;
mod peer_discovery;
mod server;
pub mod sync_service;
mod tray;
mod watcher;
mod window_focus;

use circuit_breaker::CircuitBreaker;
use eviction::EvictionWorker;
use peer_discovery::{start_udp_discovery, PeerTracker};
use server::DaemonServer;
use watcher::traits::{ClipboardWatcher, RawClipboardEvent};
use watcher::{WaylandWatcher, X11Watcher};
use window_focus::WindowFocusDetector;

use clap::Parser;
use clipboard_history_core::blob::{BlobStore, ThumbnailGenerator};
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::security::{
    CryptoEngine, PasswordManagerGuard, SecretFilter, SecretHandlingPolicy,
};
use clipboard_history_core::storage::SqliteRepository;
use clipboard_history_core::transforms::UrlCleaner;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "High-performance background clipboard watcher daemon"
)]
struct Cli {
    #[arg(short, long, help = "Custom configuration file path")]
    config: Option<PathBuf>,

    #[arg(short, long, help = "Run in verbose debug logging mode")]
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    clipboard_history_core::security::apply_process_memory_shield();
    clipboard_history_core::security::lock_process_memory();

    // High performance constrained Tokio runtime to keep idle RSS < 15MB
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(1024 * 1024)
        .enable_all()
        .build()?;

    runtime.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let env_filter = if cli.verbose {
        EnvFilter::new("debug,clipboard_history=trace")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    info!(
        "Starting clipboard-history-daemon v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Early Single-Instance Guard: Probe existing socket before initializing DB or watchers
    let socket_path = AppConfig::socket_path();
    if socket_path.exists() {
        if let Ok(mut stream) = tokio::net::UnixStream::connect(&socket_path).await {
            if let Some(token) = clipboard_history_core::ipc::IpcClient::get_auth_token() {
                let _ = clipboard_history_core::ipc::write_message(
                    &mut stream,
                    &clipboard_history_core::ipc::IpcRequest::Authenticate { auth_token: token },
                )
                .await;
                let _ = clipboard_history_core::ipc::read_message::<
                    _,
                    clipboard_history_core::ipc::IpcResponse,
                >(&mut stream)
                .await;
            }

            if clipboard_history_core::ipc::write_message(
                &mut stream,
                &clipboard_history_core::ipc::IpcRequest::GetStatus,
            )
            .await
            .is_ok()
            {
                let timeout_res = tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    clipboard_history_core::ipc::read_message::<
                        _,
                        clipboard_history_core::ipc::IpcResponse,
                    >(&mut stream),
                )
                .await;
                if let Ok(Ok(clipboard_history_core::ipc::IpcResponse::Status(_))) = timeout_res {
                    info!("Active clipboard-history-daemon instance is already running. Exiting cleanly.");
                    std::process::exit(0);
                }
            }
        }
        debug!("Removing stale socket at {}", socket_path.display());
        let _ = std::fs::remove_file(&socket_path);
    }

    // Generate ephemeral 256-bit session token for IPC authentication
    let auth_token_str = format!(
        "{:032x}{:032x}",
        rand::random::<u128>(),
        rand::random::<u128>()
    );
    let auth_token = Arc::new(auth_token_str);
    clipboard_history_core::security::protect_sensitive_buffer(auth_token.as_bytes());

    let token_path = AppConfig::auth_token_path();
    if let Some(parent) = token_path.parent() {
        let _ = std::fs::create_dir_all(parent);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&token_path)
        {
            use std::io::Write;
            let _ = file.write_all(auth_token.as_bytes());
        }
    }

    let config = if let Some(path) = cli.config {
        let content = std::fs::read_to_string(&path)?;
        toml::from_str::<AppConfig>(&content)?
    } else {
        AppConfig::load_or_default()
    };

    let config_arc = Arc::new(RwLock::new(config.clone()));
    let is_paused = Arc::new(AtomicBool::new(false));

    // Initialize Storage
    let db_path = AppConfig::db_path();
    let repo = SqliteRepository::open(&db_path)?;

    // Initialize BlobStore
    let blobs_dir = AppConfig::blobs_dir();
    let blob_store = BlobStore::new(&blobs_dir)?;

    // Initialize Security Filter
    let secret_filter = SecretFilter::new(&config.security.custom_secret_patterns);

    // Initialize Circuit Breaker
    let mut circuit_breaker = CircuitBreaker::new(
        config.general.rate_limit_per_second,
        config.general.max_blob_size_mb,
    );

    // Spawn Background Eviction & Retention Worker
    let eviction_worker =
        EvictionWorker::new(repo.clone(), blob_store.clone(), Arc::clone(&config_arc));
    tokio::spawn(async move {
        eviction_worker.run_loop().await;
    });

    // Initialize Crypto Engine
    let crypto = Arc::new(
        CryptoEngine::load_or_create(AppConfig::secret_key_path())
            .expect("Failed to initialize or load CryptoEngine"),
    );

    let peer_tracker = PeerTracker::new();
    let pending_pairings: server::PendingPairingsMap =
        Arc::new(RwLock::new(std::collections::HashMap::new()));
    let echo_tracker = sync_service::SyncedEchoTracker::new();

    // Ensure display server environment variables are populated (especially for systemd units)
    clipboard_history_core::clipboard::ensure_display_environment();

    // Spawn IPC Socket Server
    let server = DaemonServer::new(
        repo.clone(),
        blob_store.clone(),
        Arc::clone(&crypto),
        Arc::clone(&config_arc),
        Arc::clone(&is_paused),
        socket_path.clone(),
        peer_tracker.clone(),
        pending_pairings.clone(),
        echo_tracker.clone(),
        auth_token.clone(),
    );
    tokio::spawn(async move {
        if let Err(e) = server.run().await {
            error!("IPC server error: {}", e);
        }
    });

    // Detect Display Server
    let session_type = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_lowercase();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();

    let is_wayland = session_type == "wayland"
        || wayland_display.is_some()
        || (which::which("wl-paste").is_ok()
            && (session_type.is_empty() || session_type == "wayland"));

    let is_x11 = std::env::var("DISPLAY").is_ok() || !is_wayland;

    info!(
        "Detected display environment: Wayland={}, X11={} (session: {}, wayland_display: {:?})",
        is_wayland, is_x11, session_type, wayland_display
    );

    let (event_tx, mut event_rx) = mpsc::channel::<RawClipboardEvent>(64);
    let mut spawned_any = false;

    // 1. Spawn Wayland watcher if Wayland is available
    if is_wayland && which::which("wl-paste").is_ok() {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut watcher = WaylandWatcher::new();
            if let Err(e) = watcher.run(tx).await {
                error!("Wayland watcher exited: {}", e);
            }
        });
        spawned_any = true;
    }

    // 2. Spawn X11 watcher if X11 / Xwayland is available
    if is_x11 {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut watcher = X11Watcher::new();
            if let Err(e) = watcher.run(tx).await {
                debug!("X11 watcher exited: {}", e);
            }
        });
        spawned_any = true;
    }

    if !spawned_any {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut watcher = WaylandWatcher::new();
            let _ = watcher.run(tx).await;
        });
    }

    // Spawn Shutdown Signal Listener
    let shutdown_socket = socket_path.clone();
    let shutdown_token = token_path.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Received shutdown signal. Cleaning up...");
        if shutdown_socket.exists() {
            let _ = std::fs::remove_file(shutdown_socket);
        }
        if shutdown_token.exists() {
            let _ = std::fs::remove_file(shutdown_token);
        }
        std::process::exit(0);
    });

    // Spawn System Tray StatusNotifierItem Service
    if config.ui.tray_icon_enabled {
        let tray_repo = repo.clone();
        let tray_crypto = Arc::clone(&crypto);
        let tray_config = Arc::clone(&config_arc);
        let tray_paused = Arc::clone(&is_paused);
        tokio::spawn(async move {
            if let Err(e) =
                tray::start_tray_service(tray_repo, tray_crypto, tray_config, tray_paused).await
            {
                debug!("D-Bus Tray service could not be initialized: {}", e);
            }
        });
    }

    // Spawn P2P LAN Encrypted Sync Service (Primary port 54123, Secondary fallback 54124)
    let sync_cfg_arc = config_arc.clone();
    let sync_repo = repo.clone();
    let sync_blob_store_base = blob_store.clone();
    let sync_crypto_base = crypto.clone();
    let listen_port = config.sync.listen_port;
    let echo_tracker_tcp = echo_tracker.clone();

    let ports_to_listen = if listen_port == 54123 {
        vec![54123, 54124]
    } else {
        vec![listen_port]
    };

    let pending_pairings_tcp = pending_pairings.clone();
    for port in ports_to_listen {
        let sync_cfg_arc = sync_cfg_arc.clone();
        let sync_repo = sync_repo.clone();
        let sync_blob_store_base = sync_blob_store_base.clone();
        let sync_crypto = sync_crypto_base.clone();
        let echo_tracker_rx = echo_tracker_tcp.clone();
        let pending_pairings_port = pending_pairings_tcp.clone();
        tokio::spawn(async move {
            let addr = format!("0.0.0.0:{}", port);
            if let Ok(listener) = tokio::net::TcpListener::bind(&addr).await {
                info!("P2P LAN Encrypted Sync listening on {}", addr);
                while let Ok((mut socket, peer_addr)) = listener.accept().await {
                    let current_sync = sync_cfg_arc.read().await.sync.clone();
                    let pin = current_sync.pairing_pin.clone();
                    let dev_name = current_sync.device_name.clone();
                    let sync_repo = sync_repo.clone();
                    let sync_blob_store = sync_blob_store_base.clone();
                    let sync_crypto = sync_crypto.clone();
                    let echo_tracker_rx = echo_tracker_rx.clone();
                    let sync_cfg_arc = sync_cfg_arc.clone();
                    let pending_pairings_conn = pending_pairings_port.clone();

                    tokio::spawn(async move {
                        use clipboard_history_core::sync::{
                            read_sync_message, send_sync_message, SyncMessage,
                        };
                        let (mut reader, mut writer) = socket.split();
                        let mut authenticated = false;
                        let clean_ip = peer_addr.ip().to_string();

                        while let Ok(msg) = read_sync_message(&mut reader, &pin).await {
                            let current_sync = sync_cfg_arc.read().await.sync.clone();
                            match msg {
                                SyncMessage::PairingRequest {
                                    origin_device,
                                    pairing_pin: _remote_pin,
                                    request_id,
                                } => {
                                    if current_sync.is_peer_trusted(&clean_ip, &origin_device) {
                                        let resp = SyncMessage::PairingResponse {
                                            request_id,
                                            accepted: true,
                                            device_name: dev_name.clone(),
                                            message: "Already paired".to_string(),
                                        };
                                        let _ =
                                            send_sync_message(&mut writer, "CH_PAIRING_V1", &resp)
                                                .await;
                                        continue;
                                    }

                                    let (resp_tx, resp_rx) =
                                        tokio::sync::oneshot::channel::<bool>();
                                    let req_info =
                                        clipboard_history_core::ipc::PendingPairingInfo {
                                            request_id: request_id.clone(),
                                            device_name: origin_device.clone(),
                                            peer_address: peer_addr.to_string(),
                                        };
                                    {
                                        let mut map = pending_pairings_conn.write().await;
                                        map.insert(
                                            request_id.clone(),
                                            server::PendingPairingItem {
                                                info: req_info,
                                                tx: resp_tx,
                                            },
                                        );
                                    }

                                    let dev_clone = origin_device.clone();
                                    let ip_str = peer_addr.ip().to_string();
                                    let req_id_notify = request_id.clone();
                                    let pending_notify = pending_pairings_conn.clone();
                                    tokio::spawn(async move {
                                        let body = format!(
                                            "'{}' ({}) wants to sync clipboard with this computer.",
                                            dev_clone, ip_str
                                        );
                                        let res = tokio::process::Command::new("notify-send")
                                            .args([
                                                "-a",
                                                "Clipboard History",
                                                "-i",
                                                "network-wireless-symbolic",
                                                "-u",
                                                "critical",
                                                "--action=accept=Accept",
                                                "--action=reject=Decline",
                                                "Clipboard Pairing Request",
                                                &body,
                                            ])
                                            .output()
                                            .await;
                                        if let Ok(out) = res {
                                            let action = String::from_utf8_lossy(&out.stdout)
                                                .trim()
                                                .to_string();
                                            if action == "accept" || action == "reject" {
                                                let mut map = pending_notify.write().await;
                                                if let Some(item) = map.remove(&req_id_notify) {
                                                    let _ = item.tx.send(action == "accept");
                                                }
                                            }
                                        }
                                    });

                                    let accepted = match tokio::time::timeout(
                                        std::time::Duration::from_secs(45),
                                        resp_rx,
                                    )
                                    .await
                                    {
                                        Ok(Ok(val)) => val,
                                        _ => false,
                                    };
                                    pending_pairings_conn.write().await.remove(&request_id);

                                    if accepted {
                                        info!(
                                            "Pairing accepted for peer '{}' ({})",
                                            origin_device, clean_ip
                                        );
                                        {
                                            let mut cfg = sync_cfg_arc.write().await;
                                            if !cfg.sync.trusted_peers.contains(&clean_ip) {
                                                cfg.sync.trusted_peers.push(clean_ip.clone());
                                            }
                                            if !origin_device.is_empty()
                                                && !cfg.sync.trusted_peers.contains(&origin_device)
                                            {
                                                cfg.sync.trusted_peers.push(origin_device.clone());
                                            }
                                            let peer_full = peer_addr.to_string();
                                            if !cfg.sync.peer_addresses.contains(&peer_full) {
                                                cfg.sync.peer_addresses.push(peer_full);
                                            }
                                            cfg.sync.enabled = true;
                                            let _ = cfg.save(AppConfig::config_path());
                                        }
                                        let resp = SyncMessage::PairingResponse {
                                            request_id,
                                            accepted: true,
                                            device_name: dev_name.clone(),
                                            message: "Pairing accepted".to_string(),
                                        };
                                        let _ =
                                            send_sync_message(&mut writer, "CH_PAIRING_V1", &resp)
                                                .await;

                                        // Stream mutual history to newly paired peer and request peer's history
                                        let (enc_enabled, max_entries) = {
                                            let c = sync_cfg_arc.read().await;
                                            (c.security.encryption_enabled, c.general.max_entries)
                                        };
                                        let _ = sync_service::stream_history_to_writer(
                                            &mut writer,
                                            &pin,
                                            &dev_name,
                                            &sync_repo,
                                            &sync_blob_store,
                                            &sync_crypto,
                                            enc_enabled,
                                            max_entries,
                                        )
                                        .await;
                                        let _ = send_sync_message(
                                            &mut writer,
                                            &pin,
                                            &SyncMessage::HistorySyncRequest {
                                                origin_device: dev_name.clone(),
                                                limit: max_entries,
                                            },
                                        )
                                        .await;
                                    } else {
                                        info!(
                                            "Pairing rejected/timed out for peer '{}' ({})",
                                            origin_device, clean_ip
                                        );
                                        let resp = SyncMessage::PairingResponse {
                                            request_id,
                                            accepted: false,
                                            device_name: dev_name.clone(),
                                            message: "Pairing request rejected or timed out"
                                                .to_string(),
                                        };
                                        let _ =
                                            send_sync_message(&mut writer, "CH_PAIRING_V1", &resp)
                                                .await;
                                    }
                                }

                                SyncMessage::UnpairRequest { origin_device } => {
                                    let mut cfg = sync_cfg_arc.write().await;
                                    cfg.sync
                                        .trusted_peers
                                        .retain(|p| p != &clean_ip && p != &origin_device);
                                    let _ = cfg.save(AppConfig::config_path());
                                    info!(
                                        "Peer '{}' ({}) requested unpairing. Removed from trusted_peers.",
                                        origin_device, clean_ip
                                    );
                                }

                                SyncMessage::AuthRequest {
                                    device_name: remote_dev,
                                    pairing_pin,
                                    ..
                                } => {
                                    let is_trusted =
                                        current_sync.is_peer_trusted(&clean_ip, &remote_dev);
                                    if pairing_pin == pin || is_trusted {
                                        authenticated = true;
                                        let _ = send_sync_message(
                                            &mut writer,
                                            &pin,
                                            &SyncMessage::AuthResponse {
                                                success: true,
                                                device_name: dev_name.clone(),
                                                message: "Authenticated".to_string(),
                                            },
                                        )
                                        .await;
                                    } else {
                                        let _ = send_sync_message(
                                            &mut writer,
                                            &pin,
                                            &SyncMessage::AuthResponse {
                                                success: false,
                                                device_name: dev_name.clone(),
                                                message: "Invalid PIN".to_string(),
                                            },
                                        )
                                        .await;
                                        break;
                                    }
                                }

                                SyncMessage::HistorySyncRequest {
                                    origin_device,
                                    limit,
                                } => {
                                    let is_trusted =
                                        current_sync.is_peer_trusted(&clean_ip, &origin_device);
                                    if is_trusted || authenticated {
                                        let (enc_enabled, max_entries) = {
                                            let c = sync_cfg_arc.read().await;
                                            (c.security.encryption_enabled, c.general.max_entries)
                                        };
                                        let capped_limit = limit.clamp(1, 500);
                                        let _ = sync_service::stream_history_to_writer(
                                            &mut writer,
                                            &pin,
                                            &dev_name,
                                            &sync_repo,
                                            &sync_blob_store,
                                            &sync_crypto,
                                            enc_enabled,
                                            capped_limit,
                                        )
                                        .await;
                                        let _ = sync_repo.evict_capacity(max_entries);
                                    }
                                }

                                SyncMessage::HistorySyncComplete {
                                    origin_device,
                                    count,
                                } => {
                                    info!(
                                        "Peer '{}' completed history sync ({} entries)",
                                        origin_device, count
                                    );
                                    let max_entries = sync_cfg_arc.read().await.general.max_entries;
                                    let _ = sync_repo.evict_capacity(max_entries);
                                }

                                SyncMessage::EntrySync {
                                    origin_device,
                                    entry,
                                    blob_payload,
                                    thumbnail_payload,
                                    is_history_batch,
                                } => {
                                    let is_trusted =
                                        current_sync.is_peer_trusted(&clean_ip, &origin_device);
                                    if !is_trusted && !authenticated {
                                        tracing::warn!(
                                            "Rejected unauthenticated EntrySync from untrusted peer '{}' ({})",
                                            origin_device,
                                            peer_addr
                                        );
                                        break;
                                    }
                                    let enc_enabled =
                                        sync_cfg_arc.read().await.security.encryption_enabled;
                                    let _ = sync_service::handle_incoming_entry_sync(
                                        *entry,
                                        blob_payload,
                                        thumbnail_payload,
                                        is_history_batch,
                                        &sync_repo,
                                        &sync_blob_store,
                                        &sync_crypto,
                                        enc_enabled,
                                        &echo_tracker_rx,
                                    )
                                    .await;
                                }

                                SyncMessage::ClearHistorySync {
                                    origin_device,
                                    include_pinned,
                                } => {
                                    let is_trusted =
                                        current_sync.is_peer_trusted(&clean_ip, &origin_device);
                                    if !is_trusted && !authenticated {
                                        tracing::warn!(
                                            "Rejected unauthenticated ClearHistorySync from untrusted peer '{}' ({})",
                                            origin_device,
                                            peer_addr
                                        );
                                        break;
                                    }
                                    info!(
                                        "Processing ClearHistorySync from peer '{}' (include_pinned={})",
                                        origin_device, include_pinned
                                    );
                                    let _ = sync_repo.clear(include_pinned);
                                    if let Ok(active_hashes) = sync_repo.get_all_blob_hashes() {
                                        let _ = sync_blob_store.cleanup_orphans(&active_hashes);
                                    }
                                }

                                SyncMessage::Ping => {
                                    if !authenticated
                                        && !current_sync.is_peer_trusted(&clean_ip, "")
                                    {
                                        break;
                                    }
                                    let _ =
                                        send_sync_message(&mut writer, &pin, &SyncMessage::Pong)
                                            .await;
                                }
                                _ => {}
                            }
                        }
                    });
                }
            } else {
                debug!("Could not bind LAN Sync port {}", port);
            }
        });
    }

    // Spawn P2P UDP Peer Discovery Service
    let tracker_udp = peer_tracker.clone();
    let sync_cfg_udp = config_arc.clone();
    let repo_udp = repo.clone();
    let blob_udp = blob_store.clone();
    let crypto_udp = crypto.clone();
    let echo_tracker_udp = echo_tracker.clone();
    let listen_port_udp = config.sync.listen_port;
    tokio::spawn(async move {
        start_udp_discovery(
            listen_port_udp,
            tracker_udp,
            sync_cfg_udp,
            repo_udp,
            blob_udp,
            crypto_udp,
            echo_tracker_udp,
        )
        .await;
    });

    // Spawn Micro-Batching Writer Task (commit transactions every 250ms or 10 items)
    let (batch_tx, mut batch_rx) = mpsc::channel::<ClipboardEntry>(128);
    let batch_repo = repo.clone();
    let batch_blob_store = blob_store.clone();
    let batch_crypto = Arc::clone(&crypto);
    let batch_config_arc = Arc::clone(&config_arc);
    let batch_peer_tracker = peer_tracker.clone();

    tokio::spawn(async move {
        use std::time::Duration;
        let mut buffer: Vec<ClipboardEntry> = Vec::with_capacity(10);
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                item = batch_rx.recv() => {
                    match item {
                        Some(entry) => {
                            buffer.push(entry);
                            if buffer.len() >= 10 {
                                flush_micro_batch(
                                    &mut buffer,
                                    &batch_repo,
                                    &batch_blob_store,
                                    &batch_crypto,
                                    &batch_config_arc,
                                    &batch_peer_tracker,
                                ).await;
                            }
                        }
                        None => {
                            if !buffer.is_empty() {
                                flush_micro_batch(
                                    &mut buffer,
                                    &batch_repo,
                                    &batch_blob_store,
                                    &batch_crypto,
                                    &batch_config_arc,
                                    &batch_peer_tracker,
                                ).await;
                            }
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        flush_micro_batch(
                            &mut buffer,
                            &batch_repo,
                            &batch_blob_store,
                            &batch_crypto,
                            &batch_config_arc,
                            &batch_peer_tracker,
                        ).await;
                    }
                }
            }
        }
    });

    let window_focus_detector = WindowFocusDetector::new();

    // Main Event Processing Loop
    while let Some(event) = event_rx.recv().await {
        if is_paused.load(Ordering::Relaxed) {
            debug!("Clipboard monitoring paused. Skipping event.");
            continue;
        }

        if !circuit_breaker.allow_event() {
            continue;
        }

        let current_cfg = config_arc.read().await.clone();

        // 1. Query active window once per event (Deduplicated: HIGH-05)
        let active_win = window_focus_detector.get_active_window_info();
        let active_class = active_win.as_ref().map(|(_, c)| c.as_str());

        // Incognito & Private Browsing Window Check (adjustable in settings)
        if current_cfg.security.ignore_incognito_windows {
            if let Some((ref title, ref class)) = active_win {
                if WindowFocusDetector::matches_incognito_pattern(
                    title,
                    class,
                    &current_cfg.security.incognito_window_patterns,
                ) {
                    info!(
                        "Ignored clipboard copy originating while incognito/private browsing window was active ('{}')",
                        title
                    );
                    continue;
                }
            }
        }

        // 2. Password Manager & Sensitive MIME check (adjustable in settings)
        if current_cfg.security.ignore_password_managers
            && PasswordManagerGuard::is_sensitive_mime(&event.mime_types)
        {
            info!("Ignored clipboard entry flagged by password manager");
            continue;
        }

        // 3. Application Filter check (Blacklist or Whitelist)
        let source_app = event.source_app.as_deref().or(active_class);

        if !current_cfg
            .security
            .is_app_allowed(source_app, active_class)
        {
            info!(
                "Ignored clipboard entry filtered by application rules (mode: {:?}, app: {:?}, window_class: {:?})",
                current_cfg.security.app_filter_mode, source_app, active_class
            );
            continue;
        }

        // Primary selection filtering
        if event.selection == watcher::traits::SelectionTarget::Primary {
            if !current_cfg.general.capture_primary_selection {
                continue;
            }
            if let Some(ref t) = event.text {
                if t.trim().len() < current_cfg.general.primary_min_length {
                    continue;
                }
            }
        }

        // 4. Process Text / HTML / UriList
        if let Some(mut text) = event.text {
            if text.trim().is_empty() {
                continue;
            }

            let hash = BlobStore::compute_hash(text.as_bytes());

            // Check if this event was an echo of an incoming sync we just injected into system clipboard
            if echo_tracker.is_echo(&hash).await {
                debug!("Skipping broadcast of locally injected synced text entry");
                continue;
            }

            // Automatic URL De-Tracker & Privacy Cleaner if enabled in settings
            if current_cfg.security.auto_clean_tracking_urls {
                text = UrlCleaner::clean_text_urls(&text);
            }

            if !circuit_breaker.allow_payload_size(text.len()) {
                continue;
            }

            // Sensitive data check
            if secret_filter.contains_secret(&text) {
                match current_cfg.security.secret_policy {
                    SecretHandlingPolicy::Reject => {
                        info!("Ignored clipboard entry containing sensitive secret / private key");
                        continue;
                    }
                    SecretHandlingPolicy::Mask => {
                        text = secret_filter.mask(&text);
                    }
                    SecretHandlingPolicy::Allow => {}
                }
            }

            let cleaned_html = event.html.map(|h| {
                if current_cfg.security.auto_clean_tracking_urls {
                    UrlCleaner::clean_text_urls(&h)
                } else {
                    h
                }
            });

            let mut entry = if event.mime_types.iter().any(|m| m == "text/uri-list") {
                ClipboardEntry::new_uri_list(
                    text.clone(),
                    hash,
                    event.mime_types,
                    source_app.map(|s| s.to_string()),
                )
            } else if let Some(ref html) = cleaned_html {
                ClipboardEntry::new_html(
                    text.clone(),
                    html.clone(),
                    hash,
                    event.mime_types,
                    source_app.map(|s| s.to_string()),
                )
            } else {
                ClipboardEntry::new_text(
                    text.clone(),
                    hash,
                    event.mime_types,
                    source_app.map(|s| s.to_string()),
                )
            };

            // Storage encryption at rest with Envelope Encryption (DEK)
            if current_cfg.security.encryption_enabled {
                if let Ok(wrapped_dek) = crypto.encrypt_entry_envelope(&mut entry) {
                    let _ = repo.insert_entry_dek(&entry.id, &wrapped_dek, &[]);
                } else {
                    entry.preview = crypto.encrypt_str(&entry.preview).unwrap_or(entry.preview);
                    entry.text_content =
                        entry.text_content.and_then(|t| crypto.encrypt_str(&t).ok());
                    entry.html_content =
                        entry.html_content.and_then(|h| crypto.encrypt_str(&h).ok());
                }
            }

            info!(
                "Captured clipboard entry: [{}] {}",
                entry.entry_type, entry.preview
            );
            let _ = batch_tx.send(entry).await;
        } else if let Some(image_bytes) = event.image_data {
            if !circuit_breaker.allow_payload_size(image_bytes.len()) {
                continue;
            }

            // Check if this image was an echo of an incoming sync we just injected into system clipboard
            let hash = BlobStore::compute_hash(&image_bytes);
            if echo_tracker.is_echo(&hash).await {
                debug!("Skipping broadcast of locally injected synced image");
                continue;
            }

            let blob_store_clone = blob_store.clone();
            let crypto_clone = Arc::clone(&crypto);
            let encryption_enabled = current_cfg.security.encryption_enabled;
            let image_bytes_clone = image_bytes.clone();
            let mime_types = event.mime_types.clone();
            let src_app = source_app.map(|s| s.to_string());

            // Offload thumbnail generation & blob storage to spawn_blocking (HIGH-06)
            let process_res = tokio::task::spawn_blocking(move || {
                let blob_hash = if encryption_enabled {
                    blob_store_clone.save_encrypted(&image_bytes_clone, &crypto_clone)?
                } else {
                    blob_store_clone.save(&image_bytes_clone)?
                };

                // Generate thumbnail
                let (dimensions, thumb_hash) =
                    match ThumbnailGenerator::generate(&image_bytes_clone) {
                        Ok((dims, thumb_bytes)) => {
                            let th = if encryption_enabled {
                                blob_store_clone
                                    .save_encrypted(&thumb_bytes, &crypto_clone)
                                    .ok()
                            } else {
                                blob_store_clone.save(&thumb_bytes).ok()
                            };
                            (dims, th)
                        }
                        Err(_) => ((0, 0), None),
                    };

                let entry = ClipboardEntry::new_image(
                    blob_hash,
                    thumb_hash,
                    image_bytes_clone.len(),
                    dimensions,
                    mime_types,
                    src_app,
                );
                Ok::<_, clipboard_history_core::error::CoreError>(entry)
            })
            .await;

            match process_res {
                Ok(Ok(entry)) => {
                    info!("Captured clipboard image entry: {}", entry.preview);
                    let _ = batch_tx.send(entry).await;
                }
                Ok(Err(e)) => error!("Failed to process image blob: {}", e),
                Err(e) => error!("Tokio spawn_blocking error processing image: {}", e),
            }

            #[cfg(target_os = "linux")]
            unsafe {
                libc::malloc_trim(0);
            }
        }
    }

    Ok(())
}

async fn flush_micro_batch(
    buffer: &mut Vec<ClipboardEntry>,
    repo: &SqliteRepository,
    blob_store: &BlobStore,
    crypto: &CryptoEngine,
    config: &Arc<RwLock<AppConfig>>,
    peer_tracker: &PeerTracker,
) {
    if buffer.is_empty() {
        return;
    }
    let items = std::mem::take(buffer);
    let repo_clone = repo.clone();
    let items_clone = items.clone();

    let res =
        tokio::task::spawn_blocking(move || repo_clone.batch_insert_or_update(&items_clone)).await;

    match res {
        Ok(Ok(saved_items)) => {
            debug!(
                "Committed micro-batch of {} items to SQLite",
                saved_items.len()
            );
            let current_cfg = config.read().await.clone();
            let active_peers = peer_tracker.get_all_peers().await;
            for entry in saved_items {
                broadcast_to_peers(
                    &current_cfg.sync,
                    &entry,
                    repo,
                    blob_store,
                    crypto,
                    current_cfg.security.encryption_enabled,
                    &active_peers,
                );
            }
        }
        Ok(Err(e)) => error!("Failed to commit micro-batch: {}", e),
        Err(e) => error!("Tokio spawn_blocking error during micro-batch: {}", e),
    }

    #[cfg(target_os = "linux")]
    unsafe {
        libc::malloc_trim(0);
    }
}

fn broadcast_to_peers(
    cfg: &clipboard_history_core::config::SyncConfig,
    entry: &ClipboardEntry,
    repo: &SqliteRepository,
    blob_store: &BlobStore,
    crypto: &CryptoEngine,
    encryption_enabled: bool,
    known_peers: &[(String, String)],
) {
    if !cfg.enabled {
        return;
    }
    use clipboard_history_core::sync::SyncMessage;

    let mut entry_clone = entry.clone();
    sync_service::decrypt_entry_for_transit(&mut entry_clone, repo, crypto);

    let (blob_payload, thumbnail_payload) =
        if entry.entry_type == clipboard_history_core::domain::EntryType::Image {
            let b = sync_service::read_image_blob(entry, blob_store, crypto, encryption_enabled);
            let t = sync_service::read_thumbnail_blob(
                entry,
                b.as_deref(),
                blob_store,
                crypto,
                encryption_enabled,
            );
            (b, t)
        } else {
            (None, None)
        };

    let msg = SyncMessage::EntrySync {
        origin_device: cfg.device_name.clone(),
        entry: Box::new(entry_clone),
        blob_payload,
        thumbnail_payload,
        is_history_batch: false,
    };

    sync_service::broadcast_sync_message(cfg, known_peers, msg);
}
