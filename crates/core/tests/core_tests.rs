use chrono::{Duration, Utc};
use clipboard_history_core::blob::{BlobStore, ThumbnailGenerator};
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

    // Test JWT detection and masking (MED-06)
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    assert!(filter.contains_secret(jwt));
    assert!(filter.mask(jwt).contains("[MASKED JWT]"));

    // Test Stripe key detection and masking (MED-06)
    let stripe_key = ["sk", "live", "51MzZ1234567890abcdefghijklmnopqrstuvwxyz"].join("_");
    assert!(filter.contains_secret(&stripe_key));
    assert!(filter.mask(&stripe_key).contains("[MASKED STRIPE KEY]"));

    // Test Google API key detection and masking (MED-06)
    let google_key = ["AIzaSyD", "1234567890abcdefghijklmnopqrstuv"].join("-");
    assert!(filter.contains_secret(&google_key));
    assert!(filter.mask(&google_key).contains("[MASKED GOOGLE KEY]"));

    // Test 15-digit Amex card detection and masking (MED-06)
    let amex_card = "378282246310005";
    assert!(filter.contains_secret(amex_card));
    assert!(filter.mask(amex_card).contains("[MASKED CARD]"));

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
fn test_crypto_engine_minimum_ciphertext_length() {
    use clipboard_history_core::security::{CryptoEngine, CryptoError};
    let engine = CryptoEngine::new_from_key([0x42u8; 32]);

    // Payloads strictly smaller than 28 bytes (12-byte nonce + 16-byte Poly1305 tag) must fail with CiphertextTooShort
    for len in 0..28 {
        let dummy = vec![0u8; len];
        let err = engine.decrypt(&dummy).unwrap_err();
        match err {
            CryptoError::CiphertextTooShort => {}
            other => panic!("Expected CiphertextTooShort for payload len {}, got: {:?}", len, other),
        }
    }

    // A validly encrypted payload (even of empty plaintext) has length >= 28
    let encrypted = engine.encrypt(b"").expect("Failed to encrypt empty slice");
    assert_eq!(encrypted.len(), 28);
    let decrypted = engine.decrypt(&encrypted).expect("Failed to decrypt");
    assert_eq!(decrypted, b"");
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

#[test]
fn test_snippets_crud_sqlite() {
    use clipboard_history_core::domain::Snippet;
    use clipboard_history_core::storage::SqliteRepository;

    let repo = SqliteRepository::open_in_memory().expect("Failed to open in-memory db");

    let snippet1 = Snippet::new(
        "Greeting".to_string(),
        "Hello {clipboard}, welcome!".to_string(),
        Some("Templates".to_string()),
    );
    let snippet2 = Snippet::new(
        "Email Sig".to_string(),
        "Best regards,\nAli".to_string(),
        Some("Signatures".to_string()),
    );

    // Insert
    let inserted1 = repo
        .insert_snippet(&snippet1)
        .expect("Insert snippet 1 failed");
    let inserted2 = repo
        .insert_snippet(&snippet2)
        .expect("Insert snippet 2 failed");
    assert_eq!(inserted1.label, "Greeting");
    assert_eq!(inserted2.label, "Email Sig");

    // List all
    let all = repo.list_snippets(None).expect("List snippets failed");
    assert_eq!(all.len(), 2);

    // Filter by category
    let templates = repo
        .list_snippets(Some("Templates"))
        .expect("Filter snippets failed");
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].label, "Greeting");

    // Get by ID
    let fetched = repo.get_snippet(&snippet1.id).expect("Get snippet failed");
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().content, "Hello {clipboard}, welcome!");

    // Touch snippet (updates last_used_at)
    repo.touch_snippet(&snippet1.id)
        .expect("Touch snippet failed");

    // Update snippet
    let mut updated = snippet1.clone();
    updated.content = "Updated content".to_string();
    repo.update_snippet(&updated)
        .expect("Update snippet failed");
    let fetched_updated = repo.get_snippet(&snippet1.id).unwrap().unwrap();
    assert_eq!(fetched_updated.content, "Updated content");

    // Delete snippet
    let deleted = repo
        .delete_snippet(&snippet1.id)
        .expect("Delete snippet failed");
    assert!(deleted);
    let after_delete = repo.list_snippets(None).unwrap();
    assert_eq!(after_delete.len(), 1);
    assert_eq!(after_delete[0].id, snippet2.id);
}

