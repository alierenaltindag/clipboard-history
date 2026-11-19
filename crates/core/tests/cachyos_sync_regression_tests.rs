use chrono::Duration;
use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::SyncConfig;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::security::{CryptoEngine, PasswordManagerGuard};
use clipboard_history_core::storage::SqliteRepository;
use clipboard_history_core::sync::{
    build_candidate_sync_addrs, clean_ip_address, read_sync_message_with_pins, send_sync_message,
    SyncMessage,
};
use std::collections::HashMap;
use tokio::io::duplex;

#[test]
fn regression_clean_ip_address_formats() {
    assert_eq!(clean_ip_address("192.168.1.50"), "192.168.1.50");
    assert_eq!(clean_ip_address("192.168.1.50:54123"), "192.168.1.50");
    assert_eq!(clean_ip_address("  192.168.1.50:49382 "), "192.168.1.50");
    assert_eq!(clean_ip_address("::ffff:192.168.1.50"), "192.168.1.50");
    assert_eq!(
        clean_ip_address("[::ffff:192.168.1.50]:49382"),
        "192.168.1.50"
    );
    assert_eq!(
        clean_ip_address("::ffff:192.168.1.50:54123"),
        "192.168.1.50"
    );
    assert_eq!(clean_ip_address("[fe80::1]:54123"), "fe80::1");
    assert_eq!(clean_ip_address("fe80::1"), "fe80::1");
    assert_eq!(clean_ip_address("cachyos-laptop"), "cachyos-laptop");
    assert_eq!(clean_ip_address("cachyos-laptop:54123"), "cachyos-laptop");
}

#[test]
fn regression_build_candidate_sync_addrs_deprioritizes_ephemeral_ports() {
    let from_ephemeral_v4 = build_candidate_sync_addrs("192.168.1.50:49382", 54123);
    assert_eq!(
        from_ephemeral_v4,
        vec!["192.168.1.50:54123", "192.168.1.50:54124"]
    );

    let from_mapped_ephemeral = build_candidate_sync_addrs("[::ffff:192.168.1.50]:49382", 54123);
    assert_eq!(
        from_mapped_ephemeral,
        vec!["192.168.1.50:54123", "192.168.1.50:54124"]
    );

    let from_ephemeral_v6 = build_candidate_sync_addrs("[fe80::1]:55123", 54123);
    assert_eq!(
        from_ephemeral_v6,
        vec!["[fe80::1]:54123", "[fe80::1]:54124"]
    );

    let from_secondary_port = build_candidate_sync_addrs("192.168.1.50:54124", 54123);
    assert_eq!(
        from_secondary_port,
        vec!["192.168.1.50:54124", "192.168.1.50:54123"]
    );
}

#[test]
fn regression_sync_config_is_peer_trusted_no_prefix_collision_and_ipv6_mapped() {
    let cfg = SyncConfig {
        trusted_peers: vec![
            "192.168.1.1".to_string(),
            "192.168.1.50".to_string(),
            "cachyos-desktop".to_string(),
        ],
        ..Default::default()
    };

    assert!(cfg.is_peer_trusted("192.168.1.1:54123", ""));
    assert!(!cfg.is_peer_trusted("192.168.1.10", ""));
    assert!(!cfg.is_peer_trusted("192.168.1.10:54123", ""));
    assert!(!cfg.is_peer_trusted("192.168.1.100:54124", "other-host"));

    assert!(cfg.is_peer_trusted("::ffff:192.168.1.50", ""));
    assert!(cfg.is_peer_trusted("[::ffff:192.168.1.50]:49382", ""));
    assert!(cfg.is_peer_trusted("10.0.0.42:54123", "cachyos-desktop"));
}

