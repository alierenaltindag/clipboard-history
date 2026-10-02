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
