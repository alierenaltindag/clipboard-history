use crate::error::{CoreError, Result};
use crate::sync::protocol::SyncMessage;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Nonce};
use sha2::Sha256;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use pbkdf2::pbkdf2_hmac;

use std::collections::HashMap;
use std::sync::RwLock;

static KEY_CACHE: std::sync::OnceLock<RwLock<HashMap<String, [u8; 32]>>> =
    std::sync::OnceLock::new();
const FIXED_LAN_SALT: [u8; 16] = *b"CH_LAN_SYNC_v100";

pub struct LanCrypto;

impl LanCrypto {
    pub fn derive_key(pin: &str, salt: &[u8; 16]) -> [u8; 32] {
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(pin.as_bytes(), salt, 100_000, &mut key);
        key
    }

    /// Derives or retrieves a cached AES-256 key for the given PIN using standard LAN domain salt.
    pub fn get_or_derive_cached_key(pin: &str) -> [u8; 32] {
        let cache = KEY_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
        if let Ok(guard) = cache.read() {
            if let Some(key) = guard.get(pin) {
                return *key;
            }
        }

        let key = Self::derive_key(pin, &FIXED_LAN_SALT);
        if let Ok(mut guard) = cache.write() {
            guard.insert(pin.to_string(), key);
        }
        key
    }

    pub fn encrypt(pin: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        use crate::security::crypto::CryptoError;

        let key = Self::get_or_derive_cached_key(pin);
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| {
            CoreError::Crypto(CryptoError::EncryptionFailed(format!(
                "Failed to init AES cipher: {}",
                e
            )))
        })?;

        // 96-bit (12-byte) CSPRNG nonce
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, plaintext).map_err(|e| {
            CoreError::Crypto(CryptoError::EncryptionFailed(format!(
                "LAN encryption failed: {}",
                e
            )))
        })?;

        // Output format: [16 bytes salt] + [12 bytes nonce] + [ciphertext + 16 bytes tag]
        let mut packet = Vec::with_capacity(16 + 12 + ciphertext.len());
        packet.extend_from_slice(&FIXED_LAN_SALT);
        packet.extend_from_slice(nonce.as_slice());
        packet.extend_from_slice(&ciphertext);
        Ok(packet)
    }

    pub fn decrypt(pin: &str, packet: &[u8]) -> Result<Vec<u8>> {
        use crate::security::crypto::CryptoError;
        if packet.len() < 28 {
            return Err(CoreError::Crypto(CryptoError::CiphertextTooShort));
        }

        let (salt_bytes, rest) = packet.split_at(16);
        let (nonce_bytes, ciphertext) = rest.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let key = if salt_bytes == FIXED_LAN_SALT {
            Self::get_or_derive_cached_key(pin)
        } else {
            let mut salt = [0u8; 16];
            salt.copy_from_slice(salt_bytes);
            Self::derive_key(pin, &salt)
        };

        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| {
            CoreError::Crypto(CryptoError::EncryptionFailed(format!(
                "Failed to init AES cipher: {}",
                e
            )))
        })?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CoreError::Crypto(CryptoError::DecryptionFailed))?;

        Ok(plaintext)
    }

    pub fn decrypt_with_candidates(pins: &[&str], packet: &[u8]) -> Result<Vec<u8>> {
        for &pin in pins {
            if let Ok(dec) = Self::decrypt(pin, packet) {
                return Ok(dec);
            }
        }
        Err(CoreError::Crypto(
            crate::security::crypto::CryptoError::DecryptionFailed,
        ))
    }
}

pub fn get_local_ips() -> Vec<std::net::Ipv4Addr> {
    let mut ips = vec![std::net::Ipv4Addr::new(127, 0, 0, 1)];
    #[cfg(unix)]
    unsafe {
        let mut ifaddrs: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifaddrs) == 0 && !ifaddrs.is_null() {
            let mut curr = ifaddrs;
            while !curr.is_null() {
                let ifa = *curr;
                if !ifa.ifa_addr.is_null() {
                    let family = (*ifa.ifa_addr).sa_family as i32;
                    if family == libc::AF_INET {
                        let sock_in = ifa.ifa_addr as *const libc::sockaddr_in;
                        let ip_bytes = (*sock_in).sin_addr.s_addr.to_ne_bytes();
                        let ip = std::net::Ipv4Addr::from(ip_bytes);
                        if !ips.contains(&ip) {
                            ips.push(ip);
                        }
                    }
                }
                curr = ifa.ifa_next;
            }
            libc::freeifaddrs(ifaddrs);
        }
    }
    ips
}

