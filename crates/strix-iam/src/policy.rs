//! IAM policy definitions and evaluation.
//!
//! Implements AWS-style IAM policies with:
//! - Explicit Deny takes precedence over Allow
//! - Wildcard matching with `*` (any characters) and `?` (single character)
//! - Condition key evaluation
//! - ARN parsing and matching

use std::collections::HashMap;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};

/// An IAM policy document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// Policy name (unique per user).
    pub name: String,
    /// Policy version (e.g., "2012-10-17").
    #[serde(default = "default_version")]
    pub version: String,
    /// Policy statements.
    #[serde(rename = "Statement")]
    pub statements: Vec<PolicyStatement>,
}

fn default_version() -> String {
    "2012-10-17".to_string()
}

impl Policy {
    /// Create a new policy with the given name and statements.
    pub fn new(name: impl Into<String>, statements: Vec<PolicyStatement>) -> Self {
        Self {
            name: name.into(),
            version: default_version(),
            statements,
        }
    }

    /// Check if this policy allows the given action on the resource.
    pub fn evaluate(&self, action: &Action, resource: &Resource) -> Option<Effect> {
        let mut result = None;

        for stmt in &self.statements {
            if stmt.matches(action, resource) {
                match stmt.effect {
                    Effect::Deny => return Some(Effect::Deny),
                    Effect::Allow => result = Some(Effect::Allow),
                }
            }
        }

        result
    }
}

/// A statement within a policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStatement {
    /// Effect of this statement.
    #[serde(rename = "Effect")]
    pub effect: Effect,
    /// Actions this statement applies to.
    #[serde(rename = "Action")]
    pub actions: Vec<String>,
    /// Resources this statement applies to.
    #[serde(rename = "Resource")]
    pub resources: Vec<String>,
}

impl PolicyStatement {
    /// Check if this statement matches the given action and resource.
    pub fn matches(&self, action: &Action, resource: &Resource) -> bool {
        self.matches_action(action) && self.matches_resource(resource)
    }

    fn matches_action(&self, action: &Action) -> bool {
        let action_str = action.to_string();
        for pattern in &self.actions {
            if pattern == "*" || pattern == "s3:*" {
                return true;
            }
            if matches_wildcard(pattern, &action_str) {
                return true;
            }
        }
        false
    }

    fn matches_resource(&self, resource: &Resource) -> bool {
        let resource_str = resource.to_string();
        for pattern in &self.resources {
            if pattern == "*" {
                return true;
            }
            if matches_wildcard(pattern, &resource_str) {
                return true;
            }
        }
        false
    }
}

/// Effect of a policy statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Allow,
    Deny,
}

/// S3 API actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Bucket operations
    CreateBucket,
    DeleteBucket,
    ListBucket,
    HeadBucket,
    GetBucketLocation,
    GetBucketVersioning,
    PutBucketVersioning,
    GetBucketTagging,
    PutBucketTagging,
    DeleteBucketTagging,

    // Object operations
    GetObject,
    PutObject,
    DeleteObject,
    HeadObject,
    CopyObject,
    ListMultipartUploadParts,
    AbortMultipartUpload,

    // List operations
    ListAllMyBuckets,
    ListBucketMultipartUploads,

    // Any action (for admin)
    All,
}

impl Action {
    /// Parse an action from an S3 API operation name.
    pub fn from_operation(op: &str) -> Option<Self> {
        Some(match op {
            "CreateBucket" => Action::CreateBucket,
            "DeleteBucket" => Action::DeleteBucket,
            "ListBucket" | "ListObjects" | "ListObjectsV2" => Action::ListBucket,
            "HeadBucket" => Action::HeadBucket,
            "GetBucketLocation" => Action::GetBucketLocation,
            "GetBucketVersioning" => Action::GetBucketVersioning,
            "PutBucketVersioning" => Action::PutBucketVersioning,
            "GetBucketTagging" => Action::GetBucketTagging,
            "PutBucketTagging" => Action::PutBucketTagging,
            "DeleteBucketTagging" => Action::DeleteBucketTagging,
            "GetObject" => Action::GetObject,
            "PutObject" => Action::PutObject,
            "DeleteObject" | "DeleteObjects" => Action::DeleteObject,
            "HeadObject" => Action::HeadObject,
            "CopyObject" => Action::CopyObject,
            "ListParts" => Action::ListMultipartUploadParts,
            "AbortMultipartUpload" => Action::AbortMultipartUpload,
            "ListBuckets" => Action::ListAllMyBuckets,
            "ListMultipartUploads" => Action::ListBucketMultipartUploads,
            "*" => Action::All,
            _ => return None,
        })
    }
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Action::CreateBucket => "s3:CreateBucket",
            Action::DeleteBucket => "s3:DeleteBucket",
            Action::ListBucket => "s3:ListBucket",
            Action::HeadBucket => "s3:HeadBucket",
            Action::GetBucketLocation => "s3:GetBucketLocation",
            Action::GetBucketVersioning => "s3:GetBucketVersioning",
            Action::PutBucketVersioning => "s3:PutBucketVersioning",
            Action::GetBucketTagging => "s3:GetBucketTagging",
            Action::PutBucketTagging => "s3:PutBucketTagging",
            Action::DeleteBucketTagging => "s3:DeleteBucketTagging",
            Action::GetObject => "s3:GetObject",
            Action::PutObject => "s3:PutObject",
            Action::DeleteObject => "s3:DeleteObject",
            Action::HeadObject => "s3:HeadObject",
            Action::CopyObject => "s3:CopyObject",
            Action::ListMultipartUploadParts => "s3:ListMultipartUploadParts",
            Action::AbortMultipartUpload => "s3:AbortMultipartUpload",
            Action::ListAllMyBuckets => "s3:ListAllMyBuckets",
            Action::ListBucketMultipartUploads => "s3:ListBucketMultipartUploads",
            Action::All => "s3:*",
        };
        write!(f, "{}", s)
    }
}

