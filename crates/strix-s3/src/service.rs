//! S3 service implementation.

use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use futures::stream::StreamExt;
use http::HeaderMap;
use metrics::counter;
use s3s::auth::Credentials;
use s3s::dto::*;
use s3s::s3_error;
use s3s::{S3, S3Request, S3Response, S3Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tracing::{debug, instrument};

use strix_core::{
    AuditLogEntry, CompletePart, CopyObjectOpts, CreateBucketOpts, GetObjectOpts, ListObjectsOpts,
    ListPartsOpts, ListUploadsOpts, ListVersionsOpts, MetadataDirective, ObjectStore,
    PutObjectOpts, Tag as CoreTag, TaggingConfiguration,
};
use strix_iam::{Action, Effect, IamProvider, Principal, Resource};

use crate::auth::AuthProvider;
use crate::error::to_s3_error;
use crate::stream::S3BodyStream;

#[derive(Clone, Debug)]
pub struct RequestAuditContext {
    pub source_ip: Option<String>,
    pub request_id: String,
}

struct AuditEvent<'a> {
    operation: &'a str,
    bucket: Option<&'a str>,
    key: Option<&'a str>,
    principal: Option<&'a str>,
    success: bool,
    error_message: Option<&'a str>,
}

const OBJECT_TAGS_METADATA_KEY: &str = "strix-object-tags";

/// S3 service implementation for Strix.
pub struct StrixS3Service {
    store: Arc<dyn ObjectStore>,
    #[allow(dead_code)]
    auth: Arc<dyn AuthProvider>,
    iam: Option<Arc<dyn IamProvider>>,
    root_access_key: String,
}

impl StrixS3Service {
    /// Create a new S3 service.
    pub fn new(store: Arc<dyn ObjectStore>, auth: Arc<dyn AuthProvider>) -> Self {
        Self {
            store,
            auth,
            iam: None,
            root_access_key: String::new(),
        }
    }

    /// Create a new S3 service with IAM provider for policy enforcement.
    pub fn with_iam(
        store: Arc<dyn ObjectStore>,
        auth: Arc<dyn AuthProvider>,
        iam: Arc<dyn IamProvider>,
        root_access_key: String,
    ) -> Self {
        Self {
            store,
            auth,
            iam: Some(iam),
            root_access_key,
        }
    }

    /// Get the username for the given credentials.
    /// Returns "root" for the root user, or looks up the username from the access key.
    /// For temporary (ASIA) credentials, returns the assumed identity username.
    async fn get_username(&self, credentials: &Option<Credentials>) -> S3Result<String> {
        match credentials {
            None => {
                // Anonymous request
                Ok("anonymous".to_string())
            }
            Some(creds) => {
                let access_key = &creds.access_key;

                // Check if this is the root user
                if access_key == &self.root_access_key {
                    return Ok("root".to_string());
                }

                let Some(iam) = &self.iam else {
                    // No IAM provider - only root user is allowed
                    return Err(s3_error!(AccessDenied, "Access denied"));
                };

                // Check if this is a temporary credential (ASIA prefix)
                if access_key.starts_with("ASIA") {
                    // Look up username from temporary credentials table
                    match iam.get_temp_credentials(access_key).await {
                        Ok(Some((cred, _user))) => {
                            // Return the assumed identity (the user who assumed the role)
                            Ok(cred.assumed_identity)
                        }
                        Ok(None) => {
                            // Temp credential not found or expired
                            Err(s3_error!(AccessDenied, "Invalid or expired credentials"))
                        }
                        Err(e) => {
                            tracing::error!("Temp credential lookup failed: {}", e);
                            Err(s3_error!(InternalError, "Authentication failed"))
                        }
                    }
                } else {
                    // Look up username from permanent credentials table
                    match iam.get_credentials(access_key).await {
                        Ok(Some((_, user))) => Ok(user.username),
                        Ok(None) => {
                            // Should not happen if auth succeeded, but handle it
                            Err(s3_error!(AccessDenied, "Invalid credentials"))
                        }
                        Err(e) => {
                            tracing::error!("IAM lookup failed: {}", e);
                            Err(s3_error!(InternalError, "Authentication failed"))
                        }
                    }
                }
            }
        }
    }

    /// Validate session token for temporary (STS) credentials.
    /// Returns Ok(()) if valid or if not using temporary credentials.
    async fn validate_session_token(
        &self,
        credentials: &Option<Credentials>,
        headers: &HeaderMap,
    ) -> S3Result<()> {
        let Some(creds) = credentials else {
            return Ok(()); // Anonymous request
        };

        // Only validate for temporary credentials (ASIA prefix)
        if !creds.access_key.starts_with("ASIA") {
            return Ok(());
        }

        let Some(iam) = &self.iam else {
            return Err(s3_error!(AccessDenied, "IAM not configured"));
        };

        // Extract X-Amz-Security-Token header
        let session_token = headers
            .get("x-amz-security-token")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                s3_error!(
                    AccessDenied,
                    "Missing X-Amz-Security-Token for temporary credentials"
                )
            })?;

        // Validate the session token against the stored hash
        let is_valid = iam
            .validate_session_token(&creds.access_key, session_token)
            .await
            .map_err(|e| {
                tracing::error!("Session token validation failed: {}", e);
                s3_error!(InternalError, "Authentication failed")
            })?;

        if !is_valid {
            return Err(s3_error!(
                AccessDenied,
                "Invalid session token for temporary credentials"
            ));
        }

        Ok(())
    }

    /// Check if the user is authorized to perform the given action on the resource.
    async fn check_authorization(
        &self,
        credentials: &Option<Credentials>,
        headers: &HeaderMap,
        action: &str,
        bucket: Option<&str>,
        key: Option<&str>,
    ) -> S3Result<()> {
        // Validate session token for temporary credentials first
        self.validate_session_token(credentials, headers).await?;

        let username = self.get_username(credentials).await?;

        // Root user bypasses all authorization checks
        if username == "root" {
            debug!("Root user authorized for {}", action);
            return Ok(());
        }

        let Some(iam) = &self.iam else {
            // No IAM provider means no policy enforcement - deny by default
            return Err(s3_error!(AccessDenied, "Access denied"));
        };

        // Build the resource ARN
        let resource = match (bucket, key) {
            (Some(b), Some(k)) => Resource::object(b, k),
            (Some(b), None) => Resource::bucket(b),
            (None, None) => Resource::all(),
            (None, Some(_)) => return Err(s3_error!(InvalidArgument, "Invalid resource")),
        };

        let iam_action = Action::from_operation(action).unwrap_or(Action::All);

        // Check IAM user policies
        let iam_authorized = iam
            .is_authorized(&username, &iam_action, &resource)
            .await
            .map_err(|e| {
                tracing::error!("IAM authorization check failed: {}", e);
                s3_error!(InternalError, "Authorization check failed")
            })?;

        if iam_authorized {
            debug!(
                "User {} authorized by IAM policy for {} on {:?}",
                username, iam_action, resource
            );
            return Ok(());
        }

        // Check bucket policy if we have a bucket
        if let Some(bucket_name) = bucket {
            let principal = if username == "anonymous" {
                Principal::Anonymous
            } else {
                Principal::User(username.clone())
            };

            let bucket_policy_effect = iam
                .is_authorized_by_bucket_policy(bucket_name, &principal, &iam_action, &resource)
                .await
                .map_err(|e| {
                    tracing::error!("Bucket policy check failed: {}", e);
                    s3_error!(InternalError, "Authorization check failed")
                })?;

            match bucket_policy_effect {
                Some(Effect::Allow) => {
                    debug!(
                        "User {} authorized by bucket policy for {} on {:?}",
                        username, iam_action, resource
                    );
                    return Ok(());
                }
                Some(Effect::Deny) => {
                    debug!(
                        "User {} denied by bucket policy for {} on {:?}",
                        username, iam_action, resource
                    );
                    return Err(s3_error!(AccessDenied, "Access denied by bucket policy"));
                }
                None => {
                    // No bucket policy match - fall through to default deny
                }
            }
        }

        // Default deny
        debug!(
            "User {} denied (default) for {} on {:?}",
            username, iam_action, resource
        );

        // Log authorization failure
        self.log_audit(
            AuditEvent {
                operation: action,
                bucket,
                key,
                principal: Some(&username),
                success: false,
                error_message: Some("Access denied"),
            },
            None,
        )
        .await;

        Err(s3_error!(AccessDenied, "Access denied"))
    }

    /// Log an audit event for security-relevant operations.
    async fn log_audit(&self, event: AuditEvent<'_>, ctx: Option<&RequestAuditContext>) {
        let entry = AuditLogEntry {
            id: ctx
                .map(|c| c.request_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            timestamp: Utc::now(),
            operation: event.operation.to_string(),
            bucket: event.bucket.map(|s| s.to_string()),
            key: event.key.map(|s| s.to_string()),
            principal: event.principal.map(|s| s.to_string()),
            source_ip: ctx.and_then(|c| c.source_ip.clone()),
            status_code: if event.success { 200 } else { 403 },
            error_code: event.error_message.map(|s| s.to_string()),
            duration_ms: None,
            bytes_sent: None,
            request_id: ctx
                .map(|c| c.request_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        };

        if let Err(e) = self.store.log_audit_event(entry).await {
            tracing::warn!("Failed to log audit event: {}", e);
        }
    }

    fn request_audit_context<T>(req: &S3Request<T>) -> RequestAuditContext {
        if let Some(ctx) = req.extensions.get::<RequestAuditContext>() {
            return ctx.clone();
        }

        RequestAuditContext {
            source_ip: extract_forwarded_ip(&req.headers),
            request_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

fn extract_forwarded_ip(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok()) {
        if let Some(first) = v.split(',').map(str::trim).find(|s| !s.is_empty()) {
            return Some(first.to_string());
        }
    }

    headers
        .get("x-real-ip")
        .and_then(|h| h.to_str().ok())
        .map(ToString::to_string)
}

fn parse_copy_source_range(range: &str) -> S3Result<(u64, Option<u64>)> {
    let range = range
        .strip_prefix("bytes=")
        .ok_or_else(|| s3_error!(InvalidArgument, "Invalid copy source range format"))?;

    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| s3_error!(InvalidArgument, "Invalid copy source range format"))?;

    let start = start
        .parse::<u64>()
        .map_err(|_| s3_error!(InvalidArgument, "Invalid range start"))?;

    if end.is_empty() {
        return Ok((start, None));
    }

    let end = end
        .parse::<u64>()
        .map_err(|_| s3_error!(InvalidArgument, "Invalid range end"))?;

    if end < start {
        return Err(s3_error!(
            InvalidArgument,
            "Invalid range: end before start"
        ));
    }

    Ok((start, Some(end)))
}

fn parse_http_range_header(range: &str) -> Option<(u64, Option<u64>)> {
    let range = range.strip_prefix("bytes=")?;
    let (start, end) = range.split_once('-')?;
    let start = start.parse::<u64>().ok()?;
    if end.is_empty() {
        Some((start, None))
    } else {
        let end = end.parse::<u64>().ok()?;
        if end < start {
            None
        } else {
            Some((start, Some(end)))
        }
    }
}

fn decode_object_tags(metadata: &HashMap<String, String>) -> S3Result<Vec<(String, String)>> {
    let Some(raw) = metadata.get(OBJECT_TAGS_METADATA_KEY) else {
        return Ok(Vec::new());
    };

    serde_json::from_str::<Vec<(String, String)>>(raw)
        .map_err(|_| s3_error!(InternalError, "Invalid stored object tags"))
}

fn encode_object_tags(tag_set: &[Tag]) -> S3Result<Vec<(String, String)>> {
    let mut out = Vec::with_capacity(tag_set.len());
    for tag in tag_set {
        let key = tag
            .key
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| s3_error!(InvalidTag, "Tag key is required"))?;
        let value = tag
            .value
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| s3_error!(InvalidTag, "Tag value is required"))?;
        out.push((key, value));
    }
    Ok(out)
}

