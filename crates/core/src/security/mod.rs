pub mod crypto;
pub mod filter;
pub mod memory;
pub mod password_manager;

pub use crypto::{CryptoEngine, CryptoError};
pub use filter::{SecretFilter, SecretHandlingPolicy};
pub use memory::{
    apply_process_memory_shield, lock_process_memory, protect_sensitive_buffer, wipe_and_protect,
};
pub use password_manager::PasswordManagerGuard;
