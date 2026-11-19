use clipboard_history_core::blob::{BlobStore, ThumbnailGenerator};
use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use clipboard_history_core::error::Result;
use clipboard_history_core::security::CryptoEngine;
use clipboard_history_core::storage::SqliteRepository;
use clipboard_history_core::sync::{
    build_candidate_sync_addrs, clean_ip_address, read_sync_message, read_sync_message_with_pins,
    send_sync_message, split_host_and_port, SyncMessage,
};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Decrypt entry text/html if encrypted at rest, so it can be sent plain in transit
pub fn decrypt_entry_for_transit(
    entry: &mut ClipboardEntry,
    repo: &SqliteRepository,
    crypto: &CryptoEngine,
) {
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

/// Reads the image blob bytes for an entry (decrypting if at-rest encryption was active)
pub fn read_image_blob(
    entry: &ClipboardEntry,
    blob_store: &BlobStore,
    crypto: &CryptoEngine,
    encryption_enabled: bool,
) -> Option<Vec<u8>> {
    let hash = entry.blob_hash.as_ref()?;
    if encryption_enabled {
        blob_store
            .read_decrypted(hash, crypto)
            .ok()
            .or_else(|| blob_store.read(hash).ok())
    } else {
        blob_store
            .read(hash)
            .ok()
            .or_else(|| blob_store.read_decrypted(hash, crypto).ok())
    }
}

/// Reads or generates the thumbnail blob bytes for an entry
pub fn read_thumbnail_blob(
    entry: &ClipboardEntry,
    full_image_bytes: Option<&[u8]>,
    blob_store: &BlobStore,
    crypto: &CryptoEngine,
    encryption_enabled: bool,
) -> Option<Vec<u8>> {
    if let Some(ref thumb_hash) = entry.thumbnail_blob_hash {
        let bytes_opt = if encryption_enabled {
            blob_store
                .read_decrypted(thumb_hash, crypto)
                .ok()
                .or_else(|| blob_store.read(thumb_hash).ok())
        } else {
            blob_store
                .read(thumb_hash)
                .ok()
                .or_else(|| blob_store.read_decrypted(thumb_hash, crypto).ok())
        };
        if bytes_opt.is_some() {
            return bytes_opt;
        }
    }

    // Fall back to generating thumbnail on the fly if full image bytes are available
    if let Some(full_bytes) = full_image_bytes {
        if let Ok((_, thumb_bytes)) = ThumbnailGenerator::generate(full_bytes) {
            return Some(thumb_bytes);
        }
    }

    None
}

/// Streams local history entries from oldest to newest to a connected peer
#[allow(clippy::too_many_arguments)]
pub async fn stream_history_to_writer<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    pin: &str,
    my_dev_name: &str,
    repo: &SqliteRepository,
    blob_store: &BlobStore,
    crypto: &CryptoEngine,
    encryption_enabled: bool,
    limit: usize,
) -> Result<usize> {
    let entries = match repo.list(limit, 0, None, false) {
        Ok(list) => list,
        Err(e) => {
            warn!("Failed to query entries for history sync: {}", e);
            return Ok(0);
        }
    };

    let total = entries.len();
    // Reverse so oldest entries are sent first, newest arrives last
    let mut sent_count = 0;
    for mut entry in entries.into_iter().rev() {
        decrypt_entry_for_transit(&mut entry, repo, crypto);

        let (blob_payload, thumbnail_payload) = if entry.entry_type == EntryType::Image {
            let b = read_image_blob(&entry, blob_store, crypto, encryption_enabled);
            let t =
                read_thumbnail_blob(&entry, b.as_deref(), blob_store, crypto, encryption_enabled);
            (b, t)
        } else {
            (None, None)
        };

        let msg = SyncMessage::EntrySync {
            origin_device: my_dev_name.to_string(),
            entry: Box::new(entry),
            blob_payload,
            thumbnail_payload,
            is_history_batch: true,
        };

        match tokio::time::timeout(Duration::from_secs(5), send_sync_message(writer, pin, &msg))
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                warn!(
                    "History sync aborted early after {}/{} entries: {}",
                    sent_count, total, e
                );
                break;
            }
            Err(_) => {
                warn!(
                    "History sync timed out after {}/{} entries",
                    sent_count, total
                );
                break;
            }
        }
        sent_count += 1;
    }

    let complete_msg = SyncMessage::HistorySyncComplete {
        origin_device: my_dev_name.to_string(),
        count: sent_count,
    };
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        send_sync_message(writer, pin, &complete_msg),
    )
    .await;

    debug!("Streamed {} history entries to peer", sent_count);
    Ok(sent_count)
}

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Thread-safe tracker that prevents locally injected sync entries from echoing back to the network.
/// Keeps recent content hashes for a 20-second sliding window, allowing both Wayland and X11
/// watchers to suppress the echo without clearing the hash on first read.
#[derive(Clone, Default)]
pub struct SyncedEchoTracker {
    recent_hashes: Arc<RwLock<VecDeque<(String, Instant)>>>,
}