#[test]
fn test_snippet_template_expansion() {
    use clipboard_history_core::transforms::SnippetExpander;

    let template = "Date: {date}, Time: {time}, UUID: {uuid}, Clip: [{clipboard}]";
    let expanded = SnippetExpander::expand(template, Some("CopiedText"));

    assert!(expanded.contains("Date: 202")); // Year 202...
    assert!(expanded.contains("Time: "));
    assert!(expanded.contains("Clip: [CopiedText]"));
    assert!(!expanded.contains("{uuid}")); // UUID was replaced
    assert!(!expanded.contains("{date}"));
    assert!(!expanded.contains("{time}"));

    // Multiple UUIDs generate distinct identifiers
    let multi_uuid_tpl = "{uuid}_{uuid}";
    let multi_expanded = SnippetExpander::expand(multi_uuid_tpl, None);
    let parts: Vec<&str> = multi_expanded.split('_').collect();
    assert_eq!(parts.len(), 2);
    assert_ne!(parts[0], parts[1]);
}

#[test]
fn test_color_format_conversions() {
    use clipboard_history_core::transforms::TextTransforms;

    let color = TextTransforms::detect_color("#3B82F6").expect("Detect color failed");
    assert_eq!(color.to_hex_string(), "#3B82F6");
    assert_eq!(color.to_rgb_string(), "rgb(59, 130, 246)");
    assert_eq!(
        color.to_css_var("primary-color"),
        "--primary-color: #3B82F6;"
    );

    let hsl = color.to_hsl_string();
    assert!(hsl.starts_with("hsl("));
    assert!(hsl.ends_with("%)"));

    let glsl = color.to_glsl_vec4();
    assert!(glsl.starts_with("vec4("));

    let swift = color.to_swift_ui();
    assert!(swift.starts_with("Color(red:"));
}

#[test]
fn test_search_syntax_parser() {
    use clipboard_history_core::domain::{ClipboardEntry, EntryType};
    use clipboard_history_core::search::parser::ParsedSearchQuery;

    let parsed = ParsedSearchQuery::parse("type:code app:firefox is:pinned query_term");
    assert_eq!(parsed.entry_type, Some(EntryType::Code));
    assert_eq!(parsed.source_app.as_deref(), Some("firefox"));
    assert_eq!(parsed.is_pinned, Some(true));
    assert_eq!(parsed.text_query, "query_term");
    assert!(!parsed.is_snippet);

    let mut entry = ClipboardEntry::new_text(
        "fn main() {}".to_string(),
        "hash".to_string(),
        vec!["text/plain".to_string()],
        Some("Firefox Browser".to_string()),
    );
    entry.entry_type = EntryType::Code;
    entry.is_pinned = true;

    assert!(parsed.matches_entry(&entry));

    // Mismatched type
    entry.entry_type = EntryType::Text;
    assert!(!parsed.matches_entry(&entry));

    // Snippets token
    let snippet_query = ParsedSearchQuery::parse("is:snippet response");
    assert!(snippet_query.is_snippet);
    assert_eq!(snippet_query.text_query, "response");
    assert!(!snippet_query.matches_entry(&entry));
    entry.source_app = Some("Snippet".to_string());
    assert!(snippet_query.matches_entry(&entry));
}

#[test]
fn test_lan_sync_crypto_roundtrip() {
    use clipboard_history_core::sync::lan::LanCrypto;

    let pin = "123456";
    let data = b"Confidential clipboard payload transmitted over local WiFi network";

    // Encrypt
    let encrypted = LanCrypto::encrypt(pin, data).expect("Encryption failed");
    assert_ne!(encrypted, data);
    assert!(encrypted.len() > 12); // Must contain nonce + tag

    // Decrypt with correct PIN
    let decrypted = LanCrypto::decrypt(pin, &encrypted).expect("Decryption failed");
    assert_eq!(decrypted, data);

    // Decrypt with wrong PIN must fail
    let wrong_pin = "654321";
    let wrong_result = LanCrypto::decrypt(wrong_pin, &encrypted);
    assert!(wrong_result.is_err());
}

