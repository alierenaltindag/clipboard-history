use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::storage::SqliteRepository;
use clipboard_history_core::sync::{
    build_candidate_sync_addrs, clean_ip_address, format_peer_socket_addr, normalize_ip,
    read_sync_message_with_pins, send_sync_message, split_host_and_port, SyncMessage,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub device_name: String,
    pub address: String,
    pub last_seen: Instant,
}

#[derive(Clone, Default)]
pub struct PeerTracker {
    peers: Arc<RwLock<HashMap<String, DiscoveredPeer>>>,
    scan_notify: Arc<tokio::sync::Notify>,
}

impl PeerTracker {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            scan_notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub async fn trigger_scan(&self) {
        self.scan_notify.notify_waiters();
    }

    pub async fn add_or_update(&self, address: String, device_name: String) -> bool {
        let mut map = self.peers.write().await;
        let is_new = !map.contains_key(&address);
        map.insert(
            address.clone(),
            DiscoveredPeer {
                device_name,
                address,
                last_seen: Instant::now(),
            },
        );
        is_new
    }

    pub async fn cleanup_stale(&self, timeout: Duration) {
        let mut map = self.peers.write().await;
        map.retain(|_, peer| peer.last_seen.elapsed() < timeout);
    }

    #[allow(dead_code)]
    pub async fn get_peer_addresses(&self) -> Vec<String> {
        let map = self.peers.read().await;
        map.keys().cloned().collect()
    }

    pub async fn get_peer_descriptions(&self) -> Vec<String> {
        let map = self.peers.read().await;
        map.values()
            .map(|p| format!("{} ({})", p.device_name, p.address))
            .collect()
    }

    pub async fn get_all_peers(&self) -> Vec<(String, String)> {
        let map = self.peers.read().await;
        map.values()
            .map(|p| (p.address.clone(), p.device_name.clone()))
            .collect()
    }

    pub async fn get_port_for_ip(&self, clean_ip: &str) -> Option<u16> {
        let target_ip = clean_ip_address(clean_ip);
        if target_ip.is_empty() {
            return None;
        }
        let map = self.peers.read().await;
        let mut best: Option<(u16, Instant)> = None;
        for peer in map.values() {
            let (host, port_opt) = split_host_and_port(&peer.address);
            if clean_ip_address(&host) == target_ip {
                if let Some(port) = port_opt {
                    let is_listening = port == 54123 || port == 54124 || (port > 0 && port < 32768);
                    if is_listening {
                        match best {
                            Some((_, best_seen)) if best_seen >= peer.last_seen => {}
                            _ => best = Some((port, peer.last_seen)),
                        }
                    }
                }
            }
        }
        best.map(|(p, _)| p)
    }

    pub async fn is_recently_seen(&self, clean_ip: &str, within: Duration) -> bool {
        let target_ip = clean_ip_address(clean_ip);
        if target_ip.is_empty() {
            return false;
        }
        let map = self.peers.read().await;
        map.values().any(|peer| {
            clean_ip_address(&peer.address) == target_ip && peer.last_seen.elapsed() < within
        })
    }

    pub async fn remove_peer(&self, identifier: &str) {
        let clean_id = clean_ip_address(identifier);
        let mut map = self.peers.write().await;
        map.retain(|addr, peer| {
            let clean_addr = clean_ip_address(addr);
            addr != identifier
                && (clean_id.is_empty() || clean_addr != clean_id)
                && peer.device_name != identifier
                && (clean_id.is_empty() || peer.device_name != clean_id)
        });
    }

    pub async fn find_aliases(&self, identifier: &str) -> (Vec<String>, Vec<String>) {
        let clean_id = clean_ip_address(identifier);
        let map = self.peers.read().await;
        let mut addrs = Vec::new();
        let mut names = Vec::new();
        for (addr, peer) in map.iter() {
            let clean_addr = clean_ip_address(addr);
            if (!identifier.is_empty() && (addr == identifier || peer.device_name == identifier))
                || (!clean_id.is_empty()
                    && (clean_addr == clean_id || peer.device_name == clean_id))
            {
                addrs.push(addr.clone());
                addrs.push(clean_addr);
                names.push(peer.device_name.clone());
            }
        }
        (addrs, names)
    }
}