impl SyncedEchoTracker {
    pub fn new() -> Self {
        Self {
            recent_hashes: Arc::new(RwLock::new(VecDeque::with_capacity(128))),
        }
    }

    /// Record a hash of newly received content that is being injected into the system clipboard.
    pub async fn record_synced_hash(&self, hash: String) {
        let mut list = self.recent_hashes.write().await;
        let now = Instant::now();
        list.retain(|(_, time)| time.elapsed() < Duration::from_secs(20));
        if !list.iter().any(|(h, _)| h == &hash) {
            if list.len() >= 128 {
                list.pop_front();
            }
            list.push_back((hash, now));
        }
    }

    /// Alias for record_synced_hash for local paste and sync echo tracking
    pub async fn record_echo(&self, hash: String) {
        self.record_synced_hash(hash).await;
    }

    /// Checks if a given content hash was recently injected from network sync.
    /// Does NOT remove it, allowing both Wayland and X11 watchers to detect and suppress it.
    pub async fn is_echo(&self, hash: &str) -> bool {
        let mut list = self.recent_hashes.write().await;
        list.retain(|(_, time)| time.elapsed() < Duration::from_secs(20));
        list.iter().any(|(h, _)| h == hash)
    }
}

fn encrypt_entry_strings_fallback(entry: &mut ClipboardEntry, crypto: &CryptoEngine) {
    entry.preview = crypto
        .encrypt_str(&entry.preview)
        .unwrap_or_else(|_| entry.preview.clone());
    if let Some(ref text) = entry.text_content {
        entry.text_content = crypto.encrypt_str(text).ok().or(entry.text_content.clone());
    }
    if let Some(ref html) = entry.html_content {
        entry.html_content = crypto.encrypt_str(html).ok().or(entry.html_content.clone());
    }
}

