//! Tenant/workspace MVP utilities.

use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const TENANTS_STORAGE_KEY: &str = "strix_tenants_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub owner: String,
    pub notes: Option<String>,
    pub created_at: String,
}

pub fn load_tenants() -> Vec<Tenant> {
    LocalStorage::get(TENANTS_STORAGE_KEY).unwrap_or_default()
}

pub fn save_tenants(tenants: &[Tenant]) {
    let _ = LocalStorage::set(TENANTS_STORAGE_KEY, tenants.to_vec());
}

pub fn make_slug(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;

    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if (c == ' ' || c == '-' || c == '_' || c == '.') && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }

    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "tenant".to_string()
    } else {
        out
    }
}

pub fn prefixed_bucket_name(tenant_slug: &str, base_name: &str) -> String {
    format!("{}-{}", make_slug(tenant_slug), make_slug(base_name))
}

pub fn tenant_from_bucket_name<'a>(bucket: &str, tenants: &'a [Tenant]) -> Option<&'a Tenant> {
    tenants
        .iter()
        .find(|t| bucket.starts_with(&format!("{}-", t.slug)))
}
