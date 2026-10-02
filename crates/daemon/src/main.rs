mod circuit_breaker;
mod eviction;
mod server;
mod tray;
mod watcher;
mod window_focus;

use circuit_breaker::CircuitBreaker;
use eviction::EvictionWorker;
use server::DaemonServer;
use watcher::traits::{ClipboardWatcher, RawClipboardEvent};
use watcher::{WaylandWatcher, X11Watcher};
use window_focus::WindowFocusDetector;

use clap::Parser;
use clipboard_history_core::blob::{BlobStore, ThumbnailGenerator};
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::security::{PasswordManagerGuard, SecretFilter, SecretHandlingPolicy};
use clipboard_history_core::storage::SqliteRepository;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    // Spawn IPC Socket Server
    let socket_path = AppConfig::socket_path();
    let server = DaemonServer::new(
        repo.clone(),
        blob_store.clone(),
        Arc::clone(&config_arc),
        Arc::clone(&is_paused),
        socket_path.clone(),
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
    let is_wayland = session_type == "wayland" || wayland_display.is_some();

    info!(
        "Detected display environment: {} (session: {}, wayland_display: {:?})",
        if is_wayland { "Wayland" } else { "X11" },
        session_type,
        wayland_display
    );

    let (event_tx, mut event_rx) = mpsc::channel::<RawClipboardEvent>(32);

    // Spawn appropriate watcher
    if is_wayland {
        tokio::spawn(async move {
            let mut watcher = WaylandWatcher::new();
            if let Err(e) = watcher.run(event_tx).await {
                error!("Wayland watcher exited: {}", e);
            }
        });
    } else {
        tokio::spawn(async move {
            let mut watcher = X11Watcher::new();
            if let Err(e) = watcher.run(event_tx).await {
                error!("X11 watcher exited: {}", e);
            }
        });
    }

    // Spawn Shutdown Signal Listener
    let shutdown_socket = socket_path.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Received shutdown signal. Cleaning up...");
        if shutdown_socket.exists() {
            let _ = std::fs::remove_file(shutdown_socket);
        }
        std::process::exit(0);
    });

    // Spawn System Tray StatusNotifierItem Service
    if config.ui.tray_icon_enabled {
        let tray_paused = Arc::clone(&is_paused);
        tokio::spawn(async move {
            if let Err(e) = tray::start_tray_service(tray_paused).await {
                debug!("D-Bus Tray service could not be initialized: {}", e);
            }
        });
    }

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

        // 1. Incognito & Private Browsing Window Check (adjustable in settings)
        if current_cfg.security.ignore_incognito_windows
            && window_focus_detector
                .is_incognito_active(&current_cfg.security.incognito_window_patterns)
        {
            info!("Ignored clipboard copy originating while incognito/private browsing window was active");
            continue;
        }

        // 2. Password Manager & Sensitive MIME check (adjustable in settings)
        if current_cfg.security.ignore_password_managers
            && PasswordManagerGuard::is_sensitive_mime(&event.mime_types)
        {
            info!("Ignored clipboard entry flagged by password manager");
            continue;
        }

        // 3. Window Class Blacklist check
        if let Some(source) = &event.source_app {
            let lower_src = source.to_lowercase();
            if current_cfg
                .security
                .ignored_window_classes
                .iter()
                .any(|w| lower_src.contains(&w.to_lowercase()))
            {
                info!(
                    "Ignored clipboard entry from blacklisted window: {}",
                    source
                );
                continue;
            }
        }

        // 4. Process Text / HTML / UriList
        if let Some(mut text) = event.text {
            if text.trim().is_empty() {
                continue;
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

            let entry = if event.mime_types.iter().any(|m| m == "text/uri-list") {
                ClipboardEntry::new_uri_list(text, hash, event.mime_types, event.source_app)
            } else if let Some(html) = event.html {
                ClipboardEntry::new_html(text, html, hash, event.mime_types, event.source_app)
            } else {
                ClipboardEntry::new_text(text, hash, event.mime_types, event.source_app)
            };

            if let Err(e) = repo.insert_or_update(&entry) {
                error!("Failed to save clipboard entry: {}", e);
            } else {
                debug!(
                    "Captured clipboard entry: [{}] {}",
                    entry.entry_type, entry.preview
                );
            }
        } else if let Some(image_bytes) = event.image_data {
            if !circuit_breaker.allow_payload_size(image_bytes.len()) {
                continue;
            }

            let blob_hash = match blob_store.save(&image_bytes) {
                Ok(h) => h,
                Err(e) => {
                    error!("Failed to store image blob: {}", e);
                    continue;
                }
            };

            // Generate thumbnail
            let (dimensions, thumb_hash) = match ThumbnailGenerator::generate(&image_bytes) {
                Ok((dims, thumb_bytes)) => {
                    let th = blob_store.save(&thumb_bytes).ok();
                    (dims, th)
                }
                Err(_) => ((0, 0), None),
            };

            let entry = ClipboardEntry::new_image(
                blob_hash,
                thumb_hash,
                image_bytes.len(),
                dimensions,
                event.mime_types,
                event.source_app,
            );

            if let Err(e) = repo.insert_or_update(&entry) {
                error!("Failed to save image entry: {}", e);
            } else {
                debug!("Captured clipboard image entry: {}", entry.preview);
            }
        }
    }

    Ok(())
}