#[test]
fn test_text_concatenation_delimiters() {
    use clipboard_history_core::transforms::{ConcatDelimiter, TextTransforms};

    let items = ["Apple", "Banana", "Cherry"];

    // Newlines
    assert_eq!(
        TextTransforms::concatenate(&items, &ConcatDelimiter::Newline),
        "Apple\nBanana\nCherry"
    );

    // Paragraphs
    assert_eq!(
        TextTransforms::concatenate(&items, &ConcatDelimiter::DoubleNewline),
        "Apple\n\nBanana\n\nCherry"
    );

    // Comma
    assert_eq!(
        TextTransforms::concatenate(&items, &ConcatDelimiter::Comma),
        "Apple, Banana, Cherry"
    );

    // Numbered List
    assert_eq!(
        TextTransforms::concatenate(&items, &ConcatDelimiter::NumberedList),
        "1. Apple\n2. Banana\n3. Cherry"
    );

    // Bullet List
    assert_eq!(
        TextTransforms::concatenate(&items, &ConcatDelimiter::BulletList),
        "- Apple\n- Banana\n- Cherry"
    );

    // Custom Delimiter
    assert_eq!(
        TextTransforms::concatenate(&items, &ConcatDelimiter::Custom(" | ".to_string())),
        "Apple | Banana | Cherry"
    );
}

#[test]
fn test_diff_engine_computation() {
    use clipboard_history_core::transforms::{DiffEngine, DiffTag};

    let text_a = "fn main() {\n    println!(\"Hello\");\n}\n";
    let text_b = "fn main() {\n    println!(\"Hello, World!\");\n    // Added\n}\n";

    let diff = DiffEngine::compute_diff(text_a, text_b);

    assert_eq!(diff.additions, 2);
    assert_eq!(diff.deletions, 1);
    assert!(diff.unified.contains("@@"));
    assert!(diff.unified.contains("-    println!(\"Hello\");"));
    assert!(diff.unified.contains("+    println!(\"Hello, World!\");"));
    assert!(diff.unified.contains("+    // Added"));

    // Verify tag counts in lines
    let insert_count = diff
        .lines
        .iter()
        .filter(|l| l.tag == DiffTag::Insert)
        .count();
    let delete_count = diff
        .lines
        .iter()
        .filter(|l| l.tag == DiffTag::Delete)
        .count();
    let equal_count = diff
        .lines
        .iter()
        .filter(|l| l.tag == DiffTag::Equal)
        .count();

    assert_eq!(insert_count, 2);
    assert_eq!(delete_count, 1);
    assert!(equal_count >= 2);
}

#[test]
fn test_sqlite_batch_operations() {
    use clipboard_history_core::blob::BlobStore;
    use clipboard_history_core::domain::ClipboardEntry;
    use clipboard_history_core::storage::SqliteRepository;

    let repo = SqliteRepository::open_in_memory().expect("Failed to open in-memory db");

    let mut ids = Vec::new();
    for i in 0..5 {
        let text = format!("Batch item {}", i);
        let hash = BlobStore::compute_hash(text.as_bytes());
        let entry = ClipboardEntry::new_text(text, hash, vec!["text/plain".to_string()], None);
        let inserted = repo.insert_or_update(&entry).unwrap();
        ids.push(inserted.id);
    }
    assert_eq!(repo.count().unwrap(), 5);

    // Batch pin first 3 items
    let pinned_count = repo.batch_set_pinned(&ids[0..3], true).unwrap();
    assert_eq!(pinned_count, 3);
    for id in &ids[0..3] {
        let item = repo.get_by_id(id).unwrap();
        assert!(item.is_pinned);
    }
    for id in &ids[3..5] {
        let item = repo.get_by_id(id).unwrap();
        assert!(!item.is_pinned);
    }

    // Batch delete last 2 items
    let deleted_count = repo.batch_delete(&ids[3..5]).unwrap();
    assert_eq!(deleted_count, 2);
    assert_eq!(repo.count().unwrap(), 3);

    // Batch unpin remaining
    let unpinned_count = repo.batch_set_pinned(&ids[0..3], false).unwrap();
    assert_eq!(unpinned_count, 3);
    for id in &ids[0..3] {
        let item = repo.get_by_id(id).unwrap();
        assert!(!item.is_pinned);
    }
}