pub fn get_primary_local_ip() -> Option<String> {
    let ips = get_local_ips();
    for ip in ips {
        if !ip.is_loopback() {
            return Some(ip.to_string());
        }
    }
    None
}

/// Normalizes an IP address, handling IPv6-mapped IPv4 addresses (e.g. `::ffff:192.168.1.1` -> `192.168.1.1`).
pub fn normalize_ip(ip: std::net::IpAddr) -> String {
    match ip {
        std::net::IpAddr::V4(v4) => v4.to_string(),
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                v4.to_string()
            } else if let Some(v4) = v6.to_ipv4() {
                v4.to_string()
            } else {
                v6.to_string()
            }
        }
    }
}

/// Strips ports and normalizes IP from an address string or returns the hostname/identifier as-is.
pub fn clean_ip_address(addr_str: &str) -> String {
    let trimmed = addr_str.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(sa) = trimmed.parse::<std::net::SocketAddr>() {
        return normalize_ip(sa.ip());
    }
    if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        return normalize_ip(ip);
    }
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let inner = &rest[..end];
            let suffix = &rest[end + 1..];
            if suffix.is_empty()
                || suffix
                    .strip_prefix(':')
                    .is_some_and(|p| p.parse::<u16>().is_ok())
            {
                if let Ok(ip) = inner.parse::<std::net::IpAddr>() {
                    return normalize_ip(ip);
                }
                return inner.to_string();
            }
        }
    }
    if let Some((prefix, port_str)) = trimmed.rsplit_once(':') {
        if port_str.parse::<u16>().is_ok() {
            if let Ok(ip) = prefix.parse::<std::net::IpAddr>() {
                return normalize_ip(ip);
            }
            if !prefix.contains(':') {
                return prefix.to_string();
            }
        }
    }
    trimmed.to_string()
}

/// Splits an address string into normalized host/IP and optional port.
pub fn split_host_and_port(addr_str: &str) -> (String, Option<u16>) {
    let trimmed = addr_str.trim();
    if trimmed.is_empty() {
        return (String::new(), None);
    }
    if let Ok(sa) = trimmed.parse::<std::net::SocketAddr>() {
        return (normalize_ip(sa.ip()), Some(sa.port()));
    }
    if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        return (normalize_ip(ip), None);
    }
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let inner = &rest[..end];
            let suffix = &rest[end + 1..];
            let host = if let Ok(ip) = inner.parse::<std::net::IpAddr>() {
                normalize_ip(ip)
            } else {
                inner.to_string()
            };
            if suffix.is_empty() {
                return (host, None);
            }
            if let Some(port_str) = suffix.strip_prefix(':') {
                if let Ok(port) = port_str.parse::<u16>() {
                    return (host, Some(port));
                }
            }
        }
    }
    if let Some((prefix, port_str)) = trimmed.rsplit_once(':') {
        if let Ok(port) = port_str.parse::<u16>() {
            if let Ok(ip) = prefix.parse::<std::net::IpAddr>() {
                return (normalize_ip(ip), Some(port));
            }
            if !prefix.contains(':') {
                return (prefix.to_string(), Some(port));
            }
        }
    }
    (trimmed.to_string(), None)
}

/// Formats a host/IP and port into a socket address string, normalizing IPs and bracketing IPv6.
pub fn format_peer_socket_addr(host: &str, port: u16) -> String {
    let clean = clean_ip_address(host);
    if clean.contains(':') {
        format!("[{}]:{}", clean, port)
    } else {
        format!("{}:{}", clean, port)
    }
}

/// Builds prioritized, deduplicated TCP socket candidate addresses for reaching a peer.
pub fn build_candidate_sync_addrs(peer: &str, default_port: u16) -> Vec<String> {
    let (raw_host, port_opt) = split_host_and_port(peer);
    let host = clean_ip_address(&raw_host);
    if host.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::with_capacity(3);
    let mut push_port = |port: u16| {
        if port == 0 {
            return;
        }
        let formatted = format_peer_socket_addr(&host, port);
        if !candidates.contains(&formatted) {
            candidates.push(formatted);
        }
    };

    if let Some(port) = port_opt {
        let is_listening_port =
            port > 0 && (port == default_port || port == 54123 || port == 54124 || port < 32768);
        if is_listening_port {
            push_port(port);
        }
    }

    push_port(default_port);
    push_port(54123);
    push_port(54124);
    candidates
}