/// Discovers active broadcast targets including universal 255.255.255.255, interface subnet broadcasts, and ARP neighbors
pub fn get_broadcast_addresses(port: u16) -> Vec<SocketAddr> {
    let mut targets = vec![SocketAddr::from(([255, 255, 255, 255], port))];

    // Read interface broadcast addresses via libc::getifaddrs
    unsafe {
        let mut ifaddrs: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifaddrs) == 0 && !ifaddrs.is_null() {
            let mut curr = ifaddrs;
            while !curr.is_null() {
                let ifa = *curr;
                if !ifa.ifa_addr.is_null() && !ifa.ifa_ifu.is_null() {
                    let family = (*ifa.ifa_addr).sa_family as i32;
                    let flags = ifa.ifa_flags as i32;
                    // IPv4, UP, BROADCAST, not LOOPBACK
                    if family == libc::AF_INET
                        && (flags & libc::IFF_UP != 0)
                        && (flags & libc::IFF_BROADCAST != 0)
                        && (flags & libc::IFF_LOOPBACK == 0)
                    {
                        let sock_in = ifa.ifa_ifu as *const libc::sockaddr_in;
                        let ip_bytes = (*sock_in).sin_addr.s_addr.to_ne_bytes();
                        let ip = std::net::Ipv4Addr::from(ip_bytes);
                        let sa = SocketAddr::from((ip, port));
                        if !targets.contains(&sa) {
                            targets.push(sa);
                        }
                    }
                }
                curr = ifa.ifa_next;
            }
            libc::freeifaddrs(ifaddrs);
        }
    }

    // Also include any active ARP neighbors from /proc/net/arp on non-virtual interfaces
    if let Ok(content) = std::fs::read_to_string("/proc/net/arp") {
        for line in content.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                let dev = parts[5];
                if !dev.starts_with("docker") && !dev.starts_with("br-") && !dev.starts_with("veth")
                {
                    if let Ok(ip) = parts[0].parse::<std::net::Ipv4Addr>() {
                        let sa = SocketAddr::from((ip, port));
                        if !targets.contains(&sa) {
                            targets.push(sa);
                        }
                    }
                }
            }
        }
    }

    targets
}

pub fn get_local_ips() -> Vec<std::net::Ipv4Addr> {
    clipboard_history_core::sync::get_local_ips()
}

