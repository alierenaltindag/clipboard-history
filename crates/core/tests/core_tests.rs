use chrono::{Duration, Utc};
use clipboard_history_core::blob::{BlobStore, ThumbnailGenerator};
use clipboard_history_core::cache::BoundedCache;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::ipc::{read_message, write_message, IpcRequest, IpcResponse};
use clipboard_history_core::security::{PasswordManagerGuard, SecretFilter};
use clipboard_history_core::storage::SqliteRepository;
use image::{ImageBuffer, Rgb};
use tempfile::tempdir;
use tokio::io::duplex;

#[test]
fn test_sqlite_repository_crud_and_deduplication() {
    let repo = SqliteRepository::open_in_memory().expect("Failed to open in-memory db");

    let entry1 = ClipboardEntry::new_text(
        "Hello World".to_string(),
        BlobStore::compute_hash(b"Hello World"),
        vec!["text/plain".to_string()],
        Some("gnome-terminal".to_string()),
    );

    // Insert
    let inserted = repo.insert_or_update(&entry1).expect("Insert failed");
    assert_eq!(inserted.preview, "Hello World");
    assert_eq!(repo.count().unwrap(), 1);

    // Re-inserting exact same content should update timestamp rather than add new row
    let duplicate = ClipboardEntry::new_text(
        "Hello World".to_string(),
        BlobStore::compute_hash(b"Hello World"),
        vec!["text/plain".to_string()],
        Some("code".to_string()),
    );
    let updated = repo.insert_or_update(&duplicate).expect("Update failed");
    assert_eq!(updated.id, inserted.id);
    assert_eq!(repo.count().unwrap(), 1);

    // Search
    let search_res = repo.search("Hello", 10, 0).expect("Search failed");
    assert_eq!(search_res.len(), 1);
    assert_eq!(search_res[0].preview, "Hello World");

    // Pinning
    repo.set_pinned(&inserted.id, true).expect("Pin failed");
    let pinned_entry = repo.get_by_id(&inserted.id).expect("Get failed");
    assert!(pinned_entry.is_pinned);

    // Clear without pinned should keep pinned entry
    let cleared = repo.clear(false).expect("Clear failed");
    assert_eq!(cleared, 0);
    assert_eq!(repo.count().unwrap(), 1);

    // Delete
    repo.delete(&inserted.id).expect("Delete failed");
    assert_eq!(repo.count().unwrap(), 0);
}

#[test]
fn test_sqlite_capacity_and_retention_eviction() {
    let repo = SqliteRepository::open_in_memory().expect("Failed to open in-memory db");

    for i in 0..10 {
        let text = format!("Item {}", i);
        let hash = BlobStore::compute_hash(text.as_bytes());
        let entry = ClipboardEntry::new_text(text, hash, vec!["text/plain".to_string()], None);
        repo.insert_or_update(&entry).unwrap();
    }
    assert_eq!(repo.count().unwrap(), 10);

    // Evict capacity to max 5 items
    let evicted = repo.evict_capacity(5).expect("Capacity eviction failed");
    assert_eq!(evicted, 5);
    assert_eq!(repo.count().unwrap(), 5);

    // Test TTL eviction
    let cutoff = Utc::now() + Duration::days(1); // cutoff in the future deletes all
    let expired = repo.evict_expired(cutoff).expect("TTL eviction failed");
    assert_eq!(expired, 5);
    assert_eq!(repo.count().unwrap(), 0);
}

#[test]
fn test_blob_store_and_orphan_cleanup() {
    let dir = tempdir().expect("Failed to create tempdir");
    let store = BlobStore::new(dir.path()).expect("Failed to create blobstore");

    let payload = b"Universal Clipboard High Performance Binary Payload";
    let hash = store.save(payload).expect("Failed to save blob");

    assert!(store.exists(&hash));
    let read_bytes = store.read(&hash).expect("Failed to read blob");
    assert_eq!(read_bytes, payload);

    // Verify file permissions
    let path = store.path_for(&hash);
    let meta = std::fs::metadata(&path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);

    // Verify orphan cleanup
    let dummy_hash = store.save(b"Another Blob").expect("Save 2 failed");
    // Only pass first hash as active
    let cleaned = store
        .cleanup_orphans(std::slice::from_ref(&hash))
        .expect("Cleanup failed");
    assert_eq!(cleaned, 1);
    assert!(store.exists(&hash));
    assert!(!store.exists(&dummy_hash));
}

#[test]
fn test_thumbnail_generator() {
    // Generate a simple 200x200 red image
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_pixel(200, 200, Rgb([255, 0, 0]));
    let mut buffer = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buffer, image::ImageFormat::Png).unwrap();
    let png_bytes = buffer.into_inner();

    let (dimensions, thumb_bytes) =
        ThumbnailGenerator::generate(&png_bytes).expect("Thumbnail failed");
    assert_eq!(dimensions, (200, 200));
    assert!(!thumb_bytes.is_empty());

    // Load generated thumbnail to verify downscaled dimensions
    let thumb_img = image::load_from_memory(&thumb_bytes).expect("Load thumb failed");
    assert!(thumb_img.width() <= 128);
    assert!(thumb_img.height() <= 128);
}