/// Send an encrypted SyncMessage over an AsyncWrite stream
pub async fn send_sync_message<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    pin: &str,
    msg: &SyncMessage,
) -> Result<()> {
    let serialized =
        serde_json::to_vec(msg).map_err(|e| CoreError::Ipc(format!("Serialize error: {}", e)))?;
    let encrypted = LanCrypto::encrypt(pin, &serialized)?;
    let len = encrypted.len() as u32;

    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&encrypted).await?;
    writer.flush().await?;
    Ok(())
}

/// Read and decrypt a SyncMessage using candidate PINs, returning `(message, matched_pin)`.
pub async fn read_sync_message_with_pins<R: AsyncReadExt + Unpin>(
    reader: &mut R,
    extra_pins: &[&str],
    default_pin: &str,
) -> Result<(SyncMessage, String)> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 64 * 1024 * 1024 {
        return Err(CoreError::Ipc("Payload exceeds 64MB limit".to_string()));
    }

    let mut buf = Vec::with_capacity(len.min(64 * 1024));
    let read_count = reader.take(len as u64).read_to_end(&mut buf).await?;
    if read_count != len {
        return Err(CoreError::Ipc(format!(
            "Incomplete LAN sync payload: expected {} bytes, received {}",
            len, read_count
        )));
    }

    let mut candidates: Vec<String> = Vec::with_capacity(extra_pins.len() + 3);
    for &p in extra_pins {
        if !p.is_empty() && !candidates.iter().any(|c| c == p) {
            candidates.push(p.to_string());
        }
    }
    if !default_pin.is_empty() && !candidates.iter().any(|c| c == default_pin) {
        candidates.push(default_pin.to_string());
    }
    if !candidates.iter().any(|c| c == "123456") {
        candidates.push("123456".to_string());
    }
    if !candidates.iter().any(|c| c == "CH_PAIRING_V1") {
        candidates.push("CH_PAIRING_V1".to_string());
    }

    for candidate in &candidates {
        if let Ok(decrypted) = LanCrypto::decrypt(candidate, &buf) {
            let msg: SyncMessage = serde_json::from_slice(&decrypted)
                .map_err(|e| CoreError::Ipc(format!("Deserialize error: {}", e)))?;
            return Ok((msg, candidate.clone()));
        }
    }

    Err(CoreError::Crypto(
        crate::security::crypto::CryptoError::DecryptionFailed,
    ))
}