/// Convert chrono DateTime to s3s Timestamp
fn to_timestamp(dt: chrono::DateTime<Utc>) -> Timestamp {
    let system_time =
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(dt.timestamp() as u64);
    Timestamp::from(system_time)
}

/// Convert s3s Timestamp to chrono DateTime
fn from_timestamp(ts: Timestamp) -> Option<chrono::DateTime<Utc>> {
    let odt: time::OffsetDateTime = ts.into();
    chrono::DateTime::from_timestamp(odt.unix_timestamp(), 0)
}

/// Convert strix_core StorageClass to s3s StorageClass
fn to_storage_class(sc: strix_core::StorageClass) -> StorageClass {
    StorageClass::from(sc.to_string())
}

/// Convert strix_core StorageClass to s3s ObjectStorageClass
fn to_object_storage_class(sc: strix_core::StorageClass) -> ObjectStorageClass {
    ObjectStorageClass::from(sc.to_string())
}

#[async_trait]
impl S3 for StrixS3Service {
    // === Bucket Operations ===

    #[instrument(skip(self, req))]
    async fn create_bucket(
        &self,
        req: S3Request<CreateBucketInput>,
    ) -> S3Result<S3Response<CreateBucketOutput>> {
        let audit_ctx = Self::request_audit_context(&req);
        let start = Instant::now();
        let bucket = req.input.bucket;
        let principal = self.get_username(&req.credentials).await.ok();

        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "CreateBucket",
            Some(&bucket),
            None,
        )
        .await?;

        let region = req
            .input
            .create_bucket_configuration
            .and_then(|c| c.location_constraint)
            .map(|l| l.as_str().to_string());

        self.store
            .create_bucket(
                &bucket,
                CreateBucketOpts {
                    region,
                    tenant_slug: None,
                },
            )
            .await
            .map_err(to_s3_error)?;

        let output = CreateBucketOutput {
            location: Some(format!("/{}", bucket)),
        };