/// An S3 resource (bucket or object).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub bucket: Option<String>,
    pub key: Option<String>,
}

impl Resource {
    /// Resource for all buckets.
    pub fn all() -> Self {
        Self {
            bucket: None,
            key: None,
        }
    }

    /// Resource for a specific bucket.
    pub fn bucket(name: impl Into<String>) -> Self {
        Self {
            bucket: Some(name.into()),
            key: None,
        }
    }

    /// Resource for objects in a bucket.
    pub fn object(bucket: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            bucket: Some(bucket.into()),
            key: Some(key.into()),
        }
    }
}

impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.bucket, &self.key) {
            (None, _) => write!(f, "arn:aws:s3:::*"),
            (Some(b), None) => write!(f, "arn:aws:s3:::{}", b),
            (Some(b), Some(k)) => write!(f, "arn:aws:s3:::{}/{}", b, k),
        }
    }
}

/// Wildcard matching supporting `*` (any characters) and `?` (single character).
///
/// This is compatible with AWS IAM policy patterns.
fn matches_wildcard(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    // Use dynamic programming for full wildcard matching
    let p: Vec<char> = pattern.chars().collect();
    let v: Vec<char> = value.chars().collect();

    let m = p.len();
    let n = v.len();

    // dp[i][j] = true if pattern[0..i] matches value[0..j]
    let mut dp = vec![vec![false; n + 1]; m + 1];
    dp[0][0] = true;

    // Handle patterns starting with *
    for i in 1..=m {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=m {
        for j in 1..=n {
            if p[i - 1] == '*' {
                // * can match zero or more characters
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if p[i - 1] == '?' || p[i - 1] == v[j - 1] {
                // ? matches exactly one character, or exact match
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[m][n]
}

/// Parse an ARN into its components.
///
/// ARN format: arn:partition:service:region:account:resource
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedArn {
    pub partition: String,
    pub service: String,
    pub region: String,
    pub account: String,
    pub resource: String,
}

impl ParsedArn {
    /// Parse an ARN string.
    pub fn parse(arn: &str) -> Option<Self> {
        if !arn.starts_with("arn:") {
            return None;
        }

        let parts: Vec<&str> = arn.splitn(6, ':').collect();
        if parts.len() < 6 {
            return None;
        }

        Some(Self {
            partition: parts[1].to_string(),
            service: parts[2].to_string(),
            region: parts[3].to_string(),
            account: parts[4].to_string(),
            resource: parts[5].to_string(),
        })
    }

    /// Check if this ARN matches a pattern ARN (with wildcards).
    pub fn matches(&self, pattern: &ParsedArn) -> bool {
        matches_wildcard(&pattern.partition, &self.partition)
            && matches_wildcard(&pattern.service, &self.service)
            && matches_wildcard(&pattern.region, &self.region)
            && matches_wildcard(&pattern.account, &self.account)
            && matches_wildcard(&pattern.resource, &self.resource)
    }
}

/// Pre-defined admin policy (full access).
pub fn admin_policy() -> Policy {
    Policy::new(
        "AdministratorAccess",
        vec![PolicyStatement {
            effect: Effect::Allow,
            actions: vec!["*".to_string()],
            resources: vec!["*".to_string()],
        }],
    )
}

/// Pre-defined read-only policy.
#[allow(dead_code)]
pub fn read_only_policy() -> Policy {
    Policy::new(
        "ReadOnlyAccess",
        vec![PolicyStatement {
            effect: Effect::Allow,
            actions: vec![
                "s3:GetObject".to_string(),
                "s3:HeadObject".to_string(),
                "s3:ListBucket".to_string(),
                "s3:ListAllMyBuckets".to_string(),
                "s3:HeadBucket".to_string(),
                "s3:GetBucketLocation".to_string(),
            ],
            resources: vec!["*".to_string()],
        }],
    )
}

/// Pre-defined read-write policy (no admin operations).
#[allow(dead_code)]
pub fn read_write_policy() -> Policy {
    Policy::new(
        "ReadWriteAccess",
        vec![PolicyStatement {
            effect: Effect::Allow,
            actions: vec![
                "s3:GetObject".to_string(),
                "s3:PutObject".to_string(),
                "s3:DeleteObject".to_string(),
                "s3:HeadObject".to_string(),
                "s3:ListBucket".to_string(),
                "s3:ListAllMyBuckets".to_string(),
                "s3:HeadBucket".to_string(),
                "s3:GetBucketLocation".to_string(),
                "s3:ListMultipartUploadParts".to_string(),
                "s3:AbortMultipartUpload".to_string(),
                "s3:ListBucketMultipartUploads".to_string(),
            ],
            resources: vec!["*".to_string()],
        }],
    )
}

// === Condition Evaluation ===

/// Context for evaluating policy conditions.
///
/// Contains information about the request that can be used in condition evaluation.
#[derive(Debug, Clone, Default)]
pub struct ConditionContext {
    /// Source IP address of the request.
    pub source_ip: Option<IpAddr>,
    /// Current date/time (ISO 8601).
    pub current_time: Option<String>,
    /// Whether the request uses secure transport (HTTPS).
    pub secure_transport: bool,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// S3 prefix (for ListBucket).
    pub prefix: Option<String>,
    /// S3 delimiter (for ListBucket).
    pub delimiter: Option<String>,
    /// S3 max keys (for ListBucket).
    pub max_keys: Option<u32>,
    /// Custom context values.
    pub custom: HashMap<String, String>,
}

impl ConditionContext {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the source IP.
    pub fn with_source_ip(mut self, ip: IpAddr) -> Self {
        self.source_ip = Some(ip);
        self
    }

    /// Set secure transport flag.
    pub fn with_secure_transport(mut self, secure: bool) -> Self {
        self.secure_transport = secure;
        self
    }

    /// Get a context value by key.
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "aws:SourceIp" => self.source_ip.map(|ip| ip.to_string()),
            "aws:CurrentTime" => self.current_time.clone(),
            "aws:SecureTransport" => Some(self.secure_transport.to_string()),
            "s3:prefix" => self.prefix.clone(),
            "s3:delimiter" => self.delimiter.clone(),
            "s3:max-keys" => self.max_keys.map(|k| k.to_string()),
            key if key.starts_with("aws:") || key.starts_with("s3:") => {
                self.custom.get(key).cloned()
            }
            _ => None,
        }
    }
}

/// Evaluate a condition block against a context.
///
/// Condition format:
/// ```json
/// {
///   "StringEquals": { "s3:prefix": "home/" },
///   "IpAddress": { "aws:SourceIp": "192.168.1.0/24" }
/// }
/// ```
pub fn evaluate_condition(condition: &serde_json::Value, context: &ConditionContext) -> bool {
    let obj = match condition.as_object() {
        Some(obj) => obj,
        None => return true, // No conditions means match
    };

    // All condition operators must match (AND)
    for (operator, conditions) in obj {
        let conditions_obj = match conditions.as_object() {
            Some(obj) => obj,
            None => continue,
        };

        // All key-value pairs within an operator must match (AND)
        for (key, expected) in conditions_obj {
            let actual = context.get(key);

            let matches = match operator.as_str() {
                "StringEquals" => actual.as_deref() == expected.as_str(),
                "StringNotEquals" => actual.as_deref() != expected.as_str(),
                "StringLike" => {
                    if let (Some(actual), Some(pattern)) = (actual.as_deref(), expected.as_str()) {
                        matches_wildcard(pattern, actual)
                    } else {
                        false
                    }
                }
                "StringNotLike" => {
                    if let (Some(actual), Some(pattern)) = (actual.as_deref(), expected.as_str()) {
                        !matches_wildcard(pattern, actual)
                    } else {
                        true
                    }
                }
                "IpAddress" => {
                    if let Some(source_ip) = context.source_ip {
                        if let Some(cidr) = expected.as_str() {
                            ip_matches_cidr(source_ip, cidr)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                "NotIpAddress" => {
                    if let Some(source_ip) = context.source_ip {
                        if let Some(cidr) = expected.as_str() {
                            !ip_matches_cidr(source_ip, cidr)
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                }
                "Bool" => {
                    if let Some(expected_bool) = expected.as_str() {
                        actual.as_deref() == Some(expected_bool)
                    } else if let Some(expected_bool) = expected.as_bool() {
                        actual.as_deref() == Some(&expected_bool.to_string())
                    } else {
                        false
                    }
                }
                "NumericEquals" => {
                    if let (Some(actual), Some(expected)) = (
                        actual.and_then(|s| s.parse::<f64>().ok()),
                        expected
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .or_else(|| expected.as_f64()),
                    ) {
                        (actual - expected).abs() < f64::EPSILON
                    } else {
                        false
                    }
                }
                "NumericLessThan" => {
                    if let (Some(actual), Some(expected)) = (
                        actual.and_then(|s| s.parse::<f64>().ok()),
                        expected
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .or_else(|| expected.as_f64()),
                    ) {
                        actual < expected
                    } else {
                        false
                    }
                }
                "NumericGreaterThan" => {
                    if let (Some(actual), Some(expected)) = (
                        actual.and_then(|s| s.parse::<f64>().ok()),
                        expected
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .or_else(|| expected.as_f64()),
                    ) {
                        actual > expected
                    } else {
                        false
                    }
                }
                "Null" => {
                    let is_null = actual.is_none();
                    let expected_null =
                        expected.as_str() == Some("true") || expected.as_bool() == Some(true);
                    is_null == expected_null
                }
                _ => {
                    // Unknown operator - fail closed (deny)
                    false
                }
            };

            if !matches {
                return false;
            }
        }
    }

    true
}

/// Check if an IP address matches a CIDR range.
fn ip_matches_cidr(ip: IpAddr, cidr: &str) -> bool {
    // Simple CIDR matching
    if let Some((network, prefix_len)) = cidr.split_once('/') {
        let prefix_len: u8 = match prefix_len.parse() {
            Ok(len) => len,
            Err(_) => return false,
        };

        match (ip, network.parse::<IpAddr>()) {
            (IpAddr::V4(ip), Ok(IpAddr::V4(network))) => {
                if prefix_len > 32 {
                    return false;
                }
                let mask = if prefix_len == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix_len)
                };
                let ip_bits: u32 = ip.into();
                let network_bits: u32 = network.into();
                (ip_bits & mask) == (network_bits & mask)
            }
            (IpAddr::V6(ip), Ok(IpAddr::V6(network))) => {
                if prefix_len > 128 {
                    return false;
                }
                let ip_bits: u128 = ip.into();
                let network_bits: u128 = network.into();
                let mask = if prefix_len == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix_len)
                };
                (ip_bits & mask) == (network_bits & mask)
            }
            _ => false,
        }
    } else {
        // Exact IP match
        ip.to_string() == cidr
    }
}

// === Bucket Policies ===

/// A bucket policy document (S3-style).
///
/// Unlike IAM policies, bucket policies:
/// - Are attached to a bucket (not a user)
/// - Have a Principal field (who the policy applies to)
/// - Can allow anonymous access when Principal is "*"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketPolicy {
    /// Policy version (e.g., "2012-10-17").
    #[serde(rename = "Version", default = "default_version")]
    pub version: String,
    /// Unique policy ID.
    #[serde(rename = "Id", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Policy statements.
    #[serde(rename = "Statement")]
    pub statements: Vec<BucketPolicyStatement>,
}

impl BucketPolicy {
    /// Create a new bucket policy with the given statements.
    pub fn new(statements: Vec<BucketPolicyStatement>) -> Self {
        Self {
            version: default_version(),
            id: None,
            statements,
        }
    }

    /// Create a new bucket policy with an ID.
    pub fn with_id(id: impl Into<String>, statements: Vec<BucketPolicyStatement>) -> Self {
        Self {
            version: default_version(),
            id: Some(id.into()),
            statements,
        }
    }

    /// Check if this policy allows the given principal to perform the action on the resource.
    pub fn evaluate(
        &self,
        principal: &Principal,
        action: &Action,
        resource: &Resource,
    ) -> Option<Effect> {
        self.evaluate_with_context(principal, action, resource, &ConditionContext::default())
    }

    /// Check if this policy allows the action with condition context.
    pub fn evaluate_with_context(
        &self,
        principal: &Principal,
        action: &Action,
        resource: &Resource,
        context: &ConditionContext,
    ) -> Option<Effect> {
        let mut result = None;

        for stmt in &self.statements {
            if stmt.matches_with_context(principal, action, resource, context) {
                match stmt.effect {
                    Effect::Deny => return Some(Effect::Deny),
                    Effect::Allow => result = Some(Effect::Allow),
                }
            }
        }

        result
    }
}

/// A statement within a bucket policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketPolicyStatement {
    /// Optional statement ID.
    #[serde(rename = "Sid", skip_serializing_if = "Option::is_none")]
    pub sid: Option<String>,
    /// Effect of this statement.
    #[serde(rename = "Effect")]
    pub effect: Effect,
    /// Principal(s) this statement applies to.
    #[serde(rename = "Principal")]
    pub principal: PrincipalSpec,
    /// Actions this statement applies to.
    #[serde(rename = "Action")]
    pub actions: ActionSpec,
    /// Resources this statement applies to.
    #[serde(rename = "Resource")]
    pub resources: ResourceSpec,
    /// Optional conditions.
    #[serde(rename = "Condition", skip_serializing_if = "Option::is_none")]
    pub condition: Option<serde_json::Value>,
}

impl BucketPolicyStatement {
    /// Check if this statement matches the given principal, action, and resource.
    pub fn matches(&self, principal: &Principal, action: &Action, resource: &Resource) -> bool {
        self.matches_with_context(principal, action, resource, &ConditionContext::default())
    }

    /// Check if this statement matches with condition context.
    pub fn matches_with_context(
        &self,
        principal: &Principal,
        action: &Action,
        resource: &Resource,
        context: &ConditionContext,
    ) -> bool {
        self.matches_principal(principal)
            && self.matches_action(action)
            && self.matches_resource(resource)
            && self.matches_condition(context)
    }

    fn matches_condition(&self, context: &ConditionContext) -> bool {
        match &self.condition {
            Some(condition) => evaluate_condition(condition, context),
            None => true,
        }
    }

    fn matches_principal(&self, principal: &Principal) -> bool {
        match &self.principal {
            PrincipalSpec::Wildcard => true,
            PrincipalSpec::Aws(principals) => {
                for p in principals {
                    if p == "*" {
                        return true;
                    }
                    match principal {
                        Principal::Anonymous => {
                            if p == "*" {
                                return true;
                            }
                        }
                        Principal::User(user) => {
                            if p == "*" || p == user || p.ends_with(&format!("user/{}", user)) {
                                return true;
                            }
                        }
                        Principal::Arn(arn) => {
                            if p == "*" || p == arn || matches_wildcard(p, arn) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
        }
    }

    fn matches_action(&self, action: &Action) -> bool {
        let action_str = action.to_string();
        let actions = match &self.actions {
            ActionSpec::Single(a) => vec![a.clone()],
            ActionSpec::Multiple(a) => a.clone(),
        };
        for pattern in &actions {
            if pattern == "*" || pattern == "s3:*" {
                return true;
            }
            if matches_wildcard(pattern, &action_str) {
                return true;
            }
        }
        false
    }

    fn matches_resource(&self, resource: &Resource) -> bool {
        let resource_str = resource.to_string();
        let resources = match &self.resources {
            ResourceSpec::Single(r) => vec![r.clone()],
            ResourceSpec::Multiple(r) => r.clone(),
        };
        for pattern in &resources {
            if pattern == "*" {
                return true;
            }
            if matches_wildcard(pattern, &resource_str) {
                return true;
            }
        }
        false
    }
}

/// Principal specification in a bucket policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PrincipalSpec {
    /// Wildcard principal ("*" means everyone).
    Wildcard,
    /// AWS principals.
    Aws(Vec<String>),
}

/// A principal (who is making the request).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// Anonymous (unauthenticated) request.
    Anonymous,
    /// Authenticated user.
    User(String),
    /// Principal by ARN.
    Arn(String),
}

impl Principal {
    /// Create a principal for a user.
    pub fn user(username: impl Into<String>) -> Self {
        Principal::User(username.into())
    }

    /// Create an anonymous principal.
    pub fn anonymous() -> Self {
        Principal::Anonymous
    }

    /// Create a principal from an ARN.
    pub fn arn(arn: impl Into<String>) -> Self {
        Principal::Arn(arn.into())
    }
}

/// Action specification (single or multiple).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionSpec {
    Single(String),
    Multiple(Vec<String>),
}

/// Resource specification (single or multiple).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceSpec {
    Single(String),
    Multiple(Vec<String>),
}

/// Create a public read policy for a bucket.
#[allow(dead_code)]
pub fn public_read_policy(bucket: &str) -> BucketPolicy {
    BucketPolicy::with_id(
        "PublicRead",
        vec![BucketPolicyStatement {
            sid: Some("AllowPublicRead".to_string()),
            effect: Effect::Allow,
            principal: PrincipalSpec::Wildcard,
            actions: ActionSpec::Multiple(vec![
                "s3:GetObject".to_string(),
                "s3:HeadObject".to_string(),
            ]),
            resources: ResourceSpec::Single(format!("arn:aws:s3:::{}/*", bucket)),
            condition: None,
        }],
    )
}

/// Create a policy that denies all public access.
#[allow(dead_code)]
pub fn block_public_access_policy(bucket: &str) -> BucketPolicy {
    BucketPolicy::with_id(
        "BlockPublicAccess",
        vec![BucketPolicyStatement {
            sid: Some("DenyPublicAccess".to_string()),
            effect: Effect::Deny,
            principal: PrincipalSpec::Wildcard,
            actions: ActionSpec::Single("s3:*".to_string()),
            resources: ResourceSpec::Multiple(vec![
                format!("arn:aws:s3:::{}", bucket),
                format!("arn:aws:s3:::{}/*", bucket),
            ]),
            condition: None,
        }],
    )
}

// === Policy Validation ===

/// Validation error for policies.
#[derive(Debug, Clone)]
pub struct PolicyValidationError {
    pub message: String,
    pub location: Option<String>,
}

impl std::fmt::Display for PolicyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.location {
            Some(loc) => write!(f, "{}: {}", loc, self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for PolicyValidationError {}

/// Validate an IAM policy.
pub fn validate_policy(policy: &Policy) -> Result<(), Vec<PolicyValidationError>> {
    let mut errors = Vec::new();

    // Check policy name
    if policy.name.is_empty() {
        errors.push(PolicyValidationError {
            message: "Policy name cannot be empty".to_string(),
            location: Some("name".to_string()),
        });
    }

    // Check statements
    if policy.statements.is_empty() {
        errors.push(PolicyValidationError {
            message: "Policy must have at least one statement".to_string(),
            location: Some("Statement".to_string()),
        });
    }

    for (i, stmt) in policy.statements.iter().enumerate() {
        let loc = format!("Statement[{}]", i);

        // Check actions
        if stmt.actions.is_empty() {
            errors.push(PolicyValidationError {
                message: "Statement must have at least one action".to_string(),
                location: Some(format!("{}.Action", loc)),
            });
        }

        for (j, action) in stmt.actions.iter().enumerate() {
            if !is_valid_action(action) {
                errors.push(PolicyValidationError {
                    message: format!("Invalid action: {}", action),
                    location: Some(format!("{}.Action[{}]", loc, j)),
                });
            }
        }

        // Check resources
        if stmt.resources.is_empty() {
            errors.push(PolicyValidationError {
                message: "Statement must have at least one resource".to_string(),
                location: Some(format!("{}.Resource", loc)),
            });
        }

        for (j, resource) in stmt.resources.iter().enumerate() {
            if !is_valid_resource(resource) {
                errors.push(PolicyValidationError {
                    message: format!("Invalid resource ARN: {}", resource),
                    location: Some(format!("{}.Resource[{}]", loc, j)),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validate a bucket policy.
pub fn validate_bucket_policy(
    policy: &BucketPolicy,
    bucket: &str,
) -> Result<(), Vec<PolicyValidationError>> {
    let mut errors = Vec::new();

    // Check statements
    if policy.statements.is_empty() {
        errors.push(PolicyValidationError {
            message: "Bucket policy must have at least one statement".to_string(),
            location: Some("Statement".to_string()),
        });
    }

    for (i, stmt) in policy.statements.iter().enumerate() {
        let loc = format!("Statement[{}]", i);

        // Check actions
        let actions = match &stmt.actions {
            ActionSpec::Single(a) => vec![a.clone()],
            ActionSpec::Multiple(a) => a.clone(),
        };

        if actions.is_empty() {
            errors.push(PolicyValidationError {
                message: "Statement must have at least one action".to_string(),
                location: Some(format!("{}.Action", loc)),
            });
        }

        for (j, action) in actions.iter().enumerate() {
            if !is_valid_action(action) {
                errors.push(PolicyValidationError {
                    message: format!("Invalid action: {}", action),
                    location: Some(format!("{}.Action[{}]", loc, j)),
                });
            }
        }

        // Check resources reference the correct bucket
        let resources = match &stmt.resources {
            ResourceSpec::Single(r) => vec![r.clone()],
            ResourceSpec::Multiple(r) => r.clone(),
        };

        if resources.is_empty() {
            errors.push(PolicyValidationError {
                message: "Statement must have at least one resource".to_string(),
                location: Some(format!("{}.Resource", loc)),
            });
        }

        for (j, resource) in resources.iter().enumerate() {
            if !is_valid_resource(resource) {
                errors.push(PolicyValidationError {
                    message: format!("Invalid resource ARN: {}", resource),
                    location: Some(format!("{}.Resource[{}]", loc, j)),
                });
            }

            // Check resource matches the bucket
            if resource != "*" && !resource.contains(&format!(":::{}", bucket)) {
                errors.push(PolicyValidationError {
                    message: format!("Resource {} does not reference bucket {}", resource, bucket),
                    location: Some(format!("{}.Resource[{}]", loc, j)),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check if an action string is valid.
fn is_valid_action(action: &str) -> bool {
    if action == "*" {
        return true;
    }

    // Must start with s3: or be a wildcard
    if !action.starts_with("s3:") {
        return false;
    }

    // Check for valid S3 actions or wildcards
    let known_actions = [
        "s3:*",
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:ListBucket",
        "s3:CreateBucket",
        "s3:DeleteBucket",
        "s3:HeadBucket",
        "s3:HeadObject",
        "s3:CopyObject",
        "s3:GetBucketLocation",
        "s3:GetBucketVersioning",
        "s3:PutBucketVersioning",
        "s3:GetBucketTagging",
        "s3:PutBucketTagging",
        "s3:DeleteBucketTagging",
        "s3:ListAllMyBuckets",
        "s3:ListBucketMultipartUploads",
        "s3:ListMultipartUploadParts",
        "s3:AbortMultipartUpload",
        "s3:GetBucketPolicy",
        "s3:PutBucketPolicy",
        "s3:DeleteBucketPolicy",
        "s3:GetBucketCors",
        "s3:PutBucketCors",
        "s3:DeleteBucketCors",
        "s3:GetBucketNotification",
        "s3:PutBucketNotification",
        "s3:GetObjectVersion",
        "s3:DeleteObjectVersion",
    ];

    // Check exact match or wildcard pattern
    if known_actions.contains(&action) {
        return true;
    }

    // Allow wildcard patterns like s3:Get*, s3:Put*, etc.
    if action.contains('*') {
        return true;
    }

    // Unknown but syntactically valid action
    action.len() > 3 && action[3..].chars().all(|c| c.is_alphanumeric())
}

/// Check if a resource ARN is valid.
fn is_valid_resource(resource: &str) -> bool {
    if resource == "*" {
        return true;
    }

    // Must start with arn:
    if !resource.starts_with("arn:") {
        return false;
    }

    // Parse ARN structure
    let parts: Vec<&str> = resource.splitn(6, ':').collect();
    if parts.len() < 6 {
        return false;
    }

    // Check service is s3
    if parts[2] != "s3" && parts[2] != "aws" && parts[2] != "*" {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_matching() {
        assert!(matches_wildcard("*", "anything"));
        assert!(matches_wildcard("s3:*", "s3:GetObject"));
        assert!(matches_wildcard("s3:Get*", "s3:GetObject"));
        assert!(!matches_wildcard("s3:Put*", "s3:GetObject"));
    }

    #[test]
    fn test_policy_evaluation() {
        let policy = admin_policy();
        let action = Action::GetObject;
        let resource = Resource::bucket("test");

        assert_eq!(policy.evaluate(&action, &resource), Some(Effect::Allow));
    }

    #[test]
    fn test_deny_overrides_allow() {
        let policy = Policy::new(
            "DenyTest",
            vec![
                PolicyStatement {
                    effect: Effect::Allow,
                    actions: vec!["s3:*".to_string()],
                    resources: vec!["*".to_string()],
                },
                PolicyStatement {
                    effect: Effect::Deny,
                    actions: vec!["s3:DeleteObject".to_string()],
                    resources: vec!["arn:aws:s3:::protected/*".to_string()],
                },
            ],
        );

        // Should allow normal operations
        assert_eq!(
            policy.evaluate(&Action::GetObject, &Resource::bucket("test")),
            Some(Effect::Allow)
        );

        // Should deny delete on protected bucket
        assert_eq!(
            policy.evaluate(
                &Action::DeleteObject,
                &Resource::object("protected", "file.txt")
            ),
            Some(Effect::Deny)
        );
    }

    #[test]
    fn test_bucket_policy_public_read() {
        let policy = public_read_policy("public-bucket");
        let principal = Principal::anonymous();
        let resource = Resource::object("public-bucket", "file.txt");

        // Should allow GetObject
        assert_eq!(
            policy.evaluate(&principal, &Action::GetObject, &resource),
            Some(Effect::Allow)
        );

        // Should not allow PutObject
        assert_eq!(
            policy.evaluate(&principal, &Action::PutObject, &resource),
            None
        );
    }

    #[test]
    fn test_bucket_policy_user_principal() {
        let policy = BucketPolicy::new(vec![BucketPolicyStatement {
            sid: Some("AllowUserAccess".to_string()),
            effect: Effect::Allow,
            principal: PrincipalSpec::Aws(vec!["alice".to_string()]),
            actions: ActionSpec::Single("s3:*".to_string()),
            resources: ResourceSpec::Single("arn:aws:s3:::my-bucket/*".to_string()),
            condition: None,
        }]);

        let alice = Principal::user("alice");
        let bob = Principal::user("bob");
        let resource = Resource::object("my-bucket", "file.txt");

        // Alice should be allowed
        assert_eq!(
            policy.evaluate(&alice, &Action::GetObject, &resource),
            Some(Effect::Allow)
        );

        // Bob should not be allowed
        assert_eq!(policy.evaluate(&bob, &Action::GetObject, &resource), None);
    }

    #[test]
    fn test_bucket_policy_serialization() {
        let policy = public_read_policy("test-bucket");
        let json = serde_json::to_string_pretty(&policy).unwrap();

        // Should be valid JSON
        assert!(json.contains("Statement"));
        assert!(json.contains("Effect"));
        assert!(json.contains("Allow"));

        // Should deserialize back
        let parsed: BucketPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.statements.len(), 1);
    }

    #[test]
    fn test_advanced_wildcard_matching() {
        // Test ? for single character
        assert!(matches_wildcard("s3:Get?bject", "s3:GetObject"));
        assert!(!matches_wildcard("s3:Get?bject", "s3:GetXXbject"));

        // Test * in the middle
        assert!(matches_wildcard("s3:*Object", "s3:GetObject"));
        assert!(matches_wildcard("s3:*Object", "s3:PutObject"));
        assert!(!matches_wildcard("s3:*Object", "s3:GetBucket"));

        // Test multiple wildcards
        assert!(matches_wildcard("s3:*Bucket*", "s3:CreateBucket"));
        assert!(matches_wildcard("s3:*Bucket*", "s3:GetBucketVersioning"));
    }

    #[test]
    fn test_ip_cidr_matching() {
        use std::net::Ipv4Addr;

        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        // Should match CIDR range
        assert!(ip_matches_cidr(ip, "192.168.1.0/24"));
        assert!(ip_matches_cidr(ip, "192.168.0.0/16"));
        assert!(ip_matches_cidr(ip, "0.0.0.0/0"));

        // Should not match different networks
        assert!(!ip_matches_cidr(ip, "10.0.0.0/8"));
        assert!(!ip_matches_cidr(ip, "192.168.2.0/24"));

        // Should match exact IP
        assert!(ip_matches_cidr(ip, "192.168.1.100"));
        assert!(!ip_matches_cidr(ip, "192.168.1.101"));
    }

    #[test]
    fn test_condition_evaluation() {
        use std::net::Ipv4Addr;

        let context = ConditionContext::new()
            .with_source_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)))
            .with_secure_transport(true);

        // Test IpAddress condition
        let condition = serde_json::json!({
            "IpAddress": { "aws:SourceIp": "192.168.1.0/24" }
        });
        assert!(evaluate_condition(&condition, &context));

        // Test NotIpAddress condition
        let condition = serde_json::json!({
            "NotIpAddress": { "aws:SourceIp": "10.0.0.0/8" }
        });
        assert!(evaluate_condition(&condition, &context));

        // Test Bool condition
        let condition = serde_json::json!({
            "Bool": { "aws:SecureTransport": "true" }
        });
        assert!(evaluate_condition(&condition, &context));

        // Test failing condition
        let condition = serde_json::json!({
            "IpAddress": { "aws:SourceIp": "10.0.0.0/8" }
        });
        assert!(!evaluate_condition(&condition, &context));
    }

    #[test]
    fn test_policy_validation() {
        // Valid policy
        let policy = admin_policy();
        assert!(validate_policy(&policy).is_ok());

        // Invalid policy - empty name
        let invalid = Policy {
            name: "".to_string(),
            version: "2012-10-17".to_string(),
            statements: vec![],
        };
        let errors = validate_policy(&invalid).unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_arn_parsing() {
        let arn = ParsedArn::parse("arn:aws:s3:::my-bucket/my-key").unwrap();
        assert_eq!(arn.partition, "aws");
        assert_eq!(arn.service, "s3");
        assert_eq!(arn.region, "");
        assert_eq!(arn.account, "");
        assert_eq!(arn.resource, "my-bucket/my-key");

        // Invalid ARN
        assert!(ParsedArn::parse("not-an-arn").is_none());
    }
}