#[test]
fn test_security_filter_and_password_guard() {
    let filter = SecretFilter::default_instance();

    let clean_text = "Standard plain text without secrets";
    assert!(!filter.contains_secret(clean_text));

    let private_key =
        "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
    assert!(filter.contains_secret(private_key));
    let masked_key = filter.mask(private_key);
    assert!(masked_key.contains("[MASKED PRIVATE KEY]"));

    let github_token = "ghp_123456789012345678901234567890123456";
    assert!(filter.contains_secret(github_token));

    let aws_key = "My AWS key is AKIAIOSFODNN7EXAMPLE";
    assert!(filter.contains_secret(aws_key));

    // Test password manager MIME detector
    assert!(PasswordManagerGuard::is_sensitive_mime(&[
        "x-kde-passwordManagerHint".to_string()
    ]));
    assert!(PasswordManagerGuard::is_sensitive_mime(&[
        "application/x-keepassxc-selection".to_string()
    ]));
    assert!(!PasswordManagerGuard::is_sensitive_mime(&[
        "text/plain".to_string(),
        "text/html".to_string()
    ]));
}

#[test]
fn test_bounded_lru_cache() {
    let cache = BoundedCache::new(2);
    cache.put("k1", "v1");
    cache.put("k2", "v2");
    assert_eq!(cache.get(&"k1"), Some("v1"));

    cache.put("k3", "v3"); // should evict k2 (k1 was accessed recently)
    assert_eq!(cache.get(&"k1"), Some("v1"));
    assert_eq!(cache.get(&"k2"), None);
    assert_eq!(cache.get(&"k3"), Some("v3"));
}

#[tokio::test]
async fn test_ipc_framing() {
    let (mut client_io, mut server_io) = duplex(1024);

    let request = IpcRequest::GetEntry {
        id: "test-id".to_string(),
    };
    write_message(&mut client_io, &request)
        .await
        .expect("Write req failed");

    let received_req: IpcRequest = read_message(&mut server_io).await.expect("Read req failed");
    match received_req {
        IpcRequest::GetEntry { id } => assert_eq!(id, "test-id"),
        _ => panic!("Unexpected request type"),
    }

    let response = IpcResponse::Success;
    write_message(&mut server_io, &response)
        .await
        .expect("Write resp failed");

    let received_resp: IpcResponse = read_message(&mut client_io)
        .await
        .expect("Read resp failed");
    match received_resp {
        IpcResponse::Success => (),
        _ => panic!("Unexpected response type"),
    }
}

#[test]
fn test_crypto_engine_encryption_decryption() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("secret.key");

    let crypto = clipboard_history_core::security::CryptoEngine::load_or_create(&key_path)
        .expect("Failed to initialize CryptoEngine");

    assert!(key_path.exists());

    // Test raw byte encryption
    let plaintext = b"Sensitive secret payload to be encrypted with AES-256-GCM";
    let ciphertext = crypto.encrypt(plaintext).expect("Encryption failed");
    assert_ne!(plaintext.as_slice(), ciphertext.as_slice());

    let decrypted = crypto.decrypt(&ciphertext).expect("Decryption failed");
    assert_eq!(plaintext.as_slice(), decrypted.as_slice());

    // Test string encryption with $ENC$ prefix
    let secret_str = "sk-live-1234567890abcdef";
    let enc_str = crypto.encrypt_str(secret_str).expect("String enc failed");
    assert!(enc_str.starts_with("$ENC$"));
    let dec_str = crypto.decrypt_str(&enc_str).expect("String dec failed");
    assert_eq!(secret_str, dec_str);

    // Test tamper resistance
    let mut tampered = ciphertext.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert!(crypto.decrypt(&tampered).is_err());

    // Verify key file POSIX 0600 permissions
    let meta = std::fs::metadata(&key_path).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);

    // Test encrypted blob storage in BlobStore
    let store = BlobStore::new(dir.path().join("blobs")).unwrap();
    let secret_blob = b"Secret binary blob data protected with AES-GCM";
    let blob_hash = store
        .save_encrypted(secret_blob, &crypto)
        .expect("save_encrypted failed");
    assert!(store.exists(&blob_hash));
    let decrypted_blob = store
        .read_decrypted(&blob_hash, &crypto)
        .expect("read_decrypted failed");
    assert_eq!(decrypted_blob, secret_blob);
}

