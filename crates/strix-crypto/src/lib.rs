//! Cryptographic utilities for Strix.
//!
//! Provides hashing, HMAC, encryption, and encoding utilities needed for S3 compatibility.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use hmac::{Hmac, Mac};
use md5::{Digest as Md5Digest, Md5};
use rand::Rng;
use sha2::Sha256;

/// AES-GCM nonce size (96 bits / 12 bytes)
pub const NONCE_SIZE: usize = 12;

/// AES-256 key size (256 bits / 32 bytes)
pub const KEY_SIZE: usize = 32;

/// AES-GCM authentication tag size (128 bits / 16 bytes)
pub const TAG_SIZE: usize = 16;

/// Compute SHA-256 hash and return as hex string.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Compute SHA-256 hash and return raw bytes.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Compute MD5 hash and return as hex string (for ETags).
pub fn md5_hex(data: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Compute MD5 hash and return as base64 (for Content-MD5 header).
pub fn md5_base64(data: &[u8]) -> String {
    use base64::Engine;
    let mut hasher = Md5::new();
    hasher.update(data);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

/// Compute HMAC-SHA256.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// Compute HMAC-SHA256 and return as hex string.
pub fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    hex::encode(hmac_sha256(key, data))
}

/// Format an ETag from MD5 hash (quoted hex string).
pub fn format_etag(md5_hash: &str) -> String {
    format!("\"{}\"", md5_hash)
}

/// Format a multipart ETag from part ETags.
pub fn format_multipart_etag(part_etags: &[String]) -> String {
    // Multipart ETag is MD5 of concatenated MD5s, followed by -N
    let mut combined = Vec::new();
    for etag in part_etags {
        // Remove quotes and convert hex to bytes
        let hex_str = etag.trim_matches('"');
        if let Ok(bytes) = hex::decode(hex_str) {
            combined.extend(bytes);
        }
    }

    let mut hasher = Md5::new();
    hasher.update(&combined);
    let hash = hex::encode(hasher.finalize());

    format!("\"{}-{}\"", hash, part_etags.len())
}

/// Encode bytes as hex string.
pub fn to_hex(data: &[u8]) -> String {
    hex::encode(data)
}

/// Decode hex string to bytes.
pub fn from_hex(s: &str) -> Result<Vec<u8>, hex::FromHexError> {
    hex::decode(s)
}

/// Encode bytes as base64.
pub fn to_base64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Decode base64 to bytes.
pub fn from_base64(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s)
}

// === Encryption (SSE) ===

/// Error type for encryption operations.
#[derive(Debug, Clone)]
pub enum EncryptionError {
    /// Invalid key size (must be 32 bytes for AES-256)
    InvalidKeySize,
    /// Invalid nonce size (must be 12 bytes)
    InvalidNonceSize,
    /// Encryption failed
    EncryptionFailed,
    /// Decryption failed (authentication failed or corrupted data)
    DecryptionFailed,
    /// Invalid ciphertext (too short)
    InvalidCiphertext,
}

impl std::fmt::Display for EncryptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncryptionError::InvalidKeySize => write!(f, "Invalid key size (must be 32 bytes)"),
            EncryptionError::InvalidNonceSize => write!(f, "Invalid nonce size (must be 12 bytes)"),
            EncryptionError::EncryptionFailed => write!(f, "Encryption failed"),
            EncryptionError::DecryptionFailed => write!(f, "Decryption failed"),
            EncryptionError::InvalidCiphertext => write!(f, "Invalid ciphertext"),
        }
    }
}

impl std::error::Error for EncryptionError {}

/// Generate a random 256-bit encryption key.
pub fn generate_encryption_key() -> [u8; KEY_SIZE] {
    let mut key = [0u8; KEY_SIZE];
    rand::rng().fill(&mut key);
    key
}

/// Generate a random nonce for AES-GCM.
pub fn generate_nonce() -> [u8; NONCE_SIZE] {
    let mut nonce = [0u8; NONCE_SIZE];
    rand::rng().fill(&mut nonce);
    nonce
}

/// Derive an encryption key from a password/passphrase using HMAC-SHA256.
/// This is a simple KDF suitable for SSE-S3 where we derive from a master key.
pub fn derive_key(master_key: &[u8], context: &[u8]) -> [u8; KEY_SIZE] {
    hmac_sha256(master_key, context)
}

/// Encrypt data using AES-256-GCM.
///
/// Returns: nonce (12 bytes) || ciphertext || tag (16 bytes)
pub fn encrypt_aes256_gcm(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
    if key.len() != KEY_SIZE {
        return Err(EncryptionError::InvalidKeySize);
    }

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncryptionError::InvalidKeySize)?;
    let nonce_bytes = generate_nonce();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| EncryptionError::EncryptionFailed)?;

    // Prepend nonce to ciphertext
    let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend(ciphertext);

    Ok(result)
}

/// Encrypt data using AES-256-GCM with a provided nonce.
///
/// Returns: ciphertext || tag (16 bytes)
///
/// Warning: Never reuse a nonce with the same key!
pub fn encrypt_aes256_gcm_with_nonce(
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, EncryptionError> {
    if key.len() != KEY_SIZE {
        return Err(EncryptionError::InvalidKeySize);
    }
    if nonce.len() != NONCE_SIZE {
        return Err(EncryptionError::InvalidNonceSize);
    }

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncryptionError::InvalidKeySize)?;
    let nonce = Nonce::from_slice(nonce);

    cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| EncryptionError::EncryptionFailed)
}