/// Handles an incoming EntrySync message: saves blobs/thumbnails, updates DB,
/// and optionally copies to system clipboard if not a history batch.
#[allow(clippy::too_many_arguments)]
pub async fn handle_incoming_entry_sync(
    mut entry: ClipboardEntry,
    blob_payload: Option<Vec<u8>>,
    thumbnail_payload: Option<Vec<u8>>,
    is_history_batch: bool,
    repo: &SqliteRepository,
    blob_store: &BlobStore,
    crypto: &CryptoEngine,
    encryption_enabled: bool,
    echo_tracker: &SyncedEchoTracker,
) -> Result<()> {
    if entry.entry_type == EntryType::Image {
        let mut saved_image_bytes: Option<Vec<u8>> = None;

        if let Some(bytes) = blob_payload {
            let plain_hash = BlobStore::compute_hash(&bytes);
            let h = if encryption_enabled {
                blob_store.save_encrypted(&bytes, crypto).ok()
            } else {
                blob_store.save(&bytes).ok()
            };
            if let Some(blob_h) = h {
                entry.blob_hash = Some(blob_h);
                // Keep content_hash as deterministic plaintext hash for deduplication
                entry.content_hash = plain_hash;
            }
            saved_image_bytes = Some(bytes);
        }

        if let Some(thumb_bytes) = thumbnail_payload {
            let th = if encryption_enabled {
                blob_store.save_encrypted(&thumb_bytes, crypto).ok()
            } else {
                blob_store.save(&thumb_bytes).ok()
            };
            entry.thumbnail_blob_hash = th;
        } else if let Some(ref full_bytes) = saved_image_bytes {
            // Generate thumbnail locally if missing from sync
            if let Ok((_, thumb_bytes)) = ThumbnailGenerator::generate(full_bytes) {
                let th = if encryption_enabled {
                    blob_store.save_encrypted(&thumb_bytes, crypto).ok()
                } else {
                    blob_store.save(&thumb_bytes).ok()
                };
                entry.thumbnail_blob_hash = th;
            }
        }

        if let Err(e) = repo.insert_or_update(&entry) {
            warn!(
                "Failed to store incoming synced image entry {}: {}",
                entry.id, e
            );
        }

        // Inject into OS system clipboard if real-time single copy event
        if !is_history_batch {
            if let Some(bytes) = saved_image_bytes {
                let hash = BlobStore::compute_hash(&bytes);
                echo_tracker.record_synced_hash(hash).await;
                let mime = entry
                    .mime_types
                    .iter()
                    .find(|m| m.starts_with("image/"))
                    .map(|s| s.as_str());
                let mime_owned = mime.map(|s| s.to_string());
                tokio::spawn(async move {
                    clipboard_history_core::clipboard::copy_image_to_system_clipboard(
                        &bytes,
                        mime_owned.as_deref(),
                    )
                    .await;
                });
            }
        }
    } else {
        // Text / HTML / Code / UriList
        let plain_text_for_clipboard = entry.text_content.clone();
        if encryption_enabled {
            let orig_id = entry.id.clone();
            let already_exists = repo
                .get_by_hash(&entry.content_hash)
                .ok()
                .flatten()
                .is_some();
            if !already_exists {
                if let Ok(wrapped_dek) = crypto.encrypt_entry_envelope(&mut entry) {
                    match repo.insert_or_update(&entry) {
                        Ok(saved) => {
                            if saved.id == orig_id {
                                if let Err(e) = repo.insert_entry_dek(&saved.id, &wrapped_dek, &[])
                                {
                                    warn!(
                                        "Failed to store DEK for synced entry {}: {}",
                                        saved.id, e
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to store incoming synced entry {}: {}", orig_id, e);
                        }
                    }
                } else {
                    encrypt_entry_strings_fallback(&mut entry, crypto);
                    if let Err(e) = repo.insert_or_update(&entry) {
                        warn!("Failed to store incoming synced entry {}: {}", entry.id, e);
                    }
                }
            } else {
                encrypt_entry_strings_fallback(&mut entry, crypto);
                if let Err(e) = repo.insert_or_update(&entry) {
                    warn!("Failed to store incoming synced entry {}: {}", entry.id, e);
                }
            }
        } else if let Err(e) = repo.insert_or_update(&entry) {
            warn!("Failed to store incoming synced entry {}: {}", entry.id, e);
        }

        // Inject into OS system clipboard if real-time single copy event
        if !is_history_batch {
            if let Some(plain_text) = plain_text_for_clipboard {
                let content_hash = BlobStore::compute_hash(plain_text.as_bytes());
                echo_tracker.record_synced_hash(content_hash).await;
                tokio::spawn(async move {
                    clipboard_history_core::copy_to_system_clipboard(&plain_text).await;
                });
            }
        }
    }

    Ok(())
}

/// Performs a complete mutual two-way history synchronization with a peer.
#[allow(clippy::too_many_arguments)]
pub async fn perform_mutual_history_sync(
    peer_address: &str,
    pin: &str,
    my_dev_name: &str,
    repo: SqliteRepository,
    blob_store: BlobStore,
    crypto: Arc<CryptoEngine>,
    encryption_enabled: bool,
    max_entries: usize,
    echo_tracker: SyncedEchoTracker,
) -> bool {
    let candidate_addrs = build_candidate_sync_addrs(peer_address, 54123);

    let mut connected_stream = None;
    for addr in &candidate_addrs {
        if let Ok(Ok(stream)) =
            tokio::time::timeout(Duration::from_secs(3), tokio::net::TcpStream::connect(addr)).await
        {
            connected_stream = Some(stream);
            break;
        }
    }

    let mut stream = match connected_stream {
        Some(s) => s,
        None => {
            warn!(
                "Could not connect to peer {} for mutual history sync (candidates: {:?})",
                peer_address, candidate_addrs
            );
            return false;
        }
    };

    let (mut reader, mut writer) = stream.split();

    // 1. Authenticate
    let auth_req = SyncMessage::AuthRequest {
        device_name: my_dev_name.to_string(),
        pairing_pin: pin.to_string(),
        nonce: rand::random(),
    };
    match tokio::time::timeout(
        Duration::from_secs(5),
        send_sync_message(&mut writer, pin, &auth_req),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            warn!(
                "Failed to send AuthRequest for mutual history sync to {}: {}",
                peer_address, e
            );
            let _ = writer.shutdown().await;
            return false;
        }
        Err(_) => {
            warn!(
                "Timed out sending AuthRequest for mutual history sync to {}",
                peer_address
            );
            let _ = writer.shutdown().await;
            return false;
        }
    }
    match tokio::time::timeout(Duration::from_secs(5), read_sync_message(&mut reader, pin)).await {
        Ok(Ok(SyncMessage::AuthResponse { success: true, .. })) => {}
        Ok(Ok(SyncMessage::AuthResponse {
            success: false,
            message,
            ..
        })) => {
            warn!(
                "Mutual history sync auth rejected by {}: {}",
                peer_address, message
            );
            let _ = writer.shutdown().await;
            return false;
        }
        Ok(Ok(other)) => {
            warn!(
                "Unexpected auth response during mutual history sync with {}: {:?}",
                peer_address, other
            );
            let _ = writer.shutdown().await;
            return false;
        }
        Ok(Err(e)) => {
            warn!(
                "Failed to read/decrypt AuthResponse during mutual history sync with {}: {}",
                peer_address, e
            );
            let _ = writer.shutdown().await;
            return false;
        }
        Err(_) => {
            warn!(
                "Timed out waiting for AuthResponse during mutual history sync with {}",
                peer_address
            );
            let _ = writer.shutdown().await;
            return false;
        }
    }

    info!("Starting mutual history sync with peer {}", peer_address);

    // 2. Stream our history entries to the peer
    let _ = stream_history_to_writer(
        &mut writer,
        pin,
        my_dev_name,
        &repo,
        &blob_store,
        &crypto,
        encryption_enabled,
        max_entries,
    )
    .await;

    // 3. Request peer's history entries
    let sync_req = SyncMessage::HistorySyncRequest {
        origin_device: my_dev_name.to_string(),
        limit: max_entries,
    };
    match tokio::time::timeout(
        Duration::from_secs(5),
        send_sync_message(&mut writer, pin, &sync_req),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            warn!(
                "Failed to send HistorySyncRequest to {}: {}",
                peer_address, e
            );
            let _ = writer.shutdown().await;
            return false;
        }
        Err(_) => {
            warn!("Timed out sending HistorySyncRequest to {}", peer_address);
            let _ = writer.shutdown().await;
            return false;
        }
    }

    // 4. Receive peer's history entries until HistorySyncComplete or timeout
    let mut received_count = 0;
    loop {
        match tokio::time::timeout(Duration::from_secs(10), read_sync_message(&mut reader, pin))
            .await
        {
            Ok(Ok(msg)) => match msg {
                SyncMessage::EntrySync {
                    entry,
                    blob_payload,
                    thumbnail_payload,
                    is_history_batch,
                    ..
                } => {
                    let _ = handle_incoming_entry_sync(
                        *entry,
                        blob_payload,
                        thumbnail_payload,
                        is_history_batch,
                        &repo,
                        &blob_store,
                        &crypto,
                        encryption_enabled,
                        &echo_tracker,
                    )
                    .await;
                    received_count += 1;
                }
                SyncMessage::PinSync {
                    content_hash,
                    is_pinned,
                    entry,
                    blob_payload,
                    thumbnail_payload,
                    ..
                } => {
                    if is_pinned {
                        let res = repo.set_pinned_by_hash(&content_hash, true);
                        if res.is_err() {
                            if let Some(entry_box) = entry {
                                let _ = handle_incoming_entry_sync(
                                    *entry_box,
                                    blob_payload,
                                    thumbnail_payload,
                                    true,
                                    &repo,
                                    &blob_store,
                                    &crypto,
                                    encryption_enabled,
                                    &echo_tracker,
                                )
                                .await;
                                let _ = repo.set_pinned_by_hash(&content_hash, true);
                            }
                        }
                    } else {
                        let _ = repo.set_pinned_by_hash(&content_hash, false);
                    }
                }
                SyncMessage::HistorySyncComplete { count, .. } => {
                    debug!(
                        "Peer completed history sync ({} reported, {} received)",
                        count, received_count
                    );
                    break;
                }
                _ => {}
            },
            Ok(Err(e)) => {
                warn!(
                    "Protocol error receiving history sync from {}: {}",
                    peer_address, e
                );
                break;
            }
            Err(_) => {
                warn!(
                    "Timed out waiting for history sync entries from {}",
                    peer_address
                );
                break;
            }
        }
    }

    let _ = writer.shutdown().await;

    // 5. Retain capacity limit
    let _ = repo.evict_capacity(max_entries);
    info!(
        "Mutual history sync with {} finished: received {} entries, capped at {}",
        peer_address, received_count, max_entries
    );
    true
}

/// Helper to extract host/IP without port (handles bracketed IPv6 and bare IPv6 safely).
pub fn extract_host(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return &rest[..end];
        }
    }
    if trimmed.parse::<std::net::IpAddr>().is_ok() {
        return trimmed;
    }
    if let Some((prefix, port_str)) = trimmed.rsplit_once(':') {
        if port_str.parse::<u16>().is_ok()
            && (!prefix.contains(':') || prefix.parse::<std::net::IpAddr>().is_ok())
        {
            return prefix;
        }
    }
    trimmed
}