#[test]
fn test_text_transforms_and_color_detection() {
    use clipboard_history_core::transforms::TextTransforms;

    // Case conversions
    assert_eq!(TextTransforms::to_uppercase("hello world"), "HELLO WORLD");
    assert_eq!(TextTransforms::to_lowercase("HELLO WORLD"), "hello world");
    assert_eq!(
        TextTransforms::to_title_case("hello world_foo-bar"),
        "Hello World_Foo-Bar"
    );
    assert_eq!(
        TextTransforms::to_snake_case("helloWorldFoo"),
        "hello_world_foo"
    );
    assert_eq!(
        TextTransforms::to_kebab_case("helloWorldFoo"),
        "hello-world-foo"
    );
    assert_eq!(
        TextTransforms::to_camel_case("hello_world_foo"),
        "helloWorldFoo"
    );

    // JSON prettify / minify
    let json_raw = r#"{"name":"test","count":42}"#;
    let prettified = TextTransforms::json_prettify(json_raw).expect("Prettify failed");
    assert!(prettified.contains('\n'));
    let minified = TextTransforms::json_minify(&prettified).expect("Minify failed");
    let val_original: serde_json::Value = serde_json::from_str(json_raw).unwrap();
    let val_minified: serde_json::Value = serde_json::from_str(&minified).unwrap();
    assert_eq!(val_original, val_minified);

    // Base64
    let original = "Hello clipboard!";
    let b64 = TextTransforms::base64_encode(original);
    assert_eq!(TextTransforms::base64_decode(&b64).unwrap(), original);

    // URL encode / decode
    let url_str = "hello world & foo=bar";
    let encoded = TextTransforms::url_encode(url_str);
    assert_eq!(encoded, "hello%20world%20%26%20foo%3Dbar");
    assert_eq!(TextTransforms::url_decode(&encoded).unwrap(), url_str);

    // HTML strip formatting
    let html = "<p>Hello <b>World</b>&nbsp;&amp;&nbsp;friends!</p>";
    assert_eq!(
        TextTransforms::strip_formatting(html),
        "Hello World & friends!"
    );

    // Color detection
    let hex_color = TextTransforms::detect_color("#3B82F6").expect("Hex color not detected");
    assert_eq!(hex_color.hex, "#3B82F6");
    assert_eq!(hex_color.r, 0x3B);
    assert_eq!(hex_color.g, 0x82);
    assert_eq!(hex_color.b, 0xF6);

    let rgb_color =
        TextTransforms::detect_color("rgb(255, 128, 0)").expect("RGB color not detected");
    assert_eq!(rgb_color.hex, "#FF8000");

    let hex3_color = TextTransforms::detect_color("#F80").expect("3-digit Hex not detected");
    assert_eq!(hex3_color.hex, "#FF8800");
    assert_eq!(hex3_color.r, 255);
    assert_eq!(hex3_color.g, 136);
    assert_eq!(hex3_color.b, 0);

    let hex8_color = TextTransforms::detect_color("#3B82F680").expect("8-digit Hex not detected");
    assert_eq!(hex8_color.hex, "#3B82F6");
    assert!((hex8_color.a - 0.50196).abs() < 0.01);

    let rgba_color =
        TextTransforms::detect_color("rgba(100, 150, 200, 0.5)").expect("RGBA not detected");
    assert_eq!(rgba_color.r, 100);
    assert_eq!(rgba_color.g, 150);
    assert_eq!(rgba_color.b, 200);
    assert_eq!(rgba_color.a, 0.5);

    // QR Code generation
    let qr_svg =
        TextTransforms::generate_qr_svg("https://github.com/alierenaltindag/clipboard-history")
            .expect("QR generation failed");
    assert!(qr_svg.contains("<svg"));
    assert!(qr_svg.contains("</svg>"));
}

#[test]
fn test_security_config_defaults_and_toggle() {
    use clipboard_history_core::config::AppConfig;

    let mut config = AppConfig::default();
    // Default must be true: ignore/protect password managers and incognito windows
    assert!(config.security.ignore_password_managers);
    assert!(config.security.ignore_incognito_windows);

    // Verify disabling
    config.security.ignore_password_managers = false;
    config.security.ignore_incognito_windows = false;
    assert!(!config.security.ignore_password_managers);
    assert!(!config.security.ignore_incognito_windows);

    // Verify round-trip serialization to TOML
    let toml_str = toml::to_string(&config).unwrap();
    let deserialized: AppConfig = toml::from_str(&toml_str).unwrap();
    assert!(!deserialized.security.ignore_password_managers);
    assert!(!deserialized.security.ignore_incognito_windows);

    // Verify re-enabling
    let mut config_re_enabled = deserialized;
    config_re_enabled.security.ignore_password_managers = true;
    config_re_enabled.security.ignore_incognito_windows = true;
    let toml_re = toml::to_string(&config_re_enabled).unwrap();
    let des_re: AppConfig = toml::from_str(&toml_re).unwrap();
    assert!(des_re.security.ignore_password_managers);
    assert!(des_re.security.ignore_incognito_windows);
}