/// Decrypt data encrypted with AES-256-GCM.
///
/// Input format: nonce (12 bytes) || ciphertext || tag (16 bytes)
pub fn decrypt_aes256_gcm(key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
    if key.len() != KEY_SIZE {
        return Err(EncryptionError::InvalidKeySize);
    }
    if ciphertext.len() < NONCE_SIZE + TAG_SIZE {
        return Err(EncryptionError::InvalidCiphertext);
    }

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncryptionError::InvalidKeySize)?;

    let nonce = Nonce::from_slice(&ciphertext[..NONCE_SIZE]);
    let encrypted_data = &ciphertext[NONCE_SIZE..];

    cipher
        .decrypt(nonce, encrypted_data)
        .map_err(|_| EncryptionError::DecryptionFailed)
}

/// Decrypt data with AES-256-GCM given a separate nonce.
///
/// Input format: ciphertext || tag (16 bytes)
pub fn decrypt_aes256_gcm_with_nonce(
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, EncryptionError> {
    if key.len() != KEY_SIZE {
        return Err(EncryptionError::InvalidKeySize);
    }
    if nonce.len() != NONCE_SIZE {
        return Err(EncryptionError::InvalidNonceSize);
    }
    if ciphertext.len() < TAG_SIZE {
        return Err(EncryptionError::InvalidCiphertext);
    }

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncryptionError::InvalidKeySize)?;
    let nonce = Nonce::from_slice(nonce);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| EncryptionError::DecryptionFailed)
}

/// Validate a customer-provided encryption key (SSE-C).
///
/// Returns Ok if the key is valid (32 bytes and MD5 matches if provided).
pub fn validate_sse_c_key(
    key_base64: &str,
    key_md5_base64: Option<&str>,
) -> Result<[u8; KEY_SIZE], EncryptionError> {
    let key = from_base64(key_base64).map_err(|_| EncryptionError::InvalidKeySize)?;

    if key.len() != KEY_SIZE {
        return Err(EncryptionError::InvalidKeySize);
    }

    // Validate MD5 if provided
    if let Some(md5_b64) = key_md5_base64 {
        let expected_md5 = md5_base64(&key);
        if md5_b64 != expected_md5 {
            return Err(EncryptionError::InvalidKeySize); // MD5 mismatch
        }
    }

    let mut result = [0u8; KEY_SIZE];
    result.copy_from_slice(&key);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256() {
        let hash = sha256_hex(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_md5() {
        let hash = md5_hex(b"hello world");
        assert_eq!(hash, "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }

    #[test]
    fn test_hmac() {
        let result = hmac_sha256_hex(b"key", b"message");
        assert_eq!(
            result,
            "6e9ef29b75fffc5b7abae527d58fdadb2fe42e7219011976917343065f58ed4a"
        );
    }

    #[test]
    fn test_etag_format() {
        assert_eq!(format_etag("abc123"), "\"abc123\"");
    }

    #[test]
    fn test_multipart_etag() {
        let parts = vec![
            "\"d41d8cd98f00b204e9800998ecf8427e\"".to_string(),
            "\"d41d8cd98f00b204e9800998ecf8427e\"".to_string(),
        ];
        let etag = format_multipart_etag(&parts);
        assert!(etag.ends_with("-2\""));
    }

    #[test]
    fn test_encryption_roundtrip() {
        let key = generate_encryption_key();
        let plaintext = b"Hello, World! This is a test message for encryption.";

        let ciphertext = encrypt_aes256_gcm(&key, plaintext).unwrap();
        let decrypted = decrypt_aes256_gcm(&key, &ciphertext).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encryption_with_nonce() {
        let key = generate_encryption_key();
        let nonce = generate_nonce();
        let plaintext = b"Test message";

        let ciphertext = encrypt_aes256_gcm_with_nonce(&key, &nonce, plaintext).unwrap();
        let decrypted = decrypt_aes256_gcm_with_nonce(&key, &nonce, &ciphertext).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encryption_different_keys() {
        let key1 = generate_encryption_key();
        let key2 = generate_encryption_key();
        let plaintext = b"Secret message";

        let ciphertext = encrypt_aes256_gcm(&key1, plaintext).unwrap();
        let result = decrypt_aes256_gcm(&key2, &ciphertext);

        assert!(result.is_err());
    }

    #[test]
    fn test_encryption_tampered_ciphertext() {
        let key = generate_encryption_key();
        let plaintext = b"Important data";

        let mut ciphertext = encrypt_aes256_gcm(&key, plaintext).unwrap();
        // Tamper with the ciphertext
        if let Some(byte) = ciphertext.get_mut(NONCE_SIZE + 5) {
            *byte ^= 0xFF;
        }

        let result = decrypt_aes256_gcm(&key, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_derive_key() {
        let master = b"master-secret-key-for-testing123";
        let context1 = b"object:bucket1/key1";
        let context2 = b"object:bucket1/key2";

        let key1 = derive_key(master, context1);
        let key2 = derive_key(master, context2);

        assert_ne!(key1, key2);
        assert_eq!(key1.len(), KEY_SIZE);
    }

    #[test]
    fn test_sse_c_key_validation() {
        let key = generate_encryption_key();
        let key_b64 = to_base64(&key);
        let key_md5 = md5_base64(&key);

        // Valid key with matching MD5
        let result = validate_sse_c_key(&key_b64, Some(&key_md5));
        assert!(result.is_ok());

        // Valid key without MD5
        let result = validate_sse_c_key(&key_b64, None);
        assert!(result.is_ok());

        // Invalid MD5
        let result = validate_sse_c_key(&key_b64, Some("wrong-md5"));
        assert!(result.is_err());
    }
}
