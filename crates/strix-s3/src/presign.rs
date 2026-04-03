//! Pre-signed URL generation for S3 operations.

use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

type HmacSha256 = Hmac<Sha256>;

/// HTTP methods supported for pre-signed URLs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresignMethod {
    Get,
    Put,
    Delete,
}

impl PresignMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

/// Options for generating a pre-signed URL.
#[derive(Debug, Clone)]
pub struct PresignOptions {
    /// HTTP method
    pub method: PresignMethod,
    /// Bucket name
    pub bucket: String,
    /// Object key
    pub key: String,
    /// Expiration time in seconds (default 3600, max 604800)
    pub expires_in: u32,
    /// Content type (for PUT requests)
    pub content_type: Option<String>,
    /// AWS region (default "us-east-1")
    pub region: Option<String>,
}

/// Pre-signed URL generator.
pub struct PresignUrlGenerator {
    access_key: String,
    secret_key: String,
    endpoint: String,
    region: String,
}

impl PresignUrlGenerator {
    /// Create a new generator.
    pub fn new(
        access_key: String,
        secret_key: String,
        endpoint: String,
        region: Option<String>,
    ) -> Self {
        Self {
            access_key,
            secret_key,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            region: region.unwrap_or_else(|| "us-east-1".to_string()),
        }
    }

    /// Generate a pre-signed URL.
    pub fn generate(&self, opts: &PresignOptions) -> String {
        let region = opts.region.as_ref().unwrap_or(&self.region);
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();

        // Credential scope
        let credential_scope = format!("{}/{}/s3/aws4_request", date_stamp, region);
        let credential = format!("{}/{}", self.access_key, credential_scope);

        // URI path (URL encode the key)
        let uri_path = format!("/{}/{}", opts.bucket, urlencoding::encode(&opts.key));

        // Build query string parameters (in sorted order)
        let mut params = BTreeMap::new();
        params.insert("X-Amz-Algorithm", "AWS4-HMAC-SHA256".to_string());
        params.insert("X-Amz-Credential", credential);
        params.insert("X-Amz-Date", amz_date.clone());
        params.insert("X-Amz-Expires", opts.expires_in.to_string());
        params.insert("X-Amz-SignedHeaders", "host".to_string());

        // Build canonical query string (without signature)
        let canonical_qs = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        // Host header
        let host = self.endpoint.replace("http://", "").replace("https://", "");

        // Canonical headers
        let canonical_headers = format!("host:{}\n", host);
        let signed_headers = "host";

        // For pre-signed URLs, payload is UNSIGNED-PAYLOAD
        let payload_hash = "UNSIGNED-PAYLOAD";

        // Canonical request
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            opts.method.as_str(),
            uri_path,
            canonical_qs,
            canonical_headers,
            signed_headers,
            payload_hash
        );

        // String to sign
        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date, credential_scope, canonical_request_hash
        );

        // Signing key
        let signing_key = self.get_signing_key(&date_stamp, region);

        // Signature
        let signature = self.hmac_sha256(&signing_key, string_to_sign.as_bytes());
        let signature_hex = hex::encode(signature);

        // Build final URL
        format!(
            "{}{}?{}&X-Amz-Signature={}",
            self.endpoint, uri_path, canonical_qs, signature_hex
        )
    }

    fn get_signing_key(&self, date_stamp: &str, region: &str) -> Vec<u8> {
        let k_date = self.hmac_sha256(
            format!("AWS4{}", self.secret_key).as_bytes(),
            date_stamp.as_bytes(),
        );
        let k_region = self.hmac_sha256(&k_date, region.as_bytes());
        let k_service = self.hmac_sha256(&k_region, b"s3");
        self.hmac_sha256(&k_service, b"aws4_request")
    }

    fn hmac_sha256(&self, key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key size is valid");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presign_url_generation() {
        let generator = PresignUrlGenerator::new(
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "http://localhost:9000".to_string(),
            Some("us-east-1".to_string()),
        );

        let url = generator.generate(&PresignOptions {
            method: PresignMethod::Get,
            bucket: "test-bucket".to_string(),
            key: "test/file.txt".to_string(),
            expires_in: 3600,
            content_type: None,
            region: None,
        });

        // Verify URL structure
        assert!(url.starts_with("http://localhost:9000/test-bucket/"));
        assert!(url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(url.contains("X-Amz-Credential="));
        assert!(url.contains("X-Amz-Signature="));
        assert!(url.contains("X-Amz-Expires=3600"));
    }
}