/// Broadcasts a SyncMessage (e.g. EntrySync or ClearHistorySync) to all active, trusted, or configured peers.
pub fn broadcast_sync_message(
    cfg: &clipboard_history_core::config::SyncConfig,
    known_peers: &[(String, String)],
    msg: SyncMessage,
) {
    if !cfg.enabled {
        return;
    }

    let mut targets: Vec<(String, String)> = Vec::new();
    let mut seen_clean_hosts: Vec<String> = Vec::new();
    let mut add_target = |addr: &str, dev_name: &str| {
        let host = clean_ip_address(addr);
        if host.is_empty() || seen_clean_hosts.contains(&host) {
            return;
        }
        seen_clean_hosts.push(host);
        targets.push((addr.to_string(), dev_name.to_string()));
    };

    // 1. Add active peers known from peer_tracker if trusted (preferring non-ephemeral ports first)
    let mut non_ephemeral_known: Vec<(&str, &str)> = Vec::new();
    let mut other_known: Vec<(&str, &str)> = Vec::new();
    for (addr, dev_name) in known_peers {
        if cfg.is_peer_trusted(addr, dev_name) {
            let (_, port_opt) = split_host_and_port(addr);
            let is_listening = port_opt.is_some_and(|p| {
                p > 0 && (p == cfg.listen_port || p == 54123 || p == 54124 || p < 32768)
            });
            if is_listening {
                non_ephemeral_known.push((addr.as_str(), dev_name.as_str()));
            } else {
                other_known.push((addr.as_str(), dev_name.as_str()));
            }
        }
    }
    for (addr, dev_name) in non_ephemeral_known.into_iter().chain(other_known) {
        add_target(addr, dev_name);
    }

    // 2. Add configured manual peer addresses
    for peer in &cfg.peer_addresses {
        add_target(peer, "");
    }

    // 3. Add any IP addresses directly in trusted_peers
    for peer in &cfg.trusted_peers {
        if peer.contains('.') || peer.contains(':') {
            add_target(peer, "");
        }
    }

    if targets.is_empty() {
        return;
    }

    let msg_name = match &msg {
        SyncMessage::ClearHistorySync { .. } => "ClearHistorySync",
        SyncMessage::PinSync { .. } => "PinSync",
        SyncMessage::EntrySync { entry, .. } => {
            debug!("Broadcasting entry type: {}", entry.entry_type);
            "EntrySync"
        }
        _ => "SyncMessage",
    };

    info!(
        "Broadcasting {} to {} peer target(s): {:?}",
        msg_name,
        targets.len(),
        targets
    );

    for (peer, dev_name) in targets {
        let pin = cfg.pin_for_peer(&peer, &dev_name);
        let default_pin = cfg.pairing_pin.clone();
        let listen_port = cfg.listen_port;
        let my_dev_name = cfg.device_name.clone();
        let msg = msg.clone();
        tokio::spawn(async move {
            let candidate_peers = build_candidate_sync_addrs(&peer, listen_port);
            let mut delivered = false;
            let mut aborted_by_auth = false;

            for candidate in &candidate_peers {
                let connect_fut = tokio::net::TcpStream::connect(candidate);
                match tokio::time::timeout(Duration::from_secs(3), connect_fut).await {
                    Ok(Ok(mut stream)) => {
                        let (mut reader, mut writer) = stream.split();
                        // 1. Handshake with AuthRequest
                        let auth_req = SyncMessage::AuthRequest {
                            device_name: my_dev_name.clone(),
                            pairing_pin: pin.clone(),
                            nonce: rand::random(),
                        };
                        match tokio::time::timeout(
                            Duration::from_secs(5),
                            send_sync_message(&mut writer, &pin, &auth_req),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => {
                                warn!("Failed to send AuthRequest to {}: {}", candidate, e);
                                let _ = writer.shutdown().await;
                                continue;
                            }
                            Err(_) => {
                                warn!("Timed out sending AuthRequest to {}", candidate);
                                let _ = writer.shutdown().await;
                                continue;
                            }
                        }
                        // 2. Await AuthResponse
                        let auth_res = tokio::time::timeout(
                            Duration::from_secs(5),
                            read_sync_message_with_pins(&mut reader, &[pin.as_str()], &default_pin),
                        )
                        .await;
                        match auth_res {
                            Ok(Ok((
                                SyncMessage::AuthResponse { success: true, .. },
                                session_pin,
                            ))) => {
                                // 3. Send message
                                match tokio::time::timeout(
                                    Duration::from_secs(5),
                                    send_sync_message(&mut writer, &session_pin, &msg),
                                )
                                .await
                                {
                                    Ok(Ok(())) => {
                                        let _ = writer.shutdown().await;
                                        info!(
                                            "Successfully synchronized {} to peer at {}",
                                            msg_name, candidate
                                        );
                                        delivered = true;
                                        break;
                                    }
                                    Ok(Err(e)) => {
                                        warn!(
                                            "Failed to send {} to peer at {}: {}",
                                            msg_name, candidate, e
                                        );
                                        let _ = writer.shutdown().await;
                                    }
                                    Err(_) => {
                                        warn!(
                                            "Timed out sending {} to peer at {}",
                                            msg_name, candidate
                                        );
                                        let _ = writer.shutdown().await;
                                    }
                                }
                            }
                            Ok(Ok((
                                SyncMessage::AuthResponse {
                                    success: false,
                                    message,
                                    ..
                                },
                                _,
                            ))) => {
                                warn!(
                                    "Sync authentication rejected by peer at {}: {}",
                                    candidate, message
                                );
                                let _ = writer.shutdown().await;
                                aborted_by_auth = true;
                                break;
                            }
                            Ok(Ok((other, _))) => {
                                warn!(
                                    "Unexpected AuthResponse variant from peer at {}: {:?}",
                                    candidate, other
                                );
                                let _ = writer.shutdown().await;
                            }
                            Ok(Err(e)) => {
                                warn!(
                                    "Failed to decrypt/read AuthResponse from peer at {}: {}",
                                    candidate, e
                                );
                                let _ = writer.shutdown().await;
                            }
                            Err(_) => {
                                warn!(
                                    "Timed out waiting for AuthResponse from peer at {}",
                                    candidate
                                );
                                let _ = writer.shutdown().await;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        debug!("Connect failed to sync candidate {}: {}", candidate, e);
                    }
                    Err(_) => {
                        debug!("Connect timed out to sync candidate {}", candidate);
                    }
                }
            }

            if !delivered && !aborted_by_auth {
                warn!(
                    "Failed to broadcast {} to peer {} across candidates {:?}",
                    msg_name, peer, candidate_peers
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_synced_echo_tracker() {
        let tracker = SyncedEchoTracker::new();

        assert!(!tracker.is_echo("sample_hash_1").await);

        tracker
            .record_synced_hash("sample_hash_1".to_string())
            .await;

        // Both Wayland and X11 watchers should detect the echo
        assert!(tracker.is_echo("sample_hash_1").await);
        assert!(tracker.is_echo("sample_hash_1").await);

        // Different hash should not be an echo
        assert!(!tracker.is_echo("sample_hash_2").await);

        // Record another
        tracker
            .record_synced_hash("sample_hash_2".to_string())
            .await;
        assert!(tracker.is_echo("sample_hash_2").await);
        assert!(tracker.is_echo("sample_hash_1").await);
    }

    #[tokio::test]
    async fn test_broadcast_sync_message_noop_when_disabled() {
        let cfg = clipboard_history_core::config::SyncConfig {
            enabled: false,
            ..Default::default()
        };
        let msg = SyncMessage::ClearHistorySync {
            origin_device: "test-dev".to_string(),
            include_pinned: true,
        };
        // Should safely no-op without panicking
        broadcast_sync_message(&cfg, &[], msg);
    }

    #[test]
    fn test_extract_host_variants() {
        assert_eq!(extract_host("192.168.1.50:54123"), "192.168.1.50");
        assert_eq!(extract_host("192.168.1.50"), "192.168.1.50");
        assert_eq!(
            extract_host("[::ffff:192.168.1.50]:49382"),
            "::ffff:192.168.1.50"
        );
        assert_eq!(extract_host("[2001:db8::1]:54123"), "2001:db8::1");
        assert_eq!(extract_host("2001:db8::1"), "2001:db8::1");
        assert_eq!(
            extract_host("::ffff:192.168.1.50:54123"),
            "::ffff:192.168.1.50"
        );
        assert_eq!(extract_host("cachyos-laptop:54123"), "cachyos-laptop");
    }
}