/// Read and decrypt a SyncMessage from an AsyncRead stream
pub async fn read_sync_message<R: AsyncReadExt + Unpin>(
    reader: &mut R,
    pin: &str,
) -> Result<SyncMessage> {
    read_sync_message_with_pins(reader, &[], pin)
        .await
        .map(|(m, _)| m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_ip_address_variants() {
        assert_eq!(clean_ip_address("192.168.1.50"), "192.168.1.50");
        assert_eq!(clean_ip_address("  192.168.1.50:54123  "), "192.168.1.50");
        assert_eq!(clean_ip_address("::ffff:192.168.1.42"), "192.168.1.42");
        assert_eq!(
            clean_ip_address("[::ffff:192.168.1.50]:49382"),
            "192.168.1.50"
        );
        assert_eq!(clean_ip_address("[::ffff:192.168.1.50]"), "192.168.1.50");
        assert_eq!(
            clean_ip_address("::ffff:192.168.1.50:54123"),
            "192.168.1.50"
        );
        assert_eq!(clean_ip_address("[fe80::1]:54123"), "fe80::1");
        assert_eq!(clean_ip_address("fe80::1"), "fe80::1");
        assert_eq!(clean_ip_address("2001:db8::1"), "2001:db8::1");
        assert_eq!(clean_ip_address("[2001:db8::1]:54124"), "2001:db8::1");
        assert_eq!(clean_ip_address("[fe80::1%eth0]:54123"), "fe80::1%eth0");
        assert_eq!(clean_ip_address("cachyos-laptop:54123"), "cachyos-laptop");
        assert_eq!(clean_ip_address("cachyos-laptop"), "cachyos-laptop");
        assert_eq!(clean_ip_address("   "), "");
    }

    #[test]
    fn test_split_host_and_port_variants() {
        assert_eq!(
            split_host_and_port("192.168.1.50"),
            ("192.168.1.50".to_string(), None)
        );
        assert_eq!(
            split_host_and_port("192.168.1.50:54123"),
            ("192.168.1.50".to_string(), Some(54123))
        );
        assert_eq!(
            split_host_and_port("2001:db8::1"),
            ("2001:db8::1".to_string(), None)
        );
        assert_eq!(
            split_host_and_port("[2001:db8::1]:54124"),
            ("2001:db8::1".to_string(), Some(54124))
        );
        assert_eq!(
            split_host_and_port("[::ffff:192.168.1.50]:49382"),
            ("192.168.1.50".to_string(), Some(49382))
        );
        assert_eq!(
            split_host_and_port("::ffff:192.168.1.50:54123"),
            ("192.168.1.50".to_string(), Some(54123))
        );
        assert_eq!(
            split_host_and_port("ubuntu-desktop.local:54123"),
            ("ubuntu-desktop.local".to_string(), Some(54123))
        );
        assert_eq!(
            split_host_and_port("ubuntu-desktop"),
            ("ubuntu-desktop".to_string(), None)
        );
    }

    #[test]
    fn test_format_peer_socket_addr() {
        assert_eq!(
            format_peer_socket_addr("192.168.1.50", 54123),
            "192.168.1.50:54123"
        );
        assert_eq!(
            format_peer_socket_addr("::ffff:192.168.1.50", 54123),
            "192.168.1.50:54123"
        );
        assert_eq!(format_peer_socket_addr("fe80::1", 54123), "[fe80::1]:54123");
        assert_eq!(
            format_peer_socket_addr("[2001:db8::1]:49382", 54124),
            "[2001:db8::1]:54124"
        );
    }

    #[test]
    fn test_build_candidate_sync_addrs_ephemeral_and_dedup() {
        assert_eq!(
            build_candidate_sync_addrs("192.168.1.50", 54123),
            vec!["192.168.1.50:54123", "192.168.1.50:54124"]
        );
        assert_eq!(
            build_candidate_sync_addrs("192.168.1.50:54124", 54123),
            vec!["192.168.1.50:54124", "192.168.1.50:54123"]
        );
        assert_eq!(
            build_candidate_sync_addrs("[::ffff:192.168.1.50]:49382", 54123),
            vec!["192.168.1.50:54123", "192.168.1.50:54124"]
        );
        assert_eq!(
            build_candidate_sync_addrs("192.168.1.50:9000", 54123),
            vec![
                "192.168.1.50:9000",
                "192.168.1.50:54123",
                "192.168.1.50:54124"
            ]
        );
        assert_eq!(
            build_candidate_sync_addrs("2001:db8::1", 55000),
            vec![
                "[2001:db8::1]:55000",
                "[2001:db8::1]:54123",
                "[2001:db8::1]:54124"
            ]
        );
        assert!(build_candidate_sync_addrs("   ", 54123).is_empty());
    }

    #[tokio::test]
    async fn test_read_sync_message_with_pins_returns_matched_pin() {
        let (mut tx_io, mut rx_io) = tokio::io::duplex(4096);
        let msg = SyncMessage::AuthRequest {
            device_name: "cachyos".to_string(),
            pairing_pin: "654321".to_string(),
            nonce: 42,
        };
        send_sync_message(&mut tx_io, "654321", &msg).await.unwrap();

        let (decoded, matched_pin) =
            read_sync_message_with_pins(&mut rx_io, &["", "111111", "654321"], "999999")
                .await
                .unwrap();
        assert_eq!(matched_pin, "654321");
        match decoded {
            SyncMessage::AuthRequest { device_name, .. } => assert_eq!(device_name, "cachyos"),
            _ => panic!("Expected AuthRequest"),
        }

        // Also verify fallback to default_pin when not in extra_pins
        let msg2 = SyncMessage::Ping;
        send_sync_message(&mut tx_io, "999999", &msg2)
            .await
            .unwrap();
        let (decoded2, matched_pin2) =
            read_sync_message_with_pins(&mut rx_io, &["111111"], "999999")
                .await
                .unwrap();
        assert_eq!(matched_pin2, "999999");
        assert!(matches!(decoded2, SyncMessage::Ping));
    }
}
