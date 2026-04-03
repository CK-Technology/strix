//! Secret encryption utilities for access key storage.
//!
//! Access key secrets are encrypted at rest using AES-256-GCM with a server-side key.

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use strix_crypto::{decrypt_aes256_gcm, encrypt_aes256_gcm};

use crate::{IamError, Result};

/// Encrypt a secret for storage.
///
/// Returns base64-encoded ciphertext that includes the nonce.
pub fn encrypt_secret(secret: &str, encryption_key: &[u8; 32]) -> Result<String> {
    let ciphertext = encrypt_aes256_gcm(encryption_key, secret.as_bytes())
        .map_err(|e| IamError::Encryption(e.to_string()))?;
    Ok(BASE64.encode(&ciphertext))
}

/// Decrypt a stored secret.
pub fn decrypt_secret(encrypted: &str, encryption_key: &[u8; 32]) -> Result<String> {
    let ciphertext = BASE64
        .decode(encrypted)
        .map_err(|e| IamError::Encryption(format!("Invalid base64: {}", e)))?;

    let plaintext = decrypt_aes256_gcm(encryption_key, &ciphertext)
        .map_err(|e| IamError::Encryption(e.to_string()))?;

    String::from_utf8(plaintext).map_err(|e| IamError::Encryption(format!("Invalid UTF-8: {}", e)))
}

/// Derive an encryption key from a passphrase.
///
/// Uses SHA-256 to derive a 256-bit key from the passphrase.
pub fn derive_encryption_key(passphrase: &str) -> [u8; 32] {
    strix_crypto::sha256(format!("strix-secret-encryption:{}", passphrase).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let key = derive_encryption_key("test-passphrase");
        let secret = "my-secret-access-key-value";

        let encrypted = encrypt_secret(secret, &key).unwrap();

        // Encrypted should be different from original
        assert_ne!(encrypted, secret);

        // Should decrypt back to original
        let decrypted = decrypt_secret(&encrypted, &key).unwrap();
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn test_different_keys_fail() {
        let key1 = derive_encryption_key("key1");
        let key2 = derive_encryption_key("key2");
        let secret = "my-secret";

        let encrypted = encrypt_secret(secret, &key1).unwrap();

        // Wrong key should fail to decrypt
        assert!(decrypt_secret(&encrypted, &key2).is_err());
    }
}