#[allow(clippy::too_many_arguments)]
pub async fn start_udp_discovery(
    listen_port: u16,
    peer_tracker: PeerTracker,
    sync_cfg_arc: Arc<RwLock<AppConfig>>,
    sync_repo: SqliteRepository,
    sync_blob_store: BlobStore,
    crypto: Arc<clipboard_history_core::security::CryptoEngine>,
    echo_tracker: crate::sync_service::SyncedEchoTracker,
) {
    let bind_addr = format!("0.0.0.0:{}", listen_port);
    let socket = match UdpSocket::bind(&bind_addr).await {
        Ok(s) => {
            let _ = s.set_broadcast(true);
            Arc::new(s)
        }
        Err(e) => {
            warn!(
                "Could not bind UDP discovery socket on {}: {}",
                bind_addr, e
            );
            return;
        }
    };

    info!("P2P UDP Peer Discovery listening on {}", bind_addr);

    let my_instance_id: u64 = rand::random();
    let my_instance_str = my_instance_id.to_string();

    // 1. Inbound Discovery Receiver Task
    let recv_socket = socket.clone();
    let recv_tracker = peer_tracker.clone();
    let recv_cfg_arc = sync_cfg_arc.clone();
    let recv_repo = sync_repo.clone();
    let recv_blob = sync_blob_store.clone();
    let recv_crypto = crypto.clone();
    let recv_echo_tracker = echo_tracker.clone();
    let my_instance_filter = my_instance_str.clone();

    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        let mut local_ips = get_local_ips();
        let mut last_ip_refresh = Instant::now();

        while let Ok((len, src_addr)) = recv_socket.recv_from(&mut buf).await {
            if last_ip_refresh.elapsed() >= Duration::from_secs(30) {
                local_ips = get_local_ips();
                last_ip_refresh = Instant::now();
            }

            let text = String::from_utf8_lossy(&buf[..len]);
            let parts: Vec<&str> = text.trim().split(':').collect();

            // Format: CH_BEACON:v2:<device_name>:<tcp_port>:<instance_id>
            // or:     CH_ACK:v2:<device_name>:<tcp_port>:<instance_id>
            // Legacy: CH_BEACON:v1:<device_name>:<tcp_port>
            if parts.len() >= 4 && (parts[0] == "CH_BEACON" || parts[0] == "CH_ACK") {
                // Drop if beacon is a broadcast loopback from ourselves
                let normalized_src_ip = normalize_ip(src_addr.ip());
                let is_from_own_ip = normalized_src_ip == "::1"
                    || local_ips
                        .iter()
                        .any(|ipv4| ipv4.to_string() == normalized_src_ip);

                let matches_own_instance = parts.len() >= 5 && parts[4] == my_instance_filter;

                if is_from_own_ip || matches_own_instance {
                    continue;
                }

                let peer_dev_name = parts[2].trim().to_string();
                let peer_port: u16 = parts[3].trim().parse().unwrap_or(listen_port);
                let peer_tcp_addr = format_peer_socket_addr(&normalized_src_ip, peer_port);

                let my_cfg = recv_cfg_arc.read().await.sync.clone();
                if !my_cfg.enabled {
                    continue;
                }

                // If peer has identical configured name as ours, disambiguate with its IP
                let display_name = if peer_dev_name == my_cfg.device_name {
                    format!("{} ({})", peer_dev_name, normalized_src_ip)
                } else {
                    peer_dev_name.clone()
                };

                let is_new = recv_tracker
                    .add_or_update(peer_tcp_addr.clone(), display_name.clone())
                    .await;

                // Reply to BEACON with ACK so the other peer registers us immediately
                if parts[0] == "CH_BEACON" {
                    let ack = format!(
                        "CH_ACK:v2:{}:{}:{}",
                        my_cfg.device_name, my_cfg.listen_port, my_instance_filter
                    );
                    let _ = recv_socket.send_to(ack.as_bytes(), src_addr).await;
                }

                if is_new {
                    info!(
                        "Discovered new peer '{}' at {}",
                        display_name, peer_tcp_addr
                    );
                }

                if is_new && my_cfg.is_peer_trusted(&peer_tcp_addr, &peer_dev_name) {
                    let pin = my_cfg.pin_for_peer(&peer_tcp_addr, &peer_dev_name);
                    let dev_name = my_cfg.device_name.clone();
                    let repo_clone = recv_repo.clone();
                    let blob_clone = recv_blob.clone();
                    let crypto_clone = recv_crypto.clone();
                    let echo_tracker_clone = recv_echo_tracker.clone();
                    let target_addr = peer_tcp_addr.clone();
                    let cfg_arc_clone = recv_cfg_arc.clone();

                    tokio::spawn(async move {
                        let (enc_enabled, max_entries) = {
                            let cfg = cfg_arc_clone.read().await;
                            (cfg.security.encryption_enabled, cfg.general.max_entries)
                        };
                        crate::sync_service::perform_mutual_history_sync(
                            &target_addr,
                            &pin,
                            &dev_name,
                            repo_clone,
                            blob_clone,
                            crypto_clone,
                            enc_enabled,
                            max_entries,
                            echo_tracker_clone,
                        )
                        .await;
                    });
                }
            }
        }
    });

    // 2. Outbound Beacon Broadcaster Task (every 5 seconds or upon manual scan trigger)
    let send_socket = socket.clone();
    let send_tracker = peer_tracker.clone();
    let send_cfg_arc = sync_cfg_arc.clone();
    let scan_notify = peer_tracker.scan_notify.clone();
    let broadcast_repo = sync_repo.clone();
    let broadcast_blob = sync_blob_store.clone();
    let broadcast_crypto = crypto.clone();
    let broadcast_echo_tracker = echo_tracker.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {},
                _ = scan_notify.notified() => {
                    info!("Manual LAN peer discovery scan requested via IPC");
                }
            }

            let current = send_cfg_arc.read().await.sync.clone();
            if current.enabled {
                let beacon = format!(
                    "CH_BEACON:v2:{}:{}:{}",
                    current.device_name, current.listen_port, my_instance_str
                );
                let mut targets = get_broadcast_addresses(listen_port);

                // Add configured manual peer addresses as direct unicast UDP targets
                for peer in &current.peer_addresses {
                    let clean_host = clean_ip_address(peer);
                    if clean_host.is_empty() {
                        continue;
                    }
                    let addr_str = format_peer_socket_addr(&clean_host, listen_port);
                    if let Ok(sa) = addr_str.parse::<SocketAddr>() {
                        if !targets.contains(&sa) {
                            targets.push(sa);
                        }
                    }
                }

                for target in targets {
                    let _ = send_socket.send_to(beacon.as_bytes(), target).await;
                }

                // If manual peers are configured, probe them directly via TCP if not recently seen
                let mut probed_hosts: Vec<String> = Vec::new();
                for peer in &current.peer_addresses {
                    let clean_host = clean_ip_address(peer);
                    if clean_host.is_empty() || probed_hosts.contains(&clean_host) {
                        continue;
                    }
                    probed_hosts.push(clean_host.clone());

                    if send_tracker
                        .is_recently_seen(&clean_host, Duration::from_secs(10))
                        .await
                    {
                        continue;
                    }

                    let candidate_addrs = build_candidate_sync_addrs(peer, listen_port);
                    let pin = current.pin_for_peer(peer, "");
                    let default_pin = current.pairing_pin.clone();
                    let dev = current.device_name.clone();
                    let trk = send_tracker.clone();
                    let repo_clone = broadcast_repo.clone();
                    let blob_clone = broadcast_blob.clone();
                    let crypto_clone = broadcast_crypto.clone();
                    let echo_tracker_clone = broadcast_echo_tracker.clone();
                    let current_sync = current.clone();
                    let cfg_arc_clone = send_cfg_arc.clone();

                    tokio::spawn(async move {
                        for peer_addr in candidate_addrs {
                            let connect_fut = tokio::net::TcpStream::connect(&peer_addr);
                            if let Ok(Ok(mut stream)) =
                                tokio::time::timeout(Duration::from_secs(3), connect_fut).await
                            {
                                let (mut reader, mut writer) = stream.split();
                                let auth_req = SyncMessage::AuthRequest {
                                    device_name: dev.clone(),
                                    pairing_pin: pin.clone(),
                                    nonce: rand::random(),
                                };
                                let send_ok = tokio::time::timeout(
                                    Duration::from_secs(5),
                                    send_sync_message(&mut writer, &pin, &auth_req),
                                )
                                .await
                                .is_ok_and(|r| r.is_ok());

                                if !send_ok {
                                    let _ = writer.shutdown().await;
                                    continue;
                                }

                                let read_res = tokio::time::timeout(
                                    Duration::from_secs(5),
                                    read_sync_message_with_pins(
                                        &mut reader,
                                        &[pin.as_str()],
                                        &default_pin,
                                    ),
                                )
                                .await;
                                let _ = writer.shutdown().await;

                                match read_res {
                                    Ok(Ok((
                                        SyncMessage::AuthResponse {
                                            success: true,
                                            device_name: peer_dev,
                                            ..
                                        },
                                        matched_pin,
                                    ))) => {
                                        let is_new = trk
                                            .add_or_update(peer_addr.clone(), peer_dev.clone())
                                            .await;
                                        if is_new
                                            && current_sync.is_peer_trusted(&peer_addr, &peer_dev)
                                        {
                                            tokio::spawn(async move {
                                                let (enc_enabled, max_entries) = {
                                                    let cfg = cfg_arc_clone.read().await;
                                                    (
                                                        cfg.security.encryption_enabled,
                                                        cfg.general.max_entries,
                                                    )
                                                };
                                                crate::sync_service::perform_mutual_history_sync(
                                                    &peer_addr,
                                                    &matched_pin,
                                                    &dev,
                                                    repo_clone,
                                                    blob_clone,
                                                    crypto_clone,
                                                    enc_enabled,
                                                    max_entries,
                                                    echo_tracker_clone,
                                                )
                                                .await;
                                            });
                                        }
                                        break;
                                    }
                                    Ok(Ok((
                                        SyncMessage::AuthResponse {
                                            success: false,
                                            device_name: peer_dev,
                                            message,
                                        },
                                        _,
                                    ))) if message.contains("not paired") => {
                                        info!(
                                            "Peer at {} reported this device is not paired. Purging local pairing state.",
                                            peer_addr
                                        );
                                        let clean_p = clean_ip_address(&peer_addr);
                                        let mut cfg = cfg_arc_clone.write().await;
                                        cfg.sync.trusted_peers.retain(|p| {
                                            let cp = clean_ip_address(p);
                                            p != &peer_addr
                                                && (clean_p.is_empty() || cp != clean_p)
                                                && (peer_dev.is_empty() || p != &peer_dev)
                                        });
                                        cfg.sync.peer_addresses.retain(|p| {
                                            let cp = clean_ip_address(p);
                                            p != &peer_addr && (clean_p.is_empty() || cp != clean_p)
                                        });
                                        if !clean_p.is_empty() {
                                            cfg.sync.peer_pins.remove(&clean_p);
                                        }
                                        if !peer_dev.is_empty() {
                                            cfg.sync.peer_pins.remove(&peer_dev);
                                        }
                                        let _ = cfg.save(AppConfig::config_path());
                                        drop(cfg);
                                        trk.remove_peer(&peer_addr).await;
                                        break;
                                    }
                                    Ok(Err(e)) => {
                                        debug!(
                                            "TCP probe AuthResponse decode error from {}: {}",
                                            peer_addr, e
                                        );
                                    }
                                    Err(_) => {
                                        debug!(
                                            "TCP probe timed out waiting for AuthResponse from {}",
                                            peer_addr
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                    });
                }

                send_tracker.cleanup_stale(Duration::from_secs(30)).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_peer_tracker_port_lookup_and_exact_remove() {
        let tracker = PeerTracker::new();
        tracker
            .add_or_update("192.168.1.1:54123".to_string(), "router-pc".to_string())
            .await;
        tracker
            .add_or_update("192.168.1.10:54124".to_string(), "cachyos".to_string())
            .await;
        tracker
            .add_or_update(
                "192.168.1.10:49382".to_string(),
                "cachyos-ephemeral".to_string(),
            )
            .await;

        assert_eq!(tracker.get_port_for_ip("192.168.1.10").await, Some(54124));
        assert!(
            tracker
                .is_recently_seen("192.168.1.10", Duration::from_secs(10))
                .await
        );

        tracker.remove_peer("192.168.1.1").await;
        let remaining = tracker.get_all_peers().await;
        assert!(remaining.iter().any(|(a, _)| a == "192.168.1.10:54124"));
        assert!(!remaining.iter().any(|(a, _)| a == "192.168.1.1:54123"));
    }
}
