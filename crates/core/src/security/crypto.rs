use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: authenticated tag mismatch or corrupted data")]
    DecryptionFailed,
    #[error("Ciphertext too short: expected at least 28 bytes (12-byte nonce + 16-byte tag)")]
    CiphertextTooShort,
    #[error("Invalid UTF-8 string after decryption: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),
    #[error("Hex decode error: {0}")]
    HexError(#[from] hex::FromHexError),
}

pub struct CryptoEngine {
    cipher: Aes256Gcm,
}

impl CryptoEngine {
    /// Creates a new `CryptoEngine` with a 256-bit key.
    pub fn new_from_key(mut key_bytes: [u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        key_bytes.zeroize();
        Self { cipher }
    }

    /// Loads an existing 256-bit key from `key_path` or generates a new one with 0600 POSIX permissions.
    pub fn load_or_create<P: AsRef<Path>>(key_path: P) -> Result<Self, CryptoError> {
        let path = key_path.as_ref();
        if path.exists() {
            let mut file = OpenOptions::new().read(true).open(path)?;
            let mut key_bytes = Zeroizing::new([0u8; 32]);
            file.read_exact(key_bytes.as_mut_slice())?;
            Ok(Self::new_from_key(*key_bytes))
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
                }
            }

            let mut key_bytes = Zeroizing::new([0u8; 32]);
            let raw_key = Aes256Gcm::generate_key(&mut OsRng);
            key_bytes.copy_from_slice(&raw_key);

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)?;
                file.write_all(key_bytes.as_slice())?;
            }

            #[cfg(not(unix))]
            {
                let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
                file.write_all(key_bytes.as_slice())?;
            }

            Ok(Self::new_from_key(*key_bytes))
        }
    }

    /// Encrypts plaintext bytes using AES-256-GCM with a random 96-bit nonce.
    /// Returns `[12-byte Nonce | Ciphertext + 16-byte Tag]`.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(format!("{:?}", e)))?;

        let mut output = Vec::with_capacity(nonce.len() + ciphertext.len());
        output.extend_from_slice(nonce.as_slice());
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    /// Minimum valid payload length for AES-256-GCM: 12-byte nonce + 16-byte Poly1305 authentication tag.
    pub const MIN_CIPHERTEXT_LEN: usize = 12 + 16;

    /// Decrypts a buffer structured as `[12-byte Nonce | Ciphertext + 16-byte Tag]`.
    pub fn decrypt(&self, payload: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if payload.len() < Self::MIN_CIPHERTEXT_LEN {
            return Err(CryptoError::CiphertextTooShort);
        }

        let (nonce_bytes, ciphertext) = payload.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::DecryptionFailed)
    }

    /// Encrypts a UTF-8 string and returns a hex-encoded string prefixed with `$ENC$`.
    pub fn encrypt_str(&self, text: &str) -> Result<String, CryptoError> {
        let encrypted_bytes = self.encrypt(text.as_bytes())?;
        Ok(format!("$ENC${}", hex::encode(encrypted_bytes)))
    }

    /// Decrypts a string produced by `encrypt_str`.
    pub fn decrypt_str(&self, encoded: &str) -> Result<String, CryptoError> {
        if let Some(hex_part) = encoded.strip_prefix("$ENC$") {
            let bytes = hex::decode(hex_part)?;
            let decrypted_bytes = self.decrypt(&bytes)?;
            Ok(String::from_utf8(decrypted_bytes)?)
        } else {
            // Unencrypted plaintext fallback
            Ok(encoded.to_string())
        }
    }
}
