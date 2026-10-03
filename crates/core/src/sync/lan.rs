use crate::error::{CoreError, Result};
use crate::sync::protocol::SyncMessage;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Nonce};
use sha2::Sha256;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use aes_gcm::aead::rand_core::RngCore;
use pbkdf2::pbkdf2_hmac;

pub struct LanCrypto;

impl LanCrypto {
    pub fn derive_key(pin: &str, salt: &[u8; 16]) -> [u8; 32] {
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(pin.as_bytes(), salt, 100_000, &mut key);
        key
    }

    pub fn encrypt(pin: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        use crate::security::crypto::CryptoError;

        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);

        let key = Self::derive_key(pin, &salt);
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
        packet.extend_from_slice(&salt);
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
        let mut salt = [0u8; 16];
        salt.copy_from_slice(salt_bytes);

        let (nonce_bytes, ciphertext) = rest.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let key = Self::derive_key(pin, &salt);
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

/// Read and decrypt a SyncMessage from an AsyncRead stream
pub async fn read_sync_message<R: AsyncReadExt + Unpin>(
    reader: &mut R,
    pin: &str,
) -> Result<SyncMessage> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 16 * 1024 * 1024 {
        return Err(CoreError::Ipc("Payload exceeds 16MB limit".to_string()));
    }

    let mut buf = Vec::with_capacity(len.min(64 * 1024));
    let read_count = reader.take(len as u64).read_to_end(&mut buf).await?;
    if read_count != len {
        return Err(CoreError::Ipc(format!(
            "Incomplete LAN sync payload: expected {} bytes, received {}",
            len, read_count
        )));
    }

    let decrypted = LanCrypto::decrypt(pin, &buf)?;
    let msg: SyncMessage = serde_json::from_slice(&decrypted)
        .map_err(|e| CoreError::Ipc(format!("Deserialize error: {}", e)))?;
    Ok(msg)
}