#[tokio::test]
async fn regression_bidirectional_sync_roundtrip_mismatched_and_matching_pins() {
    // 1. Mismatched PINs: CachyOS uses default "123456", Ubuntu uses custom "987654"
    //    and stores CachyOS's PIN in peer_pins (or relies on default fallback).
    let (mut cachyos_stream, mut ubuntu_stream) = duplex(16 * 1024);

    let mut ubuntu_peer_pins = HashMap::new();
    ubuntu_peer_pins.insert("192.168.1.50".to_string(), "123456".to_string());
    let ubuntu_cfg = SyncConfig {
        pairing_pin: "987654".to_string(),
        peer_pins: ubuntu_peer_pins,
        ..Default::default()
    };
    let cachyos_cfg = SyncConfig {
        pairing_pin: "123456".to_string(),
        ..Default::default()
    };

    // Direction A -> B (CachyOS sends AuthRequest using its "123456")
    let auth_req = SyncMessage::AuthRequest {
        device_name: "cachyos-laptop".to_string(),
        pairing_pin: cachyos_cfg.pairing_pin.clone(),
        nonce: 77,
    };
    send_sync_message(&mut cachyos_stream, &cachyos_cfg.pairing_pin, &auth_req)
        .await
        .expect("send auth request");

    let peer_pin_for_cachyos = ubuntu_cfg.pin_for_peer("192.168.1.50:49382", "cachyos-laptop");
    let (decoded_req, matched_pin) = read_sync_message_with_pins(
        &mut ubuntu_stream,
        &[peer_pin_for_cachyos.as_str()],
        &ubuntu_cfg.pairing_pin,
    )
    .await
    .expect("read auth request with mismatched local pin");
    assert_eq!(matched_pin, "123456");
    assert!(matches!(
        decoded_req,
        SyncMessage::AuthRequest { device_name, .. } if device_name == "cachyos-laptop"
    ));

    // Direction B -> A (Ubuntu replies using matched_pin)
    let auth_resp = SyncMessage::AuthResponse {
        success: true,
        device_name: "ubuntu-desktop".to_string(),
        message: "Authenticated".to_string(),
    };
    send_sync_message(&mut ubuntu_stream, &matched_pin, &auth_resp)
        .await
        .expect("send auth response");

    let (decoded_resp, resp_pin) =
        read_sync_message_with_pins(&mut cachyos_stream, &[], &cachyos_cfg.pairing_pin)
            .await
            .expect("read auth response on cachyos");
    assert_eq!(resp_pin, "123456");
    assert!(matches!(
        decoded_resp,
        SyncMessage::AuthResponse { success: true, .. }
    ));

    // 2. Custom matching PINs in both directions (both use "424242")
    let custom_entry = ClipboardEntry::new_text(
        "synced across cachyos and ubuntu".to_string(),
        BlobStore::compute_hash(b"synced across cachyos and ubuntu"),
        vec!["text/plain".to_string()],
        Some("kitty".to_string()),
    );
    let sync_msg = SyncMessage::EntrySync {
        origin_device: "ubuntu-desktop".to_string(),
        entry: Box::new(custom_entry),
        blob_payload: None,
        thumbnail_payload: None,
        is_history_batch: false,
    };
    send_sync_message(&mut ubuntu_stream, "424242", &sync_msg)
        .await
        .expect("send custom pin entry");

    let (decoded_entry, entry_pin) =
        read_sync_message_with_pins(&mut cachyos_stream, &["424242"], "123456")
            .await
            .expect("read custom pin entry");
    assert_eq!(entry_pin, "424242");
    match decoded_entry {
        SyncMessage::EntrySync {
            origin_device,
            entry,
            ..
        } => {
            assert_eq!(origin_device, "ubuntu-desktop");
            assert_eq!(
                entry.text_content.as_deref(),
                Some("synced across cachyos and ubuntu")
            );
        }
        other => panic!("Expected EntrySync, got {:?}", other),
    }

    // 3. Unknown custom PIN must not decrypt with mismatched PINs
    send_sync_message(&mut ubuntu_stream, "888888", &SyncMessage::Ping)
        .await
        .expect("send ping with unknown custom pin");
    let err_res = read_sync_message_with_pins(&mut cachyos_stream, &["424242"], "999999").await;
    assert!(err_res.is_err());
}

