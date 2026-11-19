mod circuit_breaker;
mod eviction;
mod notification;
mod peer_discovery;
mod server;
pub mod sync_service;
mod tray;
mod updater_service;
mod watcher;
mod window_focus;

use circuit_breaker::CircuitBreaker;
use eviction::EvictionWorker;
use peer_discovery::{start_udp_discovery, PeerTracker};
use server::DaemonServer;
use watcher::traits::{ClipboardWatcher, RawClipboardEvent};
use watcher::{KlipperWatcher, WaylandWatcher, X11Watcher};
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

    let mut config = if let Some(ref path) = cli.config {
        let content = std::fs::read_to_string(path)?;
        toml::from_str::<AppConfig>(&content)?
    } else {
        AppConfig::load_or_default()
    };

    if sanitize_peer_addresses(&mut config) {
        let save_path = cli.config.clone().unwrap_or_else(AppConfig::config_path);
        if let Err(e) = config.save(&save_path) {
            tracing::warn!(
                "Failed to save sanitized config to {}: {}",
                save_path.display(),
                e
            );
        }
    }

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

    // Start background periodic update checker
    updater_service::start_update_checker(Arc::clone(&config_arc));

    // Detect Display Server
    let session_type = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_lowercase();
    let wayland_display = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let has_wayland_socket = has_active_wayland_socket();
    let is_pure_x11 = session_type == "x11" && wayland_display.is_none() && !has_wayland_socket;

    let is_wayland = !is_pure_x11;
    let is_x11 = std::env::var("DISPLAY").is_ok() || is_pure_x11;

    info!(
        "Detected display environment: Wayland={}, X11={} (session: {}, wayland_display: {:?}, active_socket: {})",
        is_wayland, is_x11, session_type, wayland_display, has_wayland_socket
    );

    let (event_tx, mut event_rx) = mpsc::channel::<RawClipboardEvent>(64);
    let capture_primary = config.general.capture_primary_selection;

    // 1. Spawn Wayland watcher unless we are certain this is a pure X11 session
    if is_wayland {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut watcher = WaylandWatcher::new(capture_primary);
            if let Err(e) = watcher.run(tx).await {
                tracing::warn!("Wayland watcher exited: {}", e);
            }
        });
    }

    // 2. Spawn X11 watcher if X11 / Xwayland is available
    if is_x11 {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut watcher = X11Watcher::new(capture_primary);
            if let Err(e) = watcher.run(tx).await {
                debug!("X11 watcher exited: {}", e);
            }
        });
    }

    // 3. Spawn KDE Klipper D-Bus watcher alongside Wayland/X11 watchers
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut watcher = KlipperWatcher::new();
            if let Err(e) = watcher.run(tx).await {
                debug!("Klipper watcher exited: {}", e);
            }
        });
    }

    // Spawn Shutdown Signal Listener (handles SIGINT and SIGTERM from systemd)
    let shutdown_socket = socket_path.clone();
    let shutdown_token = token_path.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = sigterm.recv() => {},
                }
            } else {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }

        info!("Received shutdown signal. Cleaning up socket and authentication token...");
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
    let tracker_tcp = peer_tracker.clone();
    for port in ports_to_listen {
        let sync_cfg_arc = sync_cfg_arc.clone();
        let sync_repo = sync_repo.clone();
        let sync_blob_store_base = sync_blob_store_base.clone();
        let sync_crypto = sync_crypto_base.clone();
        let echo_tracker_rx = echo_tracker_tcp.clone();
        let pending_pairings_port = pending_pairings_tcp.clone();
        let tracker_port = tracker_tcp.clone();
        tokio::spawn(async move {
            let addr = format!("0.0.0.0:{}", port);
            if let Ok(listener) = tokio::net::TcpListener::bind(&addr).await {
                info!("P2P LAN Encrypted Sync listening on {}", addr);
                while let Ok((mut socket, peer_addr)) = listener.accept().await {
                    let sync_repo = sync_repo.clone();
                    let sync_blob_store = sync_blob_store_base.clone();
                    let sync_crypto = sync_crypto.clone();
                    let echo_tracker_rx = echo_tracker_rx.clone();
                    let sync_cfg_arc = sync_cfg_arc.clone();
                    let pending_pairings_conn = pending_pairings_port.clone();
                    let tracker_conn = tracker_port.clone();

                    tokio::spawn(async move {
                        use clipboard_history_core::sync::{
                            clean_ip_address, format_peer_socket_addr, normalize_ip,
                            read_sync_message_with_pins, send_sync_message, SyncMessage,
                        };
                        use tokio::io::AsyncWriteExt;

                        let (mut reader, mut writer) = socket.split();
                        let mut authenticated = false;
                        let mut session_pin: Option<String> = None;
                        let clean_ip = normalize_ip(peer_addr.ip());

                        loop {
                            let current_sync = sync_cfg_arc.read().await.sync.clone();
                            let dev_name = current_sync.device_name.clone();
                            let default_pin = current_sync.pairing_pin.clone();

                            let mut candidate_pins: Vec<String> = Vec::new();
                            if let Some(ref sp) = session_pin {
                                if !sp.is_empty() && !candidate_pins.contains(sp) {
                                    candidate_pins.push(sp.clone());
                                }
                            }
                            if let Some(peer_pin) = current_sync.peer_pins.get(&clean_ip) {
                                if !peer_pin.is_empty() && !candidate_pins.contains(peer_pin) {
                                    candidate_pins.push(peer_pin.clone());
                                }
                            }
                            for pin_val in current_sync.peer_pins.values() {
                                if !pin_val.is_empty() && !candidate_pins.contains(pin_val) {
                                    candidate_pins.push(pin_val.clone());
                                }
                            }
                            let extra_pin_refs: Vec<&str> =
                                candidate_pins.iter().map(|s| s.as_str()).collect();

                            let read_res = tokio::time::timeout(
                                std::time::Duration::from_secs(60),
                                read_sync_message_with_pins(
                                    &mut reader,
                                    &extra_pin_refs,
                                    &default_pin,
                                ),
                            )
                            .await;

                            let (msg, matched_pin) = match read_res {
                                Ok(Ok(pair)) => pair,
                                Ok(Err(e)) => {
                                    debug!("Sync connection from {} ended: {}", clean_ip, e);
                                    break;
                                }
                                Err(_) => {
                                    debug!("Sync connection from {} timed out after 60s", clean_ip);
                                    break;
                                }
                            };

                            if matched_pin != "CH_PAIRING_V1" {
                                session_pin = Some(matched_pin.clone());
                            }

                            let current_sync = sync_cfg_arc.read().await.sync.clone();
                            match msg {
                                SyncMessage::PairingRequest {
                                    origin_device,
                                    pairing_pin: remote_pin,
                                    request_id,
                                } => {
                                    if current_sync.is_peer_trusted(&clean_ip, &origin_device) {
                                        let peer_listen_port = tracker_conn
                                            .get_port_for_ip(&clean_ip)
                                            .await
                                            .unwrap_or(54123);
                                        let peer_listen_addr =
                                            format_peer_socket_addr(&clean_ip, peer_listen_port);
                                        tracker_conn
                                            .add_or_update(
                                                peer_listen_addr.clone(),
                                                origin_device.clone(),
                                            )
                                            .await;
                                        {
                                            let mut cfg = sync_cfg_arc.write().await;
                                            update_paired_peer_config(
                                                &mut cfg,
                                                &clean_ip,
                                                &origin_device,
                                                &peer_listen_addr,
                                                &remote_pin,
                                            );
                                            let _ = cfg.save(AppConfig::config_path());
                                        }
                                        let resp = SyncMessage::PairingResponse {
                                            request_id,
                                            accepted: true,
                                            device_name: dev_name.clone(),
                                            message: "Already paired".to_string(),
                                        };
                                        let _ =
                                            send_sync_message(&mut writer, "CH_PAIRING_V1", &resp)
                                                .await;
                                        let _ = writer.shutdown().await;
                                        break;
                                    }

                                    let peer_listen_port = tracker_conn
                                        .get_port_for_ip(&clean_ip)
                                        .await
                                        .unwrap_or(54123);
                                    let display_addr =
                                        format_peer_socket_addr(&clean_ip, peer_listen_port);

                                    let (resp_tx, resp_rx) =
                                        tokio::sync::oneshot::channel::<bool>();
                                    let req_info =
                                        clipboard_history_core::ipc::PendingPairingInfo {
                                            request_id: request_id.clone(),
                                            device_name: origin_device.clone(),
                                            peer_address: display_addr,
                                            desktop_notification_active: false,
                                        };
                                    {
                                        let mut map = pending_pairings_conn.write().await;
                                        // Deduplicate: cancel any prior pending pairing from the same origin device or IP
                                        let existing_key = map.iter().find_map(|(id, item)| {
                                            if item.info.device_name == origin_device
                                                || clean_ip_address(&item.info.peer_address)
                                                    == clean_ip
                                            {
                                                Some(id.clone())
                                            } else {
                                                None
                                            }
                                        });
                                        if let Some(old_id) = existing_key {
                                            if let Some(old_item) = map.remove(&old_id) {
                                                let _ = old_item.tx.send(false);
                                                if let Some(nid) = old_item.notif_id {
                                                    tokio::spawn(async move {
                                                        notification::close_notification(nid).await;
                                                    });
                                                }
                                            }
                                        }

                                        map.insert(
                                            request_id.clone(),
                                            server::PendingPairingItem {
                                                info: req_info,
                                                tx: resp_tx,
                                                notif_id: None,
                                            },
                                        );
                                    }

                                    let dev_clone = origin_device.clone();
                                    let ip_str = clean_ip.clone();
                                    let req_id_notify = request_id.clone();
                                    let pending_notify = pending_pairings_conn.clone();
                                    tokio::spawn(async move {
                                        notification::dispatch_pairing_notification(
                                            dev_clone,
                                            ip_str,
                                            req_id_notify,
                                            pending_notify,
                                        )
                                        .await;
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
                                    if let Some(item) =
                                        pending_pairings_conn.write().await.remove(&request_id)
                                    {
                                        if let Some(nid) = item.notif_id {
                                            tokio::spawn(async move {
                                                notification::close_notification(nid).await;
                                            });
                                        }
                                    }

                                    if accepted {
                                        info!(
                                            "Pairing accepted for peer '{}' ({})",
                                            origin_device, clean_ip
                                        );
                                        let peer_listen_port = tracker_conn
                                            .get_port_for_ip(&clean_ip)
                                            .await
                                            .unwrap_or(54123);
                                        let peer_listen_addr =
                                            format_peer_socket_addr(&clean_ip, peer_listen_port);

                                        tracker_conn
                                            .add_or_update(
                                                peer_listen_addr.clone(),
                                                origin_device.clone(),
                                            )
                                            .await;
                                        {
                                            let mut cfg = sync_cfg_arc.write().await;
                                            update_paired_peer_config(
                                                &mut cfg,
                                                &clean_ip,
                                                &origin_device,
                                                &peer_listen_addr,
                                                &remote_pin,
                                            );
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
                                        let _ = writer.shutdown().await;
                                        break;
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
                                        let _ = writer.shutdown().await;
                                        break;
                                    }
                                }

                                SyncMessage::UnpairRequest { origin_device } => {
                                    let clean_origin_ip = clean_ip.clone();
                                    {
                                        let mut cfg = sync_cfg_arc.write().await;
                                        cfg.sync.trusted_peers.retain(|p| {
                                            let p_clean = clean_ip_address(p);
                                            p_clean != clean_origin_ip
                                                && (origin_device.is_empty() || p != &origin_device)
                                        });
                                        cfg.sync.peer_addresses.retain(|p| {
                                            let p_clean = clean_ip_address(p);
                                            p_clean != clean_origin_ip
                                                && (origin_device.is_empty() || p != &origin_device)
                                        });
                                        cfg.sync.peer_pins.remove(&clean_origin_ip);
                                        if !origin_device.is_empty() {
                                            cfg.sync.peer_pins.remove(&origin_device);
                                        }
                                        let _ = cfg.save(AppConfig::config_path());
                                    }
                                    tracker_conn.remove_peer(&clean_ip).await;
                                    if !origin_device.is_empty() {
                                        tracker_conn.remove_peer(&origin_device).await;
                                    }
                                    info!(
                                        "Peer '{}' ({}) requested unpairing. Removed from trusted_peers, peer_addresses, and peer_pins.",
                                        origin_device, clean_ip
                                    );

                                    let resp = SyncMessage::UnpairResponse {
                                        success: true,
                                        message: "Unpaired successfully".to_string(),
                                    };
                                    let _ = send_sync_message(&mut writer, "CH_PAIRING_V1", &resp)
                                        .await;
                                    let _ = writer.shutdown().await;
                                    break;
                                }

                                SyncMessage::AuthRequest {
                                    device_name: remote_dev,
                                    ..
                                } => {
                                    let reply_pin = if matched_pin != "CH_PAIRING_V1" {
                                        matched_pin.clone()
                                    } else {
                                        current_sync.pin_for_peer(&clean_ip, &remote_dev)
                                    };
                                    let is_trusted =
                                        current_sync.is_peer_trusted(&clean_ip, &remote_dev);
                                    if is_trusted && matched_pin != "CH_PAIRING_V1" {
                                        authenticated = true;
                                        session_pin = Some(reply_pin.clone());
                                        let _ = send_sync_message(
                                            &mut writer,
                                            &reply_pin,
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
                                            &reply_pin,
                                            &SyncMessage::AuthResponse {
                                                success: false,
                                                device_name: dev_name.clone(),
                                                message: if !is_trusted {
                                                    "Peer not paired. Please send a pairing request first.".to_string()
                                                } else {
                                                    "Invalid PIN".to_string()
                                                },
                                            },
                                        )
                                        .await;
                                        let _ = writer.shutdown().await;
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
                                        let reply_pin = session_pin.clone().unwrap_or_else(|| {
                                            current_sync.pin_for_peer(&clean_ip, &origin_device)
                                        });
                                        let (enc_enabled, max_entries) = {
                                            let c = sync_cfg_arc.read().await;
                                            (c.security.encryption_enabled, c.general.max_entries)
                                        };
                                        let capped_limit = limit.clamp(1, 500);
                                        let _ = sync_service::stream_history_to_writer(
                                            &mut writer,
                                            &reply_pin,
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

                                SyncMessage::PinSync {
                                    origin_device,
                                    content_hash,
                                    is_pinned,
                                    entry,
                                    blob_payload,
                                    thumbnail_payload,
                                } => {
                                    let is_trusted =
                                        current_sync.is_peer_trusted(&clean_ip, &origin_device);
                                    if !is_trusted && !authenticated {
                                        tracing::warn!(
                                            "Rejected unauthenticated PinSync from untrusted peer '{}' ({})",
                                            origin_device,
                                            peer_addr
                                        );
                                        break;
                                    }
                                    info!(
                                        "Processing PinSync from peer '{}' for hash '{}': is_pinned={}",
                                        origin_device, content_hash, is_pinned
                                    );
                                    if is_pinned {
                                        let res = sync_repo.set_pinned_by_hash(&content_hash, true);
                                        if res.is_err() {
                                            if let Some(entry_box) = entry {
                                                let enc_enabled = sync_cfg_arc
                                                    .read()
                                                    .await
                                                    .security
                                                    .encryption_enabled;
                                                let _ = sync_service::handle_incoming_entry_sync(
                                                    *entry_box,
                                                    blob_payload,
                                                    thumbnail_payload,
                                                    true,
                                                    &sync_repo,
                                                    &sync_blob_store,
                                                    &sync_crypto,
                                                    enc_enabled,
                                                    &echo_tracker_rx,
                                                )
                                                .await;
                                                let _ = sync_repo
                                                    .set_pinned_by_hash(&content_hash, true);
                                            }
                                        }
                                    } else {
                                        let _ = sync_repo.set_pinned_by_hash(&content_hash, false);
                                    }
                                }

                                SyncMessage::Ping => {
                                    if !authenticated
                                        && !current_sync.is_peer_trusted(&clean_ip, "")
                                    {
                                        break;
                                    }
                                    let reply_pin = session_pin.clone().unwrap_or_else(|| {
                                        current_sync.pin_for_peer(&clean_ip, "")
                                    });
                                    let _ = send_sync_message(
                                        &mut writer,
                                        &reply_pin,
                                        &SyncMessage::Pong,
                                    )
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
    let (batch_tx, mut batch_rx) = mpsc::channel::<PendingBatchItem>(128);
    let batch_repo = repo.clone();
    let batch_blob_store = blob_store.clone();
    let batch_crypto = Arc::clone(&crypto);
    let batch_config_arc = Arc::clone(&config_arc);
    let batch_peer_tracker = peer_tracker.clone();

    tokio::spawn(async move {
        use std::time::Duration;
        let mut buffer: Vec<PendingBatchItem> = Vec::with_capacity(10);
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

    let window_focus_detector = Arc::new(WindowFocusDetector::new());
    let mut recent_captures: std::collections::VecDeque<(String, std::time::Instant)> =
        std::collections::VecDeque::with_capacity(64);

    // Main Event Processing Loop
    while let Some(event) = event_rx.recv().await {
        if is_paused.load(Ordering::Relaxed) {
            debug!("Clipboard monitoring paused. Skipping event.");
            continue;
        }

        let current_cfg = config_arc.read().await.clone();
        circuit_breaker.update_limits(
            current_cfg.general.rate_limit_per_second,
            current_cfg.general.max_blob_size_mb,
        );

        if !circuit_breaker.allow_event() {
            continue;
        }

        // Primary selection filtering FIRST before expensive focus detection
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

        // 1. Query active window once per event (offloaded with 300ms timeout)
        let detector_clone = Arc::clone(&window_focus_detector);
        let active_win = match tokio::time::timeout(
            std::time::Duration::from_millis(300),
            tokio::task::spawn_blocking(move || detector_clone.get_active_window_info()),
        )
        .await
        {
            Ok(Ok(info)) => info,
            Ok(Err(e)) => {
                debug!("Window focus task join error: {}", e);
                None
            }
            Err(_) => {
                debug!("Window focus detection timed out after 300ms");
                None
            }
        };
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

        // 4. Process Text / HTML / UriList
        if let Some(mut text) = event.text {
            if text.trim().is_empty() {
                continue;
            }

            let raw_hash = BlobStore::compute_hash(text.as_bytes());

            // Check echo BEFORE UrlCleaner so synced URLs still suppress echo properly
            if echo_tracker.is_echo(&raw_hash).await {
                debug!("Skipping broadcast of locally injected synced text entry");
                continue;
            }

            if is_recent_duplicate(&mut recent_captures, &raw_hash) {
                debug!("Skipping duplicate capture from concurrent clipboard watchers");
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

            let hash = BlobStore::compute_hash(text.as_bytes());
            if hash != raw_hash {
                if echo_tracker.is_echo(&hash).await {
                    debug!("Skipping broadcast of cleaned synced text echo");
                    continue;
                }
                if is_recent_duplicate(&mut recent_captures, &hash) {
                    debug!("Skipping duplicate cleaned capture");
                    continue;
                }
            }
            record_recent_capture(&mut recent_captures, raw_hash.clone());
            if hash != raw_hash {
                record_recent_capture(&mut recent_captures, hash.clone());
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
                    hash.clone(),
                    event.mime_types,
                    source_app.map(|s| s.to_string()),
                )
            } else if let Some(ref html) = cleaned_html {
                ClipboardEntry::new_html(
                    text.clone(),
                    html.clone(),
                    hash.clone(),
                    event.mime_types,
                    source_app.map(|s| s.to_string()),
                )
            } else {
                ClipboardEntry::new_text(
                    text.clone(),
                    hash.clone(),
                    event.mime_types,
                    source_app.map(|s| s.to_string()),
                )
            };

            let orig_id = entry.id.clone();
            let mut wrapped_dek_opt = None;

            // Storage encryption at rest with Envelope Encryption (DEK)
            if current_cfg.security.encryption_enabled {
                let already_exists = repo.get_by_hash(&hash).ok().flatten().is_some();
                if !already_exists {
                    if let Ok(wrapped_dek) = crypto.encrypt_entry_envelope(&mut entry) {
                        wrapped_dek_opt = Some(wrapped_dek);
                    } else {
                        entry.preview = crypto.encrypt_str(&entry.preview).unwrap_or(entry.preview);
                        entry.text_content =
                            entry.text_content.and_then(|t| crypto.encrypt_str(&t).ok());
                        entry.html_content =
                            entry.html_content.and_then(|h| crypto.encrypt_str(&h).ok());
                    }
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
            let _ = batch_tx
                .send(PendingBatchItem {
                    entry,
                    orig_id,
                    wrapped_dek: wrapped_dek_opt,
                })
                .await;
        } else if let Some(image_bytes) = event.image_data {
            if !circuit_breaker.allow_payload_size(image_bytes.len()) {
                continue;
            }

            // Check if this image was an echo or a recent duplicate
            let plain_hash = BlobStore::compute_hash(&image_bytes);
            if echo_tracker.is_echo(&plain_hash).await {
                debug!("Skipping broadcast of locally injected synced image");
                continue;
            }
            if is_recent_duplicate(&mut recent_captures, &plain_hash) {
                debug!("Skipping duplicate image capture from concurrent clipboard watchers");
                continue;
            }
            record_recent_capture(&mut recent_captures, plain_hash.clone());

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

                let mut entry = ClipboardEntry::new_image(
                    blob_hash,
                    thumb_hash,
                    image_bytes_clone.len(),
                    dimensions,
                    mime_types,
                    src_app,
                );
                entry.content_hash = plain_hash;
                Ok::<_, clipboard_history_core::error::CoreError>(entry)
            })
            .await;

            match process_res {
                Ok(Ok(entry)) => {
                    info!("Captured clipboard image entry: {}", entry.preview);
                    let orig_id = entry.id.clone();
                    let _ = batch_tx
                        .send(PendingBatchItem {
                            entry,
                            orig_id,
                            wrapped_dek: None,
                        })
                        .await;
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

struct PendingBatchItem {
    entry: ClipboardEntry,
    orig_id: String,
    wrapped_dek: Option<Vec<u8>>,
}

fn has_active_wayland_socket() -> bool {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            let uid = unsafe { libc::getuid() };
            format!("/run/user/{}", uid)
        });
    let Ok(entries) = std::fs::read_dir(&runtime_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("wayland-")
            && !name_str.ends_with(".lock")
            && std::os::unix::net::UnixStream::connect(entry.path()).is_ok()
        {
            return true;
        }
    }
    false
}

fn sanitize_peer_addresses(config: &mut AppConfig) -> bool {
    let listen_port = config.sync.listen_port;
    let mut sanitized: Vec<String> = Vec::with_capacity(config.sync.peer_addresses.len());
    let mut seen_ips: Vec<String> = Vec::with_capacity(config.sync.peer_addresses.len());

    for entry in &config.sync.peer_addresses {
        let (raw_host, port_opt) = clipboard_history_core::sync::split_host_and_port(entry);
        let clean_ip = clipboard_history_core::sync::clean_ip_address(&raw_host);
        if clean_ip.is_empty() || seen_ips.contains(&clean_ip) {
            continue;
        }
        seen_ips.push(clean_ip.clone());
        let port = match port_opt {
            Some(p) if p > 0 && (p == 54123 || p == 54124 || p == listen_port || p < 32768) => p,
            _ => 54123,
        };
        sanitized.push(clipboard_history_core::sync::format_peer_socket_addr(
            &clean_ip, port,
        ));
    }

    if sanitized != config.sync.peer_addresses {
        info!(
            "Sanitized sync.peer_addresses on startup: {:?} -> {:?}",
            config.sync.peer_addresses, sanitized
        );
        config.sync.peer_addresses = sanitized;
        true
    } else {
        false
    }
}

fn update_paired_peer_config(
    cfg: &mut AppConfig,
    clean_ip: &str,
    origin_device: &str,
    peer_listen_addr: &str,
    remote_pin: &str,
) {
    if !clean_ip.is_empty() && !cfg.sync.trusted_peers.iter().any(|p| p == clean_ip) {
        cfg.sync.trusted_peers.push(clean_ip.to_string());
    }
    if !origin_device.is_empty() && !cfg.sync.trusted_peers.iter().any(|p| p == origin_device) {
        cfg.sync.trusted_peers.push(origin_device.to_string());
    }
    cfg.sync
        .peer_addresses
        .retain(|p| clipboard_history_core::sync::clean_ip_address(p) != clean_ip);
    if !peer_listen_addr.is_empty() {
        cfg.sync.peer_addresses.push(peer_listen_addr.to_string());
    }
    if !remote_pin.is_empty() {
        if remote_pin != cfg.sync.pairing_pin {
            if !clean_ip.is_empty() {
                cfg.sync
                    .peer_pins
                    .insert(clean_ip.to_string(), remote_pin.to_string());
            }
            if !origin_device.is_empty() {
                cfg.sync
                    .peer_pins
                    .insert(origin_device.to_string(), remote_pin.to_string());
            }
        } else {
            if !clean_ip.is_empty() {
                cfg.sync.peer_pins.remove(clean_ip);
            }
            if !origin_device.is_empty() {
                cfg.sync.peer_pins.remove(origin_device);
            }
        }
    }
    cfg.sync.enabled = true;
}

fn is_recent_duplicate(
    recent: &mut std::collections::VecDeque<(String, std::time::Instant)>,
    hash: &str,
) -> bool {
    let ttl = std::time::Duration::from_millis(1500);
    while let Some((_, ts)) = recent.front() {
        if ts.elapsed() >= ttl {
            recent.pop_front();
        } else {
            break;
        }
    }
    recent.iter().any(|(h, _)| h == hash)
}

fn record_recent_capture(
    recent: &mut std::collections::VecDeque<(String, std::time::Instant)>,
    hash: String,
) {
    if recent.len() >= 64 {
        recent.pop_front();
    }
    recent.push_back((hash, std::time::Instant::now()));
}

async fn flush_micro_batch(
    buffer: &mut Vec<PendingBatchItem>,
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
    let entries: Vec<ClipboardEntry> = items.iter().map(|i| i.entry.clone()).collect();
    let repo_clone = repo.clone();

    let res = tokio::task::spawn_blocking(move || {
        let saved_items = repo_clone.batch_insert_or_update(&entries)?;
        for (saved, item) in saved_items.iter().zip(items.iter()) {
            if let Some(ref wrapped_dek) = item.wrapped_dek {
                if saved.id == item.orig_id {
                    let _ = repo_clone.insert_entry_dek(&saved.id, wrapped_dek, &[]);
                }
            }
        }
        Ok::<_, clipboard_history_core::error::CoreError>(saved_items)
    })
    .await;

    match res {
        Ok(Ok(saved_items)) => {
            debug!(
                "Committed micro-batch of {} items to SQLite",
                saved_items.len()
            );
            let current_cfg = config.read().await.clone();
            let active_peers = peer_tracker.get_all_peers().await;
            let mut seen_hashes = std::collections::HashSet::with_capacity(saved_items.len());
            for entry in saved_items {
                if !seen_hashes.insert(entry.content_hash.clone()) {
                    continue;
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_peer_addresses_strips_ephemeral_and_dedups() {
        let mut cfg = AppConfig::default();
        cfg.sync.listen_port = 54123;
        cfg.sync.peer_addresses = vec![
            "[::ffff:192.168.1.50]:49382".to_string(),
            "192.168.1.50:54123".to_string(),
            "192.168.1.60:54124".to_string(),
            "2001:db8::1".to_string(),
        ];
        assert!(sanitize_peer_addresses(&mut cfg));
        assert_eq!(
            cfg.sync.peer_addresses,
            vec![
                "192.168.1.50:54123",
                "192.168.1.60:54124",
                "[2001:db8::1]:54123",
            ]
        );
        // Second call is idempotent
        assert!(!sanitize_peer_addresses(&mut cfg));
    }

    #[test]
    fn test_update_paired_peer_config_normalizes_and_stores_pin() {
        let mut cfg = AppConfig::default();
        cfg.sync.pairing_pin = "111111".to_string();
        cfg.sync.peer_addresses = vec!["192.168.1.50:49382".to_string()];

        update_paired_peer_config(
            &mut cfg,
            "192.168.1.50",
            "cachyos-laptop",
            "192.168.1.50:54124",
            "999999",
        );

        assert!(cfg.sync.enabled);
        assert!(cfg.sync.trusted_peers.contains(&"192.168.1.50".to_string()));
        assert!(cfg
            .sync
            .trusted_peers
            .contains(&"cachyos-laptop".to_string()));
        assert_eq!(
            cfg.sync.peer_addresses,
            vec!["192.168.1.50:54124".to_string()]
        );
        assert_eq!(
            cfg.sync
                .pin_for_peer("192.168.1.50:54124", "cachyos-laptop"),
            "999999"
        );
    }

    #[test]
    fn test_recent_capture_dedup() {
        let mut recent = std::collections::VecDeque::new();
        assert!(!is_recent_duplicate(&mut recent, "hash1"));
        record_recent_capture(&mut recent, "hash1".to_string());
        assert!(is_recent_duplicate(&mut recent, "hash1"));
        assert!(!is_recent_duplicate(&mut recent, "hash2"));
    }
}