        // Record metrics
        counter!("strix_s3_requests_total", "operation" => "CreateBucket").increment(1);
        counter!("strix_s3_buckets_created_total").increment(1);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "CreateBucket")
            .record(start.elapsed().as_secs_f64());

        // Log audit event
        self.log_audit(
            AuditEvent {
                operation: "CreateBucket",
                bucket: Some(&bucket),
                key: None,
                principal: principal.as_deref(),
                success: true,
                error_message: None,
            },
            Some(&audit_ctx),
        )
        .await;

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn delete_bucket(
        &self,
        req: S3Request<DeleteBucketInput>,
    ) -> S3Result<S3Response<DeleteBucketOutput>> {
        let audit_ctx = Self::request_audit_context(&req);
        let start = Instant::now();
        let bucket = &req.input.bucket;
        let principal = self.get_username(&req.credentials).await.ok();

        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "DeleteBucket",
            Some(bucket),
            None,
        )
        .await?;

        self.store
            .delete_bucket(bucket)
            .await
            .map_err(to_s3_error)?;

        // Record metrics
        counter!("strix_s3_requests_total", "operation" => "DeleteBucket").increment(1);
        counter!("strix_s3_buckets_deleted_total").increment(1);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "DeleteBucket")
            .record(start.elapsed().as_secs_f64());

        // Log audit event
        self.log_audit(
            AuditEvent {
                operation: "DeleteBucket",
                bucket: Some(bucket),
                key: None,
                principal: principal.as_deref(),
                success: true,
                error_message: None,
            },
            Some(&audit_ctx),
        )
        .await;

        Ok(S3Response::new(DeleteBucketOutput {}))
    }

    #[instrument(skip(self, req))]
    async fn list_buckets(
        &self,
        req: S3Request<ListBucketsInput>,
    ) -> S3Result<S3Response<ListBucketsOutput>> {
        // Authorization check
        self.check_authorization(&req.credentials, &req.headers, "ListBuckets", None, None)
            .await?;

        let buckets = self.store.list_buckets().await.map_err(to_s3_error)?;

        let output = ListBucketsOutput {
            buckets: Some(
                buckets
                    .into_iter()
                    .map(|b| Bucket {
                        name: Some(b.name),
                        creation_date: Some(to_timestamp(b.created_at)),
                        bucket_region: None,
                    })
                    .collect(),
            ),
            owner: Some(Owner {
                display_name: Some("strix".to_string()),
                id: Some("strix".to_string()),
            }),
            continuation_token: None,
            prefix: None,
        };

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn head_bucket(
        &self,
        req: S3Request<HeadBucketInput>,
    ) -> S3Result<S3Response<HeadBucketOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "HeadBucket",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let _info = self
            .store
            .head_bucket(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(HeadBucketOutput {
            bucket_location_type: None,
            bucket_location_name: None,
            bucket_region: None,
            access_point_alias: None,
        }))
    }

    #[instrument(skip(self, req))]
    async fn get_bucket_location(
        &self,
        req: S3Request<GetBucketLocationInput>,
    ) -> S3Result<S3Response<GetBucketLocationOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetBucketLocation",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let _info = self
            .store
            .head_bucket(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(GetBucketLocationOutput {
            location_constraint: None,
        }))
    }

    #[instrument(skip(self, req))]
    async fn get_bucket_versioning(
        &self,
        req: S3Request<GetBucketVersioningInput>,
    ) -> S3Result<S3Response<GetBucketVersioningOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetBucketVersioning",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let status = self
            .store
            .get_bucket_versioning(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(GetBucketVersioningOutput {
            status: match status {
                Some(true) => Some(BucketVersioningStatus::from_static(
                    BucketVersioningStatus::ENABLED,
                )),
                Some(false) => Some(BucketVersioningStatus::from_static(
                    BucketVersioningStatus::SUSPENDED,
                )),
                None => None,
            },
            mfa_delete: None,
        }))
    }

    #[instrument(skip(self, req))]
    async fn put_bucket_versioning(
        &self,
        req: S3Request<PutBucketVersioningInput>,
    ) -> S3Result<S3Response<PutBucketVersioningOutput>> {
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutBucketVersioning",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let input = req.input;
        let enabled = match input.versioning_configuration.status {
            Some(status) if status.as_str() == BucketVersioningStatus::ENABLED => true,
            Some(status) if status.as_str() == BucketVersioningStatus::SUSPENDED => false,
            _ => {
                return Err(s3_error!(
                    InvalidArgument,
                    "Versioning status must be Enabled or Suspended"
                ));
            }
        };

        self.store
            .set_bucket_versioning(&input.bucket, enabled)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(PutBucketVersioningOutput {}))
    }

    #[instrument(skip(self, req))]
    async fn get_bucket_tagging(
        &self,
        req: S3Request<GetBucketTaggingInput>,
    ) -> S3Result<S3Response<GetBucketTaggingOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetBucketTagging",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        match self.store.get_bucket_tagging(&req.input.bucket).await {
            Ok(Some(config)) => {
                let tag_set: TagSet = config
                    .tags
                    .into_iter()
                    .map(|t| s3s::dto::Tag {
                        key: Some(t.key),
                        value: Some(t.value),
                    })
                    .collect();
                Ok(S3Response::new(GetBucketTaggingOutput { tag_set }))
            }
            Ok(None) => Err(s3_error!(NoSuchTagSet, "No tags configured")),
            Err(e) => Err(to_s3_error(e)),
        }
    }

    #[instrument(skip(self, req))]
    async fn put_bucket_tagging(
        &self,
        req: S3Request<PutBucketTaggingInput>,
    ) -> S3Result<S3Response<PutBucketTaggingOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutBucketTagging",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let tags: Vec<CoreTag> = req
            .input
            .tagging
            .tag_set
            .into_iter()
            .map(|t| CoreTag {
                key: t.key.unwrap_or_default(),
                value: t.value.unwrap_or_default(),
            })
            .collect();

        let config = TaggingConfiguration { tags };
        self.store
            .put_bucket_tagging(&req.input.bucket, config)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(PutBucketTaggingOutput {}))
    }

    #[instrument(skip(self, req))]
    async fn delete_bucket_tagging(
        &self,
        req: S3Request<DeleteBucketTaggingInput>,
    ) -> S3Result<S3Response<DeleteBucketTaggingOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "DeleteBucketTagging",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        self.store
            .delete_bucket_tagging(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(DeleteBucketTaggingOutput {}))
    }

    #[instrument(skip(self, req))]
    async fn get_object_tagging(
        &self,
        req: S3Request<GetObjectTaggingInput>,
    ) -> S3Result<S3Response<GetObjectTaggingOutput>> {
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetObjectTagging",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let info = self
            .store
            .head_object(&req.input.bucket, &req.input.key)
            .await
            .map_err(to_s3_error)?;

        let tags = decode_object_tags(&info.metadata)?;
        let tag_set: TagSet = tags
            .into_iter()
            .map(|(k, v)| Tag {
                key: Some(k),
                value: Some(v),
            })
            .collect();

        Ok(S3Response::new(GetObjectTaggingOutput {
            tag_set,
            version_id: req.input.version_id,
        }))
    }

    #[instrument(skip(self, req))]
    async fn put_object_tagging(
        &self,
        req: S3Request<PutObjectTaggingInput>,
    ) -> S3Result<S3Response<PutObjectTaggingOutput>> {
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutObjectTagging",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;
        let info = self
            .store
            .head_object(&input.bucket, &input.key)
            .await
            .map_err(to_s3_error)?;

        let tags = encode_object_tags(&input.tagging.tag_set)?;
        let mut metadata = info.metadata;
        metadata.insert(
            OBJECT_TAGS_METADATA_KEY.to_string(),
            serde_json::to_string(&tags)
                .map_err(|_| s3_error!(InternalError, "Failed to encode object tags"))?,
        );

        let copy_resp = self
            .store
            .copy_object(
                &input.bucket,
                &input.key,
                &input.bucket,
                &input.key,
                CopyObjectOpts {
                    metadata_directive: MetadataDirective::Replace,
                    metadata,
                },
            )
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(PutObjectTaggingOutput {
            version_id: copy_resp.version_id,
        }))
    }

    #[instrument(skip(self, req))]
    async fn delete_object_tagging(
        &self,
        req: S3Request<DeleteObjectTaggingInput>,
    ) -> S3Result<S3Response<DeleteObjectTaggingOutput>> {
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "DeleteObjectTagging",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;
        let info = self
            .store
            .head_object(&input.bucket, &input.key)
            .await
            .map_err(to_s3_error)?;

        let mut metadata = info.metadata;
        metadata.remove(OBJECT_TAGS_METADATA_KEY);

        let copy_resp = self
            .store
            .copy_object(
                &input.bucket,
                &input.key,
                &input.bucket,
                &input.key,
                CopyObjectOpts {
                    metadata_directive: MetadataDirective::Replace,
                    metadata,
                },
            )
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(DeleteObjectTaggingOutput {
            version_id: copy_resp.version_id,
        }))
    }

    // === CORS Operations ===

    #[instrument(skip(self, req))]
    async fn get_bucket_cors(
        &self,
        req: S3Request<GetBucketCorsInput>,
    ) -> S3Result<S3Response<GetBucketCorsOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetBucketCors",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let cors_config = self
            .store
            .get_bucket_cors(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        match cors_config {
            Some(config) => {
                let rules: Vec<CORSRule> = config
                    .rules
                    .into_iter()
                    .map(|rule| {
                        let allowed_methods: Vec<String> =
                            rule.allowed_methods.iter().map(|m| m.to_string()).collect();
                        CORSRule {
                            id: rule.id,
                            allowed_origins: rule.allowed_origins,
                            allowed_methods,
                            allowed_headers: if rule.allowed_headers.is_empty() {
                                None
                            } else {
                                Some(rule.allowed_headers)
                            },
                            expose_headers: if rule.expose_headers.is_empty() {
                                None
                            } else {
                                Some(rule.expose_headers)
                            },
                            max_age_seconds: rule.max_age_seconds.map(|v| v as i32),
                        }
                    })
                    .collect();

                Ok(S3Response::new(GetBucketCorsOutput {
                    cors_rules: Some(rules),
                }))
            }
            None => Err(s3_error!(
                NoSuchCORSConfiguration,
                "CORS configuration not found"
            )),
        }
    }

    #[instrument(skip(self, req))]
    async fn put_bucket_cors(
        &self,
        req: S3Request<PutBucketCorsInput>,
    ) -> S3Result<S3Response<PutBucketCorsOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutBucketCors",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let input = req.input;
        let cors_config = input.cors_configuration;

        // Convert s3s CORS rules to our internal format
        let rules: Vec<strix_core::CorsRule> = cors_config
            .cors_rules
            .into_iter()
            .map(|rule| {
                let allowed_methods: Vec<strix_core::CorsMethod> = rule
                    .allowed_methods
                    .into_iter()
                    .filter_map(|m| m.parse().ok())
                    .collect();

                strix_core::CorsRule {
                    id: rule.id,
                    allowed_origins: rule.allowed_origins,
                    allowed_methods,
                    allowed_headers: rule.allowed_headers.unwrap_or_default(),
                    expose_headers: rule.expose_headers.unwrap_or_default(),
                    max_age_seconds: rule.max_age_seconds.map(|v| v as u32),
                }
            })
            .collect();

        let config = strix_core::CorsConfiguration { rules };

        self.store
            .put_bucket_cors(&input.bucket, config)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(PutBucketCorsOutput {}))
    }

    #[instrument(skip(self, req))]
    async fn delete_bucket_cors(
        &self,
        req: S3Request<DeleteBucketCorsInput>,
    ) -> S3Result<S3Response<DeleteBucketCorsOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "DeleteBucketCors",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        self.store
            .delete_bucket_cors(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(DeleteBucketCorsOutput {}))
    }

    // === Object Operations ===

    #[instrument(skip(self, req))]
    async fn get_object(
        &self,
        req: S3Request<GetObjectInput>,
    ) -> S3Result<S3Response<GetObjectOutput>> {
        let start = Instant::now();

        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetObject",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let header_range = req
            .headers
            .get("range")
            .and_then(|h| h.to_str().ok())
            .and_then(parse_http_range_header);
        let input = req.input;

        // Parse range from the Range header
        let range = input
            .range
            .as_ref()
            .and_then(|r| {
                // s3s Range is a structured type, convert it
                match r {
                    Range::Int { first, last } => Some((*first, *last)),
                    Range::Suffix { length: _ } => None, // Not supported yet
                }
            })
            .or(header_range);

        let opts = GetObjectOpts {
            range,
            if_match: input.if_match,
            if_none_match: input.if_none_match,
            if_modified_since: input.if_modified_since.and_then(from_timestamp),
            if_unmodified_since: input.if_unmodified_since.and_then(from_timestamp),
            version_id: input.version_id,
            sse_customer_key: input.sse_customer_key,
            sse_customer_key_md5: input.sse_customer_key_md5,
        };
        let requested_range = opts.range;

        let response = self
            .store
            .get_object(&input.bucket, &input.key, opts)
            .await
            .map_err(to_s3_error)?;

        // Stream full responses; for range responses, materialize exact bytes to ensure
        // precise Content-Length/Content-Range values for strict clients.
        let full_size = response.info.size;
        let (body_blob, content_length, content_range) = if let Some((start, _)) = requested_range {
            let mut data = Vec::new();
            let mut body = response.body;
            while let Some(chunk) = body.next().await {
                let chunk = chunk
                    .map_err(|_| s3_error!(InternalError, "Failed reading ranged object stream"))?;
                data.extend_from_slice(&chunk);
            }
            let length = data.len() as u64;
            let end = start.saturating_add(length.saturating_sub(1));
            let object_body: strix_core::ObjectBody =
                Box::pin(futures::stream::once(async move { Ok(Bytes::from(data)) }));
            let streaming_body = S3BodyStream::new(object_body, length);
            let http_body = streaming_body.into_s3s_body();
            (
                StreamingBlob::from(http_body),
                length,
                Some(format!("bytes {}-{}/{}", start, end, full_size)),
            )
        } else {
            let content_length = full_size;
            let streaming_body = S3BodyStream::new(response.body, content_length);
            let http_body = streaming_body.into_s3s_body();
            (StreamingBlob::from(http_body), content_length, None)
        };

        // Convert content_type string to Mime
        let content_type = response.info.content_type.and_then(|ct| ct.parse().ok());

        let output = GetObjectOutput {
            body: Some(body_blob),
            content_length: Some(content_length as i64),
            content_range,
            content_type,
            e_tag: Some(response.info.etag),
            last_modified: Some(to_timestamp(response.info.last_modified)),
            metadata: if response.info.metadata.is_empty() {
                None
            } else {
                Some(response.info.metadata)
            },
            storage_class: Some(to_storage_class(response.info.storage_class)),
            ..Default::default()
        };

        // Record metrics
        counter!("strix_s3_requests_total", "operation" => "GetObject").increment(1);
        counter!("strix_s3_objects_retrieved_total").increment(1);
        counter!("strix_s3_bytes_sent_total").increment(content_length);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "GetObject")
            .record(start.elapsed().as_secs_f64());

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn put_object(
        &self,
        req: S3Request<PutObjectInput>,
    ) -> S3Result<S3Response<PutObjectOutput>> {
        let start = Instant::now();

        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutObject",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;

        let body = input
            .body
            .ok_or_else(|| s3_error!(InvalidRequest, "Missing body"))?;
        let content_length = input.content_length.unwrap_or(0) as u64;

        // Convert Mime to String for content_type
        let content_type = input.content_type.map(|m| m.to_string());

        // Parse server-side encryption settings from headers
        let server_side_encryption =
            input
                .server_side_encryption
                .and_then(|sse| match sse.as_str() {
                    "AES256" => Some(strix_core::ServerSideEncryption::Aes256),
                    _ => None,
                });

        let opts = PutObjectOpts {
            content_type,
            content_encoding: input.content_encoding,
            content_disposition: input.content_disposition,
            cache_control: input.cache_control,
            metadata: input.metadata.unwrap_or_default(),
            storage_class: input.storage_class.map(|s| match s.as_str() {
                "REDUCED_REDUNDANCY" => strix_core::StorageClass::ReducedRedundancy,
                "GLACIER" => strix_core::StorageClass::Glacier,
                "DEEP_ARCHIVE" => strix_core::StorageClass::DeepArchive,
                _ => strix_core::StorageClass::Standard,
            }),
            server_side_encryption,
            sse_customer_key: input.sse_customer_key,
            sse_customer_key_md5: input.sse_customer_key_md5,
        };

        // Convert body to our stream type
        let body_stream =
            body.map(|result| result.map_err(|e| std::io::Error::other(e.to_string())));

        let response = self
            .store
            .put_object(
                &input.bucket,
                &input.key,
                Box::pin(body_stream),
                content_length,
                opts,
            )
            .await
            .map_err(to_s3_error)?;

        let output = PutObjectOutput {
            e_tag: Some(response.etag),
            version_id: response.version_id,
            ..Default::default()
        };

        // Record metrics
        counter!("strix_s3_requests_total", "operation" => "PutObject").increment(1);
        counter!("strix_s3_objects_created_total").increment(1);
        counter!("strix_s3_bytes_received_total").increment(content_length);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "PutObject")
            .record(start.elapsed().as_secs_f64());

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn delete_object(
        &self,
        req: S3Request<DeleteObjectInput>,
    ) -> S3Result<S3Response<DeleteObjectOutput>> {
        let audit_ctx = Self::request_audit_context(&req);
        let start = Instant::now();
        let bucket = &req.input.bucket;
        let key = &req.input.key;
        let principal = self.get_username(&req.credentials).await.ok();

        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "DeleteObject",
            Some(bucket),
            Some(key),
        )
        .await?;

        let response = self
            .store
            .delete_object_version(bucket, key, req.input.version_id.as_deref())
            .await
            .map_err(to_s3_error)?;

        // Record metrics
        counter!("strix_s3_requests_total", "operation" => "DeleteObject").increment(1);
        counter!("strix_s3_objects_deleted_total").increment(1);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "DeleteObject")
            .record(start.elapsed().as_secs_f64());

        // Log audit event
        self.log_audit(
            AuditEvent {
                operation: "DeleteObject",
                bucket: Some(bucket),
                key: Some(key),
                principal: principal.as_deref(),
                success: true,
                error_message: None,
            },
            Some(&audit_ctx),
        )
        .await;

        Ok(S3Response::new(DeleteObjectOutput {
            delete_marker: Some(response.delete_marker),
            version_id: response.version_id,
            request_charged: None,
        }))
    }

    #[instrument(skip(self, req))]
    async fn delete_objects(
        &self,
        req: S3Request<DeleteObjectsInput>,
    ) -> S3Result<S3Response<DeleteObjectsOutput>> {
        // Authorization check - check permission on bucket for DeleteObject
        // (we check per-object below for more granular control, but need at least bucket access)
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "DeleteObjects",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let input = req.input;
        let bucket = &input.bucket;

        let objects = input.delete.objects;

        let mut deleted = Vec::new();
        let mut errors = Vec::new();

        for obj in objects {
            let key = obj.key.to_string();
            let version_id = obj.version_id.clone();

            if let Err(e) = self
                .check_authorization(
                    &req.credentials,
                    &req.headers,
                    "DeleteObject",
                    Some(bucket),
                    Some(&key),
                )
                .await
            {
                errors.push(s3s::dto::Error {
                    key: Some(key),
                    version_id,
                    code: Some("AccessDenied".to_string()),
                    message: Some(e.to_string()),
                });
                continue;
            }

            match self
                .store
                .delete_object_version(bucket, &key, version_id.as_deref())
                .await
            {
                Ok(_) => {
                    deleted.push(DeletedObject {
                        key: Some(key),
                        version_id,
                        delete_marker: None,
                        delete_marker_version_id: None,
                    });
                }
                Err(e) => {
                    errors.push(s3s::dto::Error {
                        key: Some(key),
                        version_id: None,
                        code: Some("InternalError".to_string()),
                        message: Some(e.to_string()),
                    });
                }
            }
        }

        let output = DeleteObjectsOutput {
            deleted: if deleted.is_empty() {
                None
            } else {
                Some(deleted)
            },
            errors: if errors.is_empty() {
                None
            } else {
                Some(errors)
            },
            request_charged: None,
        };

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn head_object(
        &self,
        req: S3Request<HeadObjectInput>,
    ) -> S3Result<S3Response<HeadObjectOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "HeadObject",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let info = self
            .store
            .head_object(&req.input.bucket, &req.input.key)
            .await
            .map_err(to_s3_error)?;

        // Convert content_type string to Mime
        let content_type = info.content_type.and_then(|ct| ct.parse().ok());

        let output = HeadObjectOutput {
            content_length: Some(info.size as i64),
            content_type,
            e_tag: Some(info.etag),
            last_modified: Some(to_timestamp(info.last_modified)),
            metadata: if info.metadata.is_empty() {
                None
            } else {
                Some(info.metadata)
            },
            storage_class: Some(to_storage_class(info.storage_class)),
            ..Default::default()
        };

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn list_objects(
        &self,
        req: S3Request<ListObjectsInput>,
    ) -> S3Result<S3Response<ListObjectsOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "ListObjects",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let input = req.input;

        let opts = ListObjectsOpts {
            prefix: input.prefix.clone(),
            delimiter: input.delimiter.clone(),
            max_keys: input.max_keys.map(|k| k as u32),
            continuation_token: None,
            start_after: input.marker.clone(),
        };

        let response = self
            .store
            .list_objects(&input.bucket, opts)
            .await
            .map_err(to_s3_error)?;

        let output = ListObjectsOutput {
            name: Some(input.bucket),
            prefix: input.prefix,
            delimiter: input.delimiter,
            max_keys: Some(input.max_keys.unwrap_or(1000)),
            is_truncated: Some(response.is_truncated),
            marker: input.marker,
            next_marker: response.next_continuation_token,
            contents: Some(
                response
                    .objects
                    .into_iter()
                    .map(|obj| Object {
                        key: Some(obj.key),
                        size: Some(obj.size as i64),
                        e_tag: Some(obj.etag),
                        last_modified: Some(to_timestamp(obj.last_modified)),
                        storage_class: Some(to_object_storage_class(obj.storage_class)),
                        owner: None,
                        checksum_algorithm: None,
                        checksum_type: None,
                        restore_status: None,
                    })
                    .collect(),
            ),
            common_prefixes: if response.common_prefixes.is_empty() {
                None
            } else {
                Some(
                    response
                        .common_prefixes
                        .into_iter()
                        .map(|p| CommonPrefix { prefix: Some(p) })
                        .collect(),
                )
            },
            encoding_type: None,
            request_charged: None,
        };

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn list_objects_v2(
        &self,
        req: S3Request<ListObjectsV2Input>,
    ) -> S3Result<S3Response<ListObjectsV2Output>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "ListObjectsV2",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let input = req.input;

        let opts = ListObjectsOpts {
            prefix: input.prefix.clone(),
            delimiter: input.delimiter.clone(),
            max_keys: input.max_keys.map(|k| k as u32),
            continuation_token: input.continuation_token.clone(),
            start_after: input.start_after.clone(),
        };

        let response = self
            .store
            .list_objects(&input.bucket, opts)
            .await
            .map_err(to_s3_error)?;

        let output = ListObjectsV2Output {
            name: Some(input.bucket),
            prefix: input.prefix,
            delimiter: input.delimiter,
            max_keys: Some(input.max_keys.unwrap_or(1000)),
            is_truncated: Some(response.is_truncated),
            continuation_token: input.continuation_token,
            next_continuation_token: response.next_continuation_token,
            key_count: Some(response.objects.len() as i32),
            contents: Some(
                response
                    .objects
                    .into_iter()
                    .map(|obj| Object {
                        key: Some(obj.key),
                        size: Some(obj.size as i64),
                        e_tag: Some(obj.etag),
                        last_modified: Some(to_timestamp(obj.last_modified)),
                        storage_class: Some(to_object_storage_class(obj.storage_class)),
                        owner: None,
                        checksum_algorithm: None,
                        checksum_type: None,
                        restore_status: None,
                    })
                    .collect(),
            ),
            common_prefixes: if response.common_prefixes.is_empty() {
                None
            } else {
                Some(
                    response
                        .common_prefixes
                        .into_iter()
                        .map(|p| CommonPrefix { prefix: Some(p) })
                        .collect(),
                )
            },
            encoding_type: None,
            start_after: input.start_after,
            request_charged: None,
        };

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn copy_object(
        &self,
        req: S3Request<CopyObjectInput>,
    ) -> S3Result<S3Response<CopyObjectOutput>> {
        // Authorization check for destination (PutObject)
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutObject",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;

        // CopySource is not Option - extract the string from it
        let source_str = match &input.copy_source {
            CopySource::Bucket { bucket, key, .. } => {
                format!("{}/{}", bucket, key)
            }
            CopySource::AccessPoint { .. } => {
                return Err(s3_error!(InvalidArgument, "Access point not supported"));
            }
        };

        // Copy source format: bucket/key
        let (src_bucket, src_key) = source_str
            .split_once('/')
            .ok_or_else(|| s3_error!(InvalidArgument, "Invalid copy source format"))?;

        // Authorization check for source (GetObject)
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetObject",
            Some(src_bucket),
            Some(src_key),
        )
        .await?;

        let metadata_directive = match input.metadata_directive {
            Some(d) if d.as_str() == "REPLACE" => MetadataDirective::Replace,
            _ => MetadataDirective::Copy,
        };

        let opts = CopyObjectOpts {
            metadata_directive,
            metadata: input.metadata.unwrap_or_default(),
        };

        let response = self
            .store
            .copy_object(src_bucket, src_key, &input.bucket, &input.key, opts)
            .await
            .map_err(to_s3_error)?;

        let output = CopyObjectOutput {
            copy_object_result: Some(CopyObjectResult {
                e_tag: Some(response.etag),
                last_modified: Some(to_timestamp(response.last_modified)),
                checksum_crc32: None,
                checksum_crc32c: None,
                checksum_crc64nvme: None,
                checksum_sha1: None,
                checksum_sha256: None,
                checksum_type: None,
            }),
            version_id: None,
            expiration: None,
            copy_source_version_id: None,
            server_side_encryption: None,
            sse_customer_algorithm: None,
            sse_customer_key_md5: None,
            ssekms_key_id: None,
            ssekms_encryption_context: None,
            bucket_key_enabled: None,
            request_charged: None,
        };

        Ok(S3Response::new(output))
    }

    // === Multipart Upload Operations ===

    #[instrument(skip(self, req))]
    async fn create_multipart_upload(
        &self,
        req: S3Request<CreateMultipartUploadInput>,
    ) -> S3Result<S3Response<CreateMultipartUploadOutput>> {
        let start = Instant::now();

        // Authorization check - creating multipart upload requires PutObject
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutObject",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;

        // Convert Mime to String for content_type
        let content_type = input.content_type.map(|m| m.to_string());

        let server_side_encryption = match input.server_side_encryption.as_ref().map(|s| s.as_str())
        {
            Some("AES256") => Some(strix_core::ServerSideEncryption::Aes256),
            _ => {
                if input.sse_customer_algorithm.as_deref() == Some("AES256")
                    || input.sse_customer_key.is_some()
                    || input.sse_customer_key_md5.is_some()
                {
                    Some(strix_core::ServerSideEncryption::SseC)
                } else {
                    None
                }
            }
        };

        let opts = PutObjectOpts {
            content_type,
            content_encoding: input.content_encoding,
            content_disposition: input.content_disposition,
            cache_control: input.cache_control,
            metadata: input.metadata.unwrap_or_default(),
            storage_class: input.storage_class.map(|s| match s.as_str() {
                "REDUCED_REDUNDANCY" => strix_core::StorageClass::ReducedRedundancy,
                "GLACIER" => strix_core::StorageClass::Glacier,
                "DEEP_ARCHIVE" => strix_core::StorageClass::DeepArchive,
                _ => strix_core::StorageClass::Standard,
            }),
            server_side_encryption,
            sse_customer_key: input.sse_customer_key,
            sse_customer_key_md5: input.sse_customer_key_md5,
        };

        let upload = self
            .store
            .create_multipart_upload(&input.bucket, &input.key, opts)
            .await
            .map_err(to_s3_error)?;

        let output = CreateMultipartUploadOutput {
            bucket: Some(upload.bucket),
            key: Some(upload.key),
            upload_id: Some(upload.upload_id),
            ..Default::default()
        };

        // Record metrics
        counter!("strix_s3_requests_total", "operation" => "CreateMultipartUpload").increment(1);
        counter!("strix_s3_multipart_uploads_started_total").increment(1);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "CreateMultipartUpload")
            .record(start.elapsed().as_secs_f64());

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn upload_part(
        &self,
        req: S3Request<UploadPartInput>,
    ) -> S3Result<S3Response<UploadPartOutput>> {
        let start = Instant::now();

        // Authorization check - upload part requires PutObject
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutObject",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;

        let upload = strix_core::MultipartUpload {
            upload_id: input.upload_id,
            bucket: input.bucket.clone(),
            key: input.key.clone(),
            initiated: Utc::now(), // Not used in this context
        };

        let part_number = input.part_number as u16;

        let body = input
            .body
            .ok_or_else(|| s3_error!(InvalidRequest, "Missing body"))?;
        let content_length = input.content_length.unwrap_or(0) as u64;

        let body_stream =
            body.map(|result| result.map_err(|e| std::io::Error::other(e.to_string())));

        let part_info = self
            .store
            .upload_part(&upload, part_number, Box::pin(body_stream), content_length)
            .await
            .map_err(to_s3_error)?;

        let output = UploadPartOutput {
            e_tag: Some(part_info.etag),
            ..Default::default()
        };

        // Record metrics
        counter!("strix_s3_requests_total", "operation" => "UploadPart").increment(1);
        counter!("strix_s3_bytes_received_total").increment(content_length);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "UploadPart")
            .record(start.elapsed().as_secs_f64());

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn upload_part_copy(
        &self,
        req: S3Request<UploadPartCopyInput>,
    ) -> S3Result<S3Response<UploadPartCopyOutput>> {
        let start = Instant::now();
        let credentials = req.credentials.clone();
        let headers = req.headers.clone();
        let input = req.input;

        // Authorization check for destination write
        self.check_authorization(
            &credentials,
            &headers,
            "PutObject",
            Some(&input.bucket),
            Some(&input.key),
        )
        .await?;

        // Extract source
        let (src_bucket, src_key, src_version_id) = match &input.copy_source {
            CopySource::Bucket {
                bucket,
                key,
                version_id,
            } => (
                bucket.to_string(),
                key.to_string(),
                version_id.as_ref().map(|v| v.to_string()),
            ),
            CopySource::AccessPoint { .. } => {
                return Err(s3_error!(
                    InvalidArgument,
                    "Access point copy source not supported"
                ));
            }
        };

        // Authorization check for source read
        self.check_authorization(
            &credentials,
            &headers,
            "GetObject",
            Some(&src_bucket),
            Some(&src_key),
        )
        .await?;

        let range = input
            .copy_source_range
            .as_deref()
            .map(parse_copy_source_range)
            .transpose()?;

        let source = self
            .store
            .get_object(
                &src_bucket,
                &src_key,
                GetObjectOpts {
                    range,
                    version_id: src_version_id,
                    sse_customer_key: input.copy_source_sse_customer_key,
                    sse_customer_key_md5: input.copy_source_sse_customer_key_md5,
                    ..Default::default()
                },
            )
            .await
            .map_err(to_s3_error)?;

        let mut data = Vec::new();
        let mut body = source.body;
        while let Some(chunk) = body.next().await {
            let chunk = chunk
                .map_err(|_| s3_error!(InternalError, "Failed reading source object stream"))?;
            data.extend_from_slice(&chunk);
        }
        let content_length = data.len() as u64;

        let upload = strix_core::MultipartUpload {
            upload_id: input.upload_id,
            bucket: input.bucket.clone(),
            key: input.key.clone(),
            initiated: Utc::now(),
        };

        let part_info = self
            .store
            .upload_part(
                &upload,
                input.part_number as u16,
                Box::pin(futures::stream::once(async move { Ok(Bytes::from(data)) })),
                content_length,
            )
            .await
            .map_err(to_s3_error)?;

        let output = UploadPartCopyOutput {
            copy_part_result: Some(CopyPartResult {
                e_tag: Some(part_info.etag),
                last_modified: Some(to_timestamp(part_info.last_modified)),
                checksum_crc32: None,
                checksum_crc32c: None,
                checksum_crc64nvme: None,
                checksum_sha1: None,
                checksum_sha256: None,
            }),
            ..Default::default()
        };

        counter!("strix_s3_requests_total", "operation" => "UploadPartCopy").increment(1);
        counter!("strix_s3_bytes_received_total").increment(content_length);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "UploadPartCopy")
            .record(start.elapsed().as_secs_f64());

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn complete_multipart_upload(
        &self,
        req: S3Request<CompleteMultipartUploadInput>,
    ) -> S3Result<S3Response<CompleteMultipartUploadOutput>> {
        let start = Instant::now();

        // Authorization check - completing multipart upload requires PutObject
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutObject",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;

        let upload = strix_core::MultipartUpload {
            upload_id: input.upload_id.clone(),
            bucket: input.bucket.clone(),
            key: input.key.clone(),
            initiated: Utc::now(),
        };

        let parts = input
            .multipart_upload
            .and_then(|mp| mp.parts)
            .unwrap_or_default()
            .into_iter()
            .map(|p| CompletePart {
                part_number: p.part_number.unwrap_or(0) as u16,
                etag: p.e_tag.unwrap_or_default(),
            })
            .collect();

        let response = self
            .store
            .complete_multipart_upload(&upload, parts)
            .await
            .map_err(to_s3_error)?;

        let output = CompleteMultipartUploadOutput {
            bucket: Some(input.bucket),
            key: Some(input.key),
            e_tag: Some(response.etag),
            location: None,
            version_id: response.version_id,
            ..Default::default()
        };

        // Record metrics
        counter!("strix_s3_requests_total", "operation" => "CompleteMultipartUpload").increment(1);
        counter!("strix_s3_multipart_uploads_completed_total").increment(1);
        counter!("strix_s3_objects_created_total").increment(1);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "CompleteMultipartUpload")
            .record(start.elapsed().as_secs_f64());

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn abort_multipart_upload(
        &self,
        req: S3Request<AbortMultipartUploadInput>,
    ) -> S3Result<S3Response<AbortMultipartUploadOutput>> {
        let start = Instant::now();

        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "AbortMultipartUpload",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;

        let upload = strix_core::MultipartUpload {
            upload_id: input.upload_id,
            bucket: input.bucket,
            key: input.key,
            initiated: Utc::now(),
        };

        self.store
            .abort_multipart_upload(&upload)
            .await
            .map_err(to_s3_error)?;

        // Record metrics
        counter!("strix_s3_requests_total", "operation" => "AbortMultipartUpload").increment(1);
        counter!("strix_s3_multipart_uploads_aborted_total").increment(1);
        metrics::histogram!("strix_s3_request_duration_seconds", "operation" => "AbortMultipartUpload")
            .record(start.elapsed().as_secs_f64());

        Ok(S3Response::new(AbortMultipartUploadOutput {
            request_charged: None,
        }))
    }

    #[instrument(skip(self, req))]
    async fn list_parts(
        &self,
        req: S3Request<ListPartsInput>,
    ) -> S3Result<S3Response<ListPartsOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "ListParts",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;

        let upload = strix_core::MultipartUpload {
            upload_id: input.upload_id.clone(),
            bucket: input.bucket.clone(),
            key: input.key.clone(),
            initiated: Utc::now(),
        };

        let part_number_marker = input
            .part_number_marker
            .as_ref()
            .and_then(|s| s.parse::<u16>().ok());

        let max_parts = input.max_parts.map(|m| m as u32).unwrap_or(1000);
        let opts = ListPartsOpts {
            max_parts: Some(max_parts),
            part_number_marker,
        };

        let response = self
            .store
            .list_parts(&upload, opts)
            .await
            .map_err(to_s3_error)?;

        let output = ListPartsOutput {
            bucket: Some(input.bucket),
            key: Some(input.key),
            upload_id: Some(input.upload_id),
            max_parts: Some(max_parts as i32),
            is_truncated: Some(response.is_truncated),
            part_number_marker: input.part_number_marker,
            next_part_number_marker: response.next_part_number_marker.map(|m| m.to_string()),
            parts: Some(
                response
                    .parts
                    .into_iter()
                    .map(|p| Part {
                        part_number: Some(p.part_number as i32),
                        e_tag: Some(p.etag),
                        size: Some(p.size as i64),
                        last_modified: Some(to_timestamp(p.last_modified)),
                        checksum_crc32: None,
                        checksum_crc32c: None,
                        checksum_crc64nvme: None,
                        checksum_sha1: None,
                        checksum_sha256: None,
                    })
                    .collect(),
            ),
            ..Default::default()
        };

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn list_multipart_uploads(
        &self,
        req: S3Request<ListMultipartUploadsInput>,
    ) -> S3Result<S3Response<ListMultipartUploadsOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "ListMultipartUploads",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let input = req.input;

        let opts = ListUploadsOpts {
            prefix: input.prefix.clone(),
            delimiter: input.delimiter.clone(),
            max_uploads: input.max_uploads.map(|m| m as u32),
            key_marker: input.key_marker.clone(),
            upload_id_marker: input.upload_id_marker.clone(),
        };

        let response = self
            .store
            .list_multipart_uploads(&input.bucket, opts)
            .await
            .map_err(to_s3_error)?;

        let output = ListMultipartUploadsOutput {
            bucket: Some(input.bucket),
            prefix: input.prefix,
            delimiter: input.delimiter,
            max_uploads: Some(input.max_uploads.unwrap_or(1000)),
            is_truncated: Some(response.is_truncated),
            key_marker: input.key_marker,
            upload_id_marker: input.upload_id_marker,
            next_key_marker: response.next_key_marker,
            next_upload_id_marker: response.next_upload_id_marker,
            uploads: Some(
                response
                    .uploads
                    .into_iter()
                    .map(|u| MultipartUpload {
                        upload_id: Some(u.upload_id),
                        key: Some(u.key),
                        initiated: Some(to_timestamp(u.initiated)),
                        owner: None,
                        initiator: None,
                        storage_class: None,
                        checksum_algorithm: None,
                        checksum_type: None,
                    })
                    .collect(),
            ),
            common_prefixes: if response.common_prefixes.is_empty() {
                None
            } else {
                Some(
                    response
                        .common_prefixes
                        .into_iter()
                        .map(|p| CommonPrefix { prefix: Some(p) })
                        .collect(),
                )
            },
            encoding_type: None,
            request_charged: None,
        };

        Ok(S3Response::new(output))
    }

    #[instrument(skip(self, req))]
    async fn list_object_versions(
        &self,
        req: S3Request<ListObjectVersionsInput>,
    ) -> S3Result<S3Response<ListObjectVersionsOutput>> {
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "ListObjectVersions",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let input = req.input;
        let opts = ListVersionsOpts {
            prefix: input.prefix.clone(),
            delimiter: input.delimiter.clone(),
            max_keys: input.max_keys.map(|v| v as u32),
            key_marker: input.key_marker.clone(),
            version_id_marker: input.version_id_marker.clone(),
        };

        let response = self
            .store
            .list_object_versions(&input.bucket, opts)
            .await
            .map_err(to_s3_error)?;

        let output = ListObjectVersionsOutput {
            name: Some(input.bucket),
            prefix: input.prefix,
            delimiter: input.delimiter,
            max_keys: Some(input.max_keys.unwrap_or(1000)),
            key_marker: input.key_marker,
            version_id_marker: input.version_id_marker,
            is_truncated: Some(response.is_truncated),
            next_key_marker: response.next_key_marker,
            next_version_id_marker: response.next_version_id_marker,
            versions: if response.versions.is_empty() {
                None
            } else {
                Some(
                    response
                        .versions
                        .into_iter()
                        .map(|v| ObjectVersion {
                            checksum_algorithm: None,
                            checksum_type: None,
                            e_tag: v.etag,
                            is_latest: Some(v.is_latest),
                            key: Some(v.key),
                            last_modified: Some(to_timestamp(v.last_modified)),
                            owner: None,
                            restore_status: None,
                            size: v.size.map(|s| s as i64),
                            storage_class: v
                                .storage_class
                                .map(|s| ObjectVersionStorageClass::from(s.to_string())),
                            version_id: Some(v.version_id),
                        })
                        .collect(),
                )
            },
            delete_markers: if response.delete_markers.is_empty() {
                None
            } else {
                Some(
                    response
                        .delete_markers
                        .into_iter()
                        .map(|v| DeleteMarkerEntry {
                            is_latest: Some(v.is_latest),
                            key: Some(v.key),
                            last_modified: Some(to_timestamp(v.last_modified)),
                            owner: None,
                            version_id: Some(v.version_id),
                        })
                        .collect(),
                )
            },
            common_prefixes: if response.common_prefixes.is_empty() {
                None
            } else {
                Some(
                    response
                        .common_prefixes
                        .into_iter()
                        .map(|p| CommonPrefix { prefix: Some(p) })
                        .collect(),
                )
            },
            encoding_type: None,
            request_charged: None,
        };

        Ok(S3Response::new(output))
    }

    // === Lifecycle Operations ===

    #[instrument(skip(self, req))]
    async fn get_bucket_lifecycle_configuration(
        &self,
        req: S3Request<GetBucketLifecycleConfigurationInput>,
    ) -> S3Result<S3Response<GetBucketLifecycleConfigurationOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetBucketLifecycleConfiguration",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let config = self
            .store
            .get_bucket_lifecycle(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        match config {
            Some(lifecycle) => {
                let rules: Vec<LifecycleRule> = lifecycle
                    .rules
                    .into_iter()
                    .map(|rule| {
                        let expiration = rule.expiration.map(|exp| LifecycleExpiration {
                            days: exp.days.map(|d| d as i32),
                            date: None,
                            expired_object_delete_marker: Some(exp.expired_object_delete_marker),
                        });

                        let transitions = if rule.transitions.is_empty() {
                            None
                        } else {
                            Some(
                                rule.transitions
                                    .into_iter()
                                    .map(|t| Transition {
                                        days: t.days.map(|d| d as i32),
                                        date: None,
                                        storage_class: Some(TransitionStorageClass::from(
                                            t.storage_class.to_string(),
                                        )),
                                    })
                                    .collect(),
                            )
                        };

                        let noncurrent_version_expiration =
                            rule.noncurrent_version_expiration.map(|nve| {
                                NoncurrentVersionExpiration {
                                    noncurrent_days: Some(nve.noncurrent_days as i32),
                                    newer_noncurrent_versions: nve
                                        .newer_noncurrent_versions
                                        .map(|v| v as i32),
                                }
                            });

                        let abort_incomplete_multipart_upload = rule
                            .abort_incomplete_multipart_upload
                            .map(|abort| AbortIncompleteMultipartUpload {
                                days_after_initiation: Some(abort.days_after_initiation as i32),
                            });

                        let status_str = if rule.enabled { "Enabled" } else { "Disabled" };

                        LifecycleRule {
                            id: Some(rule.id),
                            status: ExpirationStatus::from(status_str.to_string()),
                            filter: None, // Use deprecated prefix field instead
                            expiration,
                            transitions,
                            noncurrent_version_expiration,
                            abort_incomplete_multipart_upload,
                            prefix: rule.prefix,
                            noncurrent_version_transitions: None,
                        }
                    })
                    .collect();

                Ok(S3Response::new(GetBucketLifecycleConfigurationOutput {
                    rules: Some(rules),
                    transition_default_minimum_object_size: None,
                }))
            }
            None => Err(s3_error!(
                NoSuchLifecycleConfiguration,
                "Lifecycle configuration not found"
            )),
        }
    }

    #[instrument(skip(self, req))]
    async fn put_bucket_lifecycle_configuration(
        &self,
        req: S3Request<PutBucketLifecycleConfigurationInput>,
    ) -> S3Result<S3Response<PutBucketLifecycleConfigurationOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutBucketLifecycleConfiguration",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let input = req.input;

        let s3_rules = input
            .lifecycle_configuration
            .map(|c| c.rules)
            .unwrap_or_default();

        let rules: Vec<strix_core::LifecycleRule> = s3_rules
            .into_iter()
            .map(|rule| {
                // Use deprecated prefix field
                let prefix = rule.prefix.clone();

                let expiration = rule.expiration.map(|exp| strix_core::LifecycleExpiration {
                    days: exp.days.map(|d| d as u32),
                    date: None,
                    expired_object_delete_marker: exp.expired_object_delete_marker.unwrap_or(false),
                });

                let transitions: Vec<strix_core::LifecycleTransition> = rule
                    .transitions
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| strix_core::LifecycleTransition {
                        days: t.days.map(|d| d as u32),
                        date: None,
                        storage_class: t
                            .storage_class
                            .map(|sc| match sc.as_str() {
                                "GLACIER" => strix_core::StorageClass::Glacier,
                                "DEEP_ARCHIVE" => strix_core::StorageClass::DeepArchive,
                                _ => strix_core::StorageClass::Standard,
                            })
                            .unwrap_or(strix_core::StorageClass::Standard),
                    })
                    .collect();

                let noncurrent_version_expiration =
                    rule.noncurrent_version_expiration.and_then(|nve| {
                        nve.noncurrent_days
                            .map(|days| strix_core::NoncurrentVersionExpiration {
                                noncurrent_days: days as u32,
                                newer_noncurrent_versions: nve
                                    .newer_noncurrent_versions
                                    .map(|v| v as u32),
                            })
                    });

                let abort_incomplete_multipart_upload = rule
                    .abort_incomplete_multipart_upload
                    .and_then(|abort| abort.days_after_initiation)
                    .map(|days| strix_core::AbortIncompleteMultipartUpload {
                        days_after_initiation: days as u32,
                    });

                let enabled = rule.status.as_str() == "Enabled";

                strix_core::LifecycleRule {
                    id: rule.id.unwrap_or_default(),
                    enabled,
                    prefix,
                    tags: Vec::new(),
                    expiration,
                    transitions,
                    noncurrent_version_expiration,
                    abort_incomplete_multipart_upload,
                }
            })
            .collect();

        let config = strix_core::LifecycleConfiguration { rules };

        self.store
            .put_bucket_lifecycle(&input.bucket, config)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(PutBucketLifecycleConfigurationOutput {
            transition_default_minimum_object_size: None,
        }))
    }

    #[instrument(skip(self, req))]
    async fn delete_bucket_lifecycle(
        &self,
        req: S3Request<DeleteBucketLifecycleInput>,
    ) -> S3Result<S3Response<DeleteBucketLifecycleOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "DeleteBucketLifecycle",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        self.store
            .delete_bucket_lifecycle(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(DeleteBucketLifecycleOutput {}))
    }

    // === Object Lock Operations ===

    #[instrument(skip(self, req))]
    async fn get_object_lock_configuration(
        &self,
        req: S3Request<GetObjectLockConfigurationInput>,
    ) -> S3Result<S3Response<GetObjectLockConfigurationOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetObjectLockConfiguration",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let config = self
            .store
            .get_object_lock_configuration(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        match config {
            Some(lock_config) => {
                let rule = lock_config
                    .rule
                    .and_then(|r| r.default_retention)
                    .map(|dr| ObjectLockRule {
                        default_retention: Some(DefaultRetention {
                            mode: Some(ObjectLockRetentionMode::from(dr.mode.to_string())),
                            days: dr.days.map(|d| d as i32),
                            years: dr.years.map(|y| y as i32),
                        }),
                    });

                Ok(S3Response::new(GetObjectLockConfigurationOutput {
                    object_lock_configuration: Some(ObjectLockConfiguration {
                        object_lock_enabled: Some(ObjectLockEnabled::from("Enabled".to_string())),
                        rule,
                    }),
                }))
            }
            None => Err(s3_error!(
                ObjectLockConfigurationNotFoundError,
                "Object lock configuration not found"
            )),
        }
    }

    #[instrument(skip(self, req))]
    async fn put_object_lock_configuration(
        &self,
        req: S3Request<PutObjectLockConfigurationInput>,
    ) -> S3Result<S3Response<PutObjectLockConfigurationOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutObjectLockConfiguration",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        let input = req.input;

        let rule = input
            .object_lock_configuration
            .and_then(|c| c.rule)
            .and_then(|r| r.default_retention)
            .map(|dr| {
                let mode = dr
                    .mode
                    .map(|m| match m.as_str() {
                        "COMPLIANCE" => strix_core::RetentionMode::Compliance,
                        _ => strix_core::RetentionMode::Governance,
                    })
                    .unwrap_or(strix_core::RetentionMode::Governance);

                strix_core::ObjectLockRule {
                    default_retention: Some(strix_core::DefaultRetention {
                        mode,
                        days: dr.days.map(|d| d as u32),
                        years: dr.years.map(|y| y as u32),
                    }),
                }
            });

        let config = strix_core::ObjectLockConfiguration {
            enabled: true,
            rule,
        };

        self.store
            .put_object_lock_configuration(&input.bucket, config)
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(PutObjectLockConfigurationOutput {
            request_charged: None,
        }))
    }

    #[instrument(skip(self, req))]
    async fn get_object_retention(
        &self,
        req: S3Request<GetObjectRetentionInput>,
    ) -> S3Result<S3Response<GetObjectRetentionOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetObjectRetention",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let retention = self
            .store
            .get_object_retention(
                &req.input.bucket,
                &req.input.key,
                req.input.version_id.as_deref(),
            )
            .await
            .map_err(to_s3_error)?;

        match retention {
            Some(ret) => {
                let mode = ObjectLockRetentionMode::from(ret.mode.to_string());
                let retain_until_date = to_timestamp(ret.retain_until_date);

                Ok(S3Response::new(GetObjectRetentionOutput {
                    retention: Some(ObjectLockRetention {
                        mode: Some(mode),
                        retain_until_date: Some(retain_until_date),
                    }),
                }))
            }
            None => Err(s3_error!(NoSuchKey, "Object retention not found")),
        }
    }

    #[instrument(skip(self, req))]
    async fn put_object_retention(
        &self,
        req: S3Request<PutObjectRetentionInput>,
    ) -> S3Result<S3Response<PutObjectRetentionOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutObjectRetention",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;
        let bypass_governance = input.bypass_governance_retention.unwrap_or(false);

        if let Some(r) = input.retention {
            let mode = r
                .mode
                .map(|m| match m.as_str() {
                    "COMPLIANCE" => strix_core::RetentionMode::Compliance,
                    _ => strix_core::RetentionMode::Governance,
                })
                .unwrap_or(strix_core::RetentionMode::Governance);

            // Convert s3s Timestamp to chrono DateTime
            let retain_until_date = r
                .retain_until_date
                .and_then(|ts| {
                    let odt: time::OffsetDateTime = ts.into();
                    chrono::DateTime::from_timestamp(odt.unix_timestamp(), 0)
                })
                .unwrap_or_else(Utc::now);

            let retention = strix_core::ObjectRetention {
                mode,
                retain_until_date,
            };

            self.store
                .put_object_retention(
                    &input.bucket,
                    &input.key,
                    input.version_id.as_deref(),
                    retention,
                    bypass_governance,
                )
                .await
                .map_err(to_s3_error)?;
        }

        Ok(S3Response::new(PutObjectRetentionOutput {
            request_charged: None,
        }))
    }

    #[instrument(skip(self, req))]
    async fn get_object_legal_hold(
        &self,
        req: S3Request<GetObjectLegalHoldInput>,
    ) -> S3Result<S3Response<GetObjectLegalHoldOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetObjectLegalHold",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let status = self
            .store
            .get_object_legal_hold(
                &req.input.bucket,
                &req.input.key,
                req.input.version_id.as_deref(),
            )
            .await
            .map_err(to_s3_error)?;

        let hold_status = match status {
            strix_core::LegalHoldStatus::On => ObjectLockLegalHoldStatus::from("ON".to_string()),
            strix_core::LegalHoldStatus::Off => ObjectLockLegalHoldStatus::from("OFF".to_string()),
        };

        Ok(S3Response::new(GetObjectLegalHoldOutput {
            legal_hold: Some(ObjectLockLegalHold {
                status: Some(hold_status),
            }),
        }))
    }

    #[instrument(skip(self, req))]
    async fn put_object_legal_hold(
        &self,
        req: S3Request<PutObjectLegalHoldInput>,
    ) -> S3Result<S3Response<PutObjectLegalHoldOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutObjectLegalHold",
            Some(&req.input.bucket),
            Some(&req.input.key),
        )
        .await?;

        let input = req.input;

        let status = input
            .legal_hold
            .and_then(|h| h.status)
            .map(|s| {
                if s.as_str() == "ON" {
                    strix_core::LegalHoldStatus::On
                } else {
                    strix_core::LegalHoldStatus::Off
                }
            })
            .unwrap_or(strix_core::LegalHoldStatus::Off);

        self.store
            .put_object_legal_hold(
                &input.bucket,
                &input.key,
                input.version_id.as_deref(),
                status,
            )
            .await
            .map_err(to_s3_error)?;

        Ok(S3Response::new(PutObjectLegalHoldOutput {
            request_charged: None,
        }))
    }

    // === Notification Operations ===

    #[instrument(skip(self, req))]
    async fn get_bucket_notification_configuration(
        &self,
        req: S3Request<GetBucketNotificationConfigurationInput>,
    ) -> S3Result<S3Response<GetBucketNotificationConfigurationOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "GetBucketNotificationConfiguration",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        // Get notification config from store (returns empty config if not set)
        let _config = self
            .store
            .get_bucket_notification(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        // Return empty notification configuration (notifications are internal)
        Ok(S3Response::new(GetBucketNotificationConfigurationOutput {
            lambda_function_configurations: None,
            queue_configurations: None,
            topic_configurations: None,
            event_bridge_configuration: None,
        }))
    }

    #[instrument(skip(self, req))]
    async fn put_bucket_notification_configuration(
        &self,
        req: S3Request<PutBucketNotificationConfigurationInput>,
    ) -> S3Result<S3Response<PutBucketNotificationConfigurationOutput>> {
        // Authorization check
        self.check_authorization(
            &req.credentials,
            &req.headers,
            "PutBucketNotificationConfiguration",
            Some(&req.input.bucket),
            None,
        )
        .await?;

        // Verify bucket exists
        let _ = self
            .store
            .head_bucket(&req.input.bucket)
            .await
            .map_err(to_s3_error)?;

        // Accept but don't fully implement S3 notification configuration
        // Our internal notification system uses different destinations
        Ok(S3Response::new(PutBucketNotificationConfigurationOutput {}))
    }
}