#[test]
fn regression_password_manager_guard_mixed_case_and_innocent_private_mimes() {
    assert!(PasswordManagerGuard::is_sensitive_mime(&[
        "x-kde-passwordManagerHint".to_string()
    ]));
    assert!(PasswordManagerGuard::is_sensitive_mime(&[
        "X-KDE-PasswordManagerHint".to_string()
    ]));
    assert!(PasswordManagerGuard::is_sensitive_mime(&[
        "application/x-keepassxc-selection".to_string()
    ]));
    assert!(PasswordManagerGuard::is_sensitive_mime(&[
        "x-private-clipboard".to_string()
    ]));
    assert!(PasswordManagerGuard::is_sensitive_mime(&[
        "application/x-password".to_string()
    ]));

    assert!(!PasswordManagerGuard::is_sensitive_mime(&[
        "application/x-private-notes".to_string()
    ]));
    assert!(!PasswordManagerGuard::is_sensitive_mime(&[
        "text/plain".to_string(),
        "application/x-private-notes".to_string(),
        "chromium/x-web-custom-data".to_string(),
    ]));
}

#[test]
fn regression_batch_insert_or_update_prevents_encrypted_dek_mismatch() {
    let repo = SqliteRepository::open_in_memory().expect("open in-memory sqlite");
    let crypto = CryptoEngine::new_from_key([0x5A; 32]);

    let plaintext = "top secret clipboard payload";
    let content_hash = BlobStore::compute_hash(plaintext.as_bytes());

    let mut first = ClipboardEntry::new_text(
        plaintext.to_string(),
        content_hash.clone(),
        vec!["text/plain".to_string()],
        Some("firefox".to_string()),
    );
    let first_id = first.id.clone();
    let wrapped_dek_1 = crypto
        .encrypt_entry_envelope(&mut first)
        .expect("encrypt first envelope");
    let saved_first = repo
        .batch_insert_or_update(&[first.clone()])
        .expect("insert first batch");
    assert_eq!(saved_first.len(), 1);
    assert_eq!(saved_first[0].id, first_id);
    repo.insert_entry_dek(&saved_first[0].id, &wrapped_dek_1, &[])
        .expect("store first DEK");

    // Simulate a duplicate copy of the exact same text encrypted with a second ephemeral DEK
    let mut duplicate = ClipboardEntry::new_text(
        plaintext.to_string(),
        content_hash,
        vec!["text/plain".to_string()],
        Some("firefox".to_string()),
    );
    duplicate.last_used_at = first.last_used_at + Duration::seconds(10);
    let _wrapped_dek_2 = crypto
        .encrypt_entry_envelope(&mut duplicate)
        .expect("encrypt duplicate envelope");
    assert_ne!(duplicate.id, first_id);
    assert_ne!(duplicate.text_content, first.text_content);

    let saved_dup = repo
        .batch_insert_or_update(&[duplicate])
        .expect("batch upsert duplicate");
    assert_eq!(saved_dup.len(), 1);

    let mut returned = saved_dup.into_iter().next().expect("returned row");
    assert_eq!(
        returned.id, first_id,
        "duplicate batch upsert must return existing entry ID"
    );
    assert_eq!(
        returned.preview, first.preview,
        "duplicate batch upsert must return existing encrypted preview matching stored DEK"
    );
    assert_eq!(
        returned.text_content, first.text_content,
        "duplicate batch upsert must return existing encrypted text matching stored DEK"
    );
    assert!(returned.last_used_at > first.last_used_at);

    let (stored_dek, _) = repo
        .get_entry_dek(&returned.id)
        .expect("query DEK")
        .expect("stored DEK must exist");
    crypto
        .decrypt_entry_envelope(&mut returned, &stored_dek)
        .expect("decrypt returned entry with stored DEK");
    assert_eq!(returned.text_content.as_deref(), Some(plaintext));
    assert_eq!(returned.preview, plaintext);
}
