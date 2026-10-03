use crate::error::{CoreError, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

#[derive(Clone, Debug)]
pub struct BlobStore {
    root_dir: PathBuf,
}

impl BlobStore {
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Result<Self> {
        let root = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        info!("BlobStore initialized at {}", root.display());
        Ok(Self { root_dir: root })
    }

    pub fn compute_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    pub fn save(&self, data: &[u8]) -> Result<String> {
        let hash = Self::compute_hash(data);
        let blob_path = self.path_for(&hash);

        if blob_path.exists() {
            debug!("Blob with hash {} already exists, skipping write", hash);
            return Ok(hash);
        }

        // Write atomically via temporary file
        let temp_path = self.root_dir.join(format!(".tmp_{}", uuid::Uuid::new_v4()));
        {
            let mut file = fs::File::create(&temp_path)?;
            let mut perms = file.metadata()?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&temp_path, perms)?;
            file.write_all(data)?;
            file.flush()?;
        }

        fs::rename(&temp_path, &blob_path)?;
        debug!("Saved blob {} ({} bytes)", hash, data.len());
        Ok(hash)
    }

    pub fn read(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.path_for(hash);
        if !path.exists() {
            return Err(CoreError::BlobNotFound(hash.to_string()));
        }
        let bytes = fs::read(path)?;
        Ok(bytes)
    }

    /// Saves raw data encrypted with AES-256-GCM.
    pub fn save_encrypted(
        &self,
        data: &[u8],
        crypto: &crate::security::CryptoEngine,
    ) -> Result<String> {
        let encrypted_bytes = crypto.encrypt(data).map_err(CoreError::Crypto)?;
        self.save(&encrypted_bytes)
    }

    /// Reads an encrypted blob and decrypts it with AES-256-GCM.
    pub fn read_decrypted(
        &self,
        hash: &str,
        crypto: &crate::security::CryptoEngine,
    ) -> Result<Vec<u8>> {
        let ciphertext = self.read(hash)?;
        crypto.decrypt(&ciphertext).map_err(CoreError::Crypto)
    }

    pub fn exists(&self, hash: &str) -> bool {
        self.path_for(hash).exists()
    }

    pub fn delete(&self, hash: &str) -> Result<()> {
        let path = self.path_for(hash);
        if path.exists() {
            fs::remove_file(path)?;
            debug!("Deleted blob {}", hash);
        }
        Ok(())
    }

    pub fn path_for(&self, hash: &str) -> PathBuf {
        self.root_dir.join(format!("{}.blob", hash))
    }

    pub fn cleanup_orphans(&self, active_hashes: &[String]) -> Result<usize> {
        let mut deleted = 0;
        let entries = fs::read_dir(&self.root_dir)?;
        let active_set: std::collections::HashSet<&str> =
            active_hashes.iter().map(|s| s.as_str()).collect();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                if filename.starts_with(".tmp_") {
                    let _ = fs::remove_file(&path);
                    continue;
                }

                if let Some(hash) = filename.strip_suffix(".blob") {
                    if !active_set.contains(hash) {
                        debug!("Removing orphaned blob: {}", filename);
                        if fs::remove_file(&path).is_ok() {
                            deleted += 1;
                        }
                    }
                }
            }
        }

        info!("Cleaned up {} orphaned blobs", deleted);
        Ok(deleted)
    }
}