#[test]
fn test_url_cleaner_tracking_params() {
    use clipboard_history_core::transforms::UrlCleaner;

    // 1. UTM and common ads parameters
    let raw1 = "https://example.com/article?utm_source=twitter&utm_medium=social&utm_campaign=spring2026&id=123";
    let cleaned1 = UrlCleaner::clean_tracking_params(raw1);
    assert_eq!(cleaned1, "https://example.com/article?id=123");

    // 2. All query params are tracking params
    let raw2 = "https://example.com/shop?utm_source=email&fbclid=IwAR123456&gclid=CjwKCAiA";
    let cleaned2 = UrlCleaner::clean_tracking_params(raw2);
    assert_eq!(cleaned2, "https://example.com/shop");

    // 3. YouTube link: preserve 'v', strip 'si' and 'feature'
    let yt = "https://www.youtube.com/watch?v=dQw4w9WgXcQ&si=u1pP9z&feature=shared";
    let cleaned_yt = UrlCleaner::clean_tracking_params(yt);
    assert_eq!(cleaned_yt, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");

    // 4. Twitter / X link: strip 's' and 't'
    let x_url = "https://x.com/rustlang/status/1234567890?s=20&t=a1b2c3d4";
    let cleaned_x = UrlCleaner::clean_tracking_params(x_url);
    assert_eq!(cleaned_x, "https://x.com/rustlang/status/1234567890");

    // 5. Amazon URL: strip /ref= in path and tracking queries
    let amz =
        "https://www.amazon.com/dp/B08N5WRWNW/ref=sr_1_1?keywords=keyboard&qid=12345&ref_=as_li";
    let cleaned_amz = UrlCleaner::clean_tracking_params(amz);
    assert_eq!(
        cleaned_amz,
        "https://www.amazon.com/dp/B08N5WRWNW?keywords=keyboard&qid=12345"
    );

    // 6. Fragment preservation
    let frag_url = "https://docs.rs/similar/latest/similar/?utm_medium=referral#functions";
    let cleaned_frag = UrlCleaner::clean_tracking_params(frag_url);
    assert_eq!(
        cleaned_frag,
        "https://docs.rs/similar/latest/similar/#functions"
    );

    // 7. Text block with embedded URLs and trailing punctuation
    let text = "Check out https://youtube.com/watch?v=abc&si=track1. Also visit https://example.com/test?fbclid=xyz, thanks!";
    let cleaned_text = UrlCleaner::clean_text_urls(text);
    assert_eq!(
        cleaned_text,
        "Check out https://youtube.com/watch?v=abc. Also visit https://example.com/test, thanks!"
    );

    // 8. Meta/Instagram, Microsoft, Mailchimp, and Google query preservation
    let google_url = "https://www.google.com/search?q=rust+lang&gclid=Cj0KCQ&dclid=123&gad_source=1&gbraid=456&wbraid=789";
    let cleaned_google = UrlCleaner::clean_tracking_params(google_url);
    assert_eq!(cleaned_google, "https://www.google.com/search?q=rust+lang");

    let meta_url =
        "https://instagram.com/p/abc123xyz/?igshid=YmMyMTA2M2Y=&fb_source=feed&fb_custom=test";
    let cleaned_meta = UrlCleaner::clean_tracking_params(meta_url);
    assert_eq!(cleaned_meta, "https://instagram.com/p/abc123xyz/");

    let ms_mc_url = "https://store.example.com/item?id=99&msclkid=ms123&mc_cid=cid456&mc_eid=eid789&recipient_id=rec1";
    let cleaned_ms_mc = UrlCleaner::clean_tracking_params(ms_mc_url);
    assert_eq!(cleaned_ms_mc, "https://store.example.com/item?id=99");

    // 9. has_tracking_params detection
    assert!(UrlCleaner::has_tracking_params(
        "https://test.com?utm_source=test"
    ));
    assert!(UrlCleaner::has_tracking_params(
        "https://test.com?fb_source=newsfeed"
    ));
    assert!(!UrlCleaner::has_tracking_params(
        "https://test.com/search?q=rust"
    ));
    assert!(!UrlCleaner::has_tracking_params("Plain text without URLs"));
}

#[test]
fn test_app_filter_rules() {
    use clipboard_history_core::config::{AppConfig, AppFilterMode, SecurityConfig};

    let mut sec = SecurityConfig::default();
    assert_eq!(sec.app_filter_mode, AppFilterMode::Blacklist);
    assert!(sec.app_filter_list.is_empty());

    // 1. Blacklist mode: default legacy classes (KeePassXC, 1Password, Bitwarden) are blocked
    assert!(!sec.is_app_allowed(Some("org.keepassxc.KeePassXC"), None));
    assert!(!sec.is_app_allowed(None, Some("1password-gui")));
    assert!(sec.is_app_allowed(Some("Alacritty"), Some("alacritty")));
    assert!(sec.is_app_allowed(None, None));

    // 2. Blacklist mode: adding app to filter list
    sec.app_filter_list.push("slack".to_string());
    sec.app_filter_list.push("discord".to_string());

    assert!(!sec.is_app_allowed(Some("Slack"), None));
    assert!(!sec.is_app_allowed(None, Some("discord")));
    assert!(sec.is_app_allowed(Some("firefox"), Some("Navigator")));

    // 3. Whitelist mode with empty list allows all
    sec.app_filter_mode = AppFilterMode::Whitelist;
    sec.app_filter_list.clear();
    assert!(sec.is_app_allowed(Some("firefox"), None));
    assert!(sec.is_app_allowed(Some("Slack"), None));

    // 4. Whitelist mode with configured allowed apps
    sec.app_filter_list.push("code".to_string());
    sec.app_filter_list.push("terminal".to_string());

    // Matches allowed list
    assert!(sec.is_app_allowed(Some("Visual Studio Code"), Some("code")));
    assert!(sec.is_app_allowed(Some("gnome-terminal"), None));

    // Does not match allowed list -> blocked
    assert!(!sec.is_app_allowed(Some("Slack"), Some("slack")));
    assert!(!sec.is_app_allowed(Some("firefox"), Some("Navigator")));
    assert!(!sec.is_app_allowed(None, None));

    // 5. TOML Serialization and Deserialization round-trip
    let mut config = AppConfig::default();
    config.security.app_filter_mode = AppFilterMode::Whitelist;
    config.security.app_filter_list = vec!["firefox".to_string(), "alacritty".to_string()];
    config.security.auto_clean_tracking_urls = true;

    let toml_str = toml::to_string(&config).expect("Serialization failed");
    let loaded: AppConfig = toml::from_str(&toml_str).expect("Deserialization failed");

    assert_eq!(loaded.security.app_filter_mode, AppFilterMode::Whitelist);
    assert_eq!(
        loaded.security.app_filter_list,
        vec!["firefox".to_string(), "alacritty".to_string()]
    );
    assert!(loaded.security.auto_clean_tracking_urls);
}

#[test]
fn test_sqlite_prefix_wipeout_prevention() {
    use clipboard_history_core::blob::BlobStore;
    use clipboard_history_core::domain::ClipboardEntry;
    use clipboard_history_core::storage::SqliteRepository;

    let repo = SqliteRepository::open_in_memory().expect("Failed to open in-memory db");

    let mut ids = Vec::new();
    for i in 0..5 {
        let text = format!("Safety item {}", i);
        let hash = BlobStore::compute_hash(text.as_bytes());
        let entry = ClipboardEntry::new_text(text, hash, vec!["text/plain".to_string()], None);
        let inserted = repo.insert_or_update(&entry).unwrap();
        ids.push(inserted.id);
    }
    assert_eq!(repo.count().unwrap(), 5);

    // 1. Empty string deletion must fail and delete 0 items
    assert!(repo.delete("").is_err());
    assert!(repo.delete("   ").is_err());
    assert_eq!(repo.count().unwrap(), 5);

    // 2. Batch delete with empty string or spaces must not delete records
    let deleted = repo.batch_delete(&["".to_string(), "  ".to_string()]).unwrap();
    assert_eq!(deleted, 0);
    assert_eq!(repo.count().unwrap(), 5);

    // 3. Pinning empty string must fail
    assert!(repo.set_pinned("", true).is_err());
    assert_eq!(repo.count().unwrap(), 5);

    // 4. get_by_id on empty string must fail
    assert!(repo.get_by_id("").is_err());

    // 5. Query with '%' wildcard must not wipe table
    assert!(repo.delete("%").is_err());
    assert_eq!(repo.count().unwrap(), 5);

    // 6. Valid prefix deletion with >= 6 chars should work for target entry only
    let target_prefix = &ids[0][..8];
    assert!(repo.delete(target_prefix).is_ok());
    assert_eq!(repo.count().unwrap(), 4);
    assert!(repo.get_by_id(&ids[0]).is_err());
}

#[test]
fn test_preview_generation_performance_and_accuracy() {
    use clipboard_history_core::blob::BlobStore;
    use clipboard_history_core::domain::ClipboardEntry;

    // Create a 50KB payload
    let large_text = "fn compute_something() -> Result<()> {\n    let x = 42;\n    println!(\"Hello\");\n}\n".repeat(1000);
    let hash = BlobStore::compute_hash(large_text.as_bytes());
    let entry = ClipboardEntry::new_text(large_text, hash, vec!["text/plain".to_string()], None);

    assert!(entry.preview.chars().count() <= 140);
    assert!(entry.preview.ends_with("..."));
    assert!(entry.preview.starts_with("fn compute_something()"));
}

#[test]
fn test_lan_sync_salted_randomness() {
    use clipboard_history_core::sync::lan::LanCrypto;

    let pin = "987654";
    let data = b"Test multi-packet salted entropy";

    let enc1 = LanCrypto::encrypt(pin, data).expect("Enc 1 failed");
    let enc2 = LanCrypto::encrypt(pin, data).expect("Enc 2 failed");

    // Ciphertexts must differ due to random salt & nonce
    assert_ne!(enc1, enc2);

    // Both must decrypt back to original plaintext
    let dec1 = LanCrypto::decrypt(pin, &enc1).expect("Dec 1 failed");
    let dec2 = LanCrypto::decrypt(pin, &enc2).expect("Dec 2 failed");
    assert_eq!(dec1, data);
    assert_eq!(dec2, data);
}

#[test]
fn test_sqlite_search_structured_parser_and_fts() {
    use clipboard_history_core::blob::BlobStore;
    use clipboard_history_core::domain::ClipboardEntry;
    use clipboard_history_core::storage::SqliteRepository;

    let repo = SqliteRepository::open_in_memory().expect("Failed to open in-memory db");

    let text1 = "fn main() { println!(\"Rust rules\"); }";
    let hash1 = BlobStore::compute_hash(text1.as_bytes());
    let mut entry1 = ClipboardEntry::new_text(
        text1.to_string(),
        hash1,
        vec!["text/plain".to_string()],
        Some("vscode".to_string()),
    );
    entry1.entry_type = clipboard_history_core::domain::EntryType::Code;
    repo.insert_or_update(&entry1).unwrap();

    let text2 = "Hey team, remember the meeting at 3pm";
    let hash2 = BlobStore::compute_hash(text2.as_bytes());
    let entry2 = ClipboardEntry::new_text(
        text2.to_string(),
        hash2,
        vec!["text/plain".to_string()],
        Some("slack".to_string()),
    );
    repo.insert_or_update(&entry2).unwrap();

    // 1. Search with type:code filter
    let code_results = repo.search("type:code", 10, 0).unwrap();
    assert_eq!(code_results.len(), 1);
    assert_eq!(code_results[0].id, entry1.id);

    // 2. Search with app:slack filter
    let slack_results = repo.search("app:slack", 10, 0).unwrap();
    assert_eq!(slack_results.len(), 1);
    assert_eq!(slack_results[0].id, entry2.id);

    // 3. Search with FTS text query
    let fts_results = repo.search("Rust", 10, 0).unwrap();
    assert_eq!(fts_results.len(), 1);
    assert_eq!(fts_results[0].id, entry1.id);

    // 4. Combined query: type:code and text
    let combined_results = repo.search("type:code println", 10, 0).unwrap();
    assert_eq!(combined_results.len(), 1);
    assert_eq!(combined_results[0].id, entry1.id);
}

#[tokio::test]
async fn test_lan_sync_incremental_reading_and_limits() {
    use clipboard_history_core::sync::{read_sync_message, send_sync_message, SyncMessage};

    let pin = "123456";
    let msg = SyncMessage::Ping;

    let mut buf = Vec::new();
    send_sync_message(&mut buf, pin, &msg).await.unwrap();

    // Read message back
    let mut reader = &buf[..];
    let read_back = read_sync_message(&mut reader, pin).await.unwrap();
    assert!(matches!(read_back, SyncMessage::Ping));

    // Verify oversized header (>16MB) fails immediately without allocation
    let oversized_len = (20 * 1024 * 1024u32).to_be_bytes();
    let mut reader = &oversized_len[..];
    let err = read_sync_message(&mut reader, pin).await;
    assert!(err.is_err());
}

#[test]
fn test_sync_message_blob_payload_roundtrip() {
    use clipboard_history_core::blob::BlobStore;
    use clipboard_history_core::domain::ClipboardEntry;
    use clipboard_history_core::sync::SyncMessage;

    let fake_img_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let blob_hash = BlobStore::compute_hash(&fake_img_bytes);
    let entry = ClipboardEntry::new_image(
        blob_hash.clone(),
        None,
        fake_img_bytes.len(),
        (100, 100),
        vec!["image/png".to_string()],
        Some("Gimp".to_string()),
    );

    let msg = SyncMessage::EntrySync {
        origin_device: "desktop-laptop".to_string(),
        entry: Box::new(entry),
        blob_payload: Some(fake_img_bytes.clone()),
    };

    let serialized = serde_json::to_vec(&msg).unwrap();
    let deserialized: SyncMessage = serde_json::from_slice(&serialized).unwrap();

    if let SyncMessage::EntrySync {
        origin_device,
        entry,
        blob_payload,
    } = deserialized
    {
        assert_eq!(origin_device, "desktop-laptop");
        assert_eq!(entry.blob_hash.as_deref(), Some(blob_hash.as_str()));
        assert_eq!(blob_payload, Some(fake_img_bytes));
    } else {
        panic!("Deserialized message is not EntrySync");
    }
}

#[test]
fn test_storage_trait_abstraction() {
    use clipboard_history_core::domain::ClipboardEntry;
    use clipboard_history_core::storage::{SqliteRepository, Storage};

    let repo = SqliteRepository::open_in_memory().expect("In-memory SQLite failed");
    let storage: &dyn Storage = &repo;

    assert_eq!(storage.count().unwrap(), 0);

    let entry = ClipboardEntry::new_text(
        "Trait test payload".to_string(),
        "hash_trait_test".to_string(),
        vec!["text/plain".to_string()],
        Some("TestRunner".to_string()),
    );

    let inserted = storage.insert_or_update(&entry).expect("Insert via trait failed");
    assert_eq!(storage.count().unwrap(), 1);

    let fetched = storage.get_by_id(&inserted.id).expect("Get by ID via trait failed");
    assert_eq!(fetched.preview, "Trait test payload");

    let results = storage.search("Trait", 10, 0).expect("Search via trait failed");
    assert_eq!(results.len(), 1);

    storage.delete(&inserted.id).expect("Delete via trait failed");
    assert_eq!(storage.count().unwrap(), 0);
}

#[test]
fn test_app_config_dirs_safe_fallbacks() {
    use clipboard_history_core::config::AppConfig;

    // Verify config_dir and data_dir do not panic and return valid paths
    let config_dir = AppConfig::config_dir();
    assert!(!config_dir.as_os_str().is_empty());

    let data_dir = AppConfig::data_dir();
    assert!(!data_dir.as_os_str().is_empty());

    let config_path = AppConfig::config_path();
    assert!(config_path.ends_with("config.toml"));

    let db_path = AppConfig::db_path();
    assert!(db_path.ends_with("history.db"));

    let socket_path = AppConfig::socket_path();
    assert!(socket_path.to_str().unwrap().contains("sock"));
}


