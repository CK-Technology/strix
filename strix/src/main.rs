//! Strix - S3-compatible object storage server.
//!
//! A modern, Rust-based MinIO alternative.

mod metrics;
mod telemetry;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use base64::Engine;
use clap::Parser;
use futures::stream::StreamExt;
use rust_embed::Embed;
use s3s::service::S3ServiceBuilder;
use serde::Serialize;
use strix_core::ObjectStore;
use strix_s3::RequestAuditContext;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::broadcast;
use tokio::time::Duration;
use tokio_rusqlite::Connection;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use metrics_exporter_prometheus::PrometheusHandle;
use strix_admin::{AdminState, CsrfConfig, PresignConfig, RateLimiter, admin_router};
use strix_iam::{IamProvider, IamStore};
use strix_s3::{IamAuth, S3BodyStream, SimpleAuthProvider, StrixS3Service};
use strix_storage::{CleanupConfig, LocalFsStore, start_cleanup_task};

/// State for health check endpoints.
#[derive(Clone)]
struct HealthState {
    storage: Arc<LocalFsStore>,
    iam: Arc<IamStore>,
}

/// Health check response.
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// State for S3 rate limiting.
#[derive(Clone)]
struct S3RateLimitState {
    rate_limiter: Arc<RateLimiter>,
}

/// State for metrics endpoint.
#[derive(Clone)]
struct MetricsState {
    handle: PrometheusHandle,
}

/// Rate limiting middleware for S3 API.
///
/// Limits requests per IP to prevent abuse. Uses a configurable rate limiter
/// that allows a burst of requests with a lockout after exceeding the limit.
async fn s3_rate_limit_middleware(
    State(state): State<S3RateLimitState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let ip = addr.ip();

    if state.rate_limiter.is_limited(&ip) {
        let retry_after = state.rate_limiter.lockout_remaining(&ip).unwrap_or(60);
        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Retry-After", retry_after.to_string())
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>SlowDown</Code>
    <Message>Please reduce your request rate.</Message>
    <RetryAfterSeconds>{}</RetryAfterSeconds>
</Error>"#,
                retry_after
            )))
            .unwrap();
    }

    next.run(request).await
}

/// Embedded GUI assets (built by Trunk from strix-gui crate).
/// If the dist folder is empty or missing, the placeholder HTML will be served.
#[derive(Embed)]
#[folder = "../crates/strix-gui/dist"]
struct GuiAssets;

/// Strix - S3-compatible object storage server
#[derive(Parser, Debug)]
#[command(name = "strix", version, about, long_about = None)]
struct Config {
    /// S3 API address
    #[arg(long, default_value = "0.0.0.0:9000", env = "STRIX_ADDRESS")]
    address: SocketAddr,

    /// Admin/Console address
    #[arg(long, default_value = "0.0.0.0:9001", env = "STRIX_CONSOLE_ADDRESS")]
    console_address: SocketAddr,

    /// Metrics address
    #[arg(long, default_value = "127.0.0.1:9090", env = "STRIX_METRICS_ADDRESS")]
    metrics_address: SocketAddr,

    /// Data directory
    #[arg(long, default_value = "/var/lib/strix", env = "STRIX_DATA_DIR")]
    data_dir: PathBuf,

    /// Root access key
    #[arg(long, env = "STRIX_ROOT_USER")]
    root_user: String,

    /// Root secret key
    #[arg(long, env = "STRIX_ROOT_PASSWORD")]
    root_password: String,

    /// Log level
    #[arg(long, default_value = "info", env = "STRIX_LOG_LEVEL")]
    log_level: String,

    /// Multipart upload expiry hours (for cleanup)
    #[arg(long, default_value = "24", env = "STRIX_MULTIPART_EXPIRY_HOURS")]
    multipart_expiry_hours: u32,

    /// S3 API rate limit (max requests per minute per IP, 0 = disabled)
    #[arg(long, default_value = "1000", env = "STRIX_S3_RATE_LIMIT")]
    s3_rate_limit: u32,

    /// Enable JSON log format for production environments
    #[arg(long, env = "STRIX_LOG_JSON")]
    log_json: bool,

    /// OpenTelemetry OTLP endpoint (e.g., http://localhost:4317)
    #[cfg(feature = "otel")]
    #[arg(long, env = "STRIX_OTLP_ENDPOINT")]
    otlp_endpoint: Option<String>,

    /// Service name for OpenTelemetry traces
    #[cfg(feature = "otel")]
    #[arg(long, default_value = "strix", env = "STRIX_SERVICE_NAME")]
    service_name: String,

    /// Stable JWT signing secret (base64, 32+ bytes decoded). If unset, random per-process secret is used.
    #[arg(long, env = "STRIX_JWT_SECRET")]
    jwt_secret: Option<String>,
}

impl Config {
    /// Validate the configuration and return errors with helpful messages.
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate root credentials
        if self.root_user.is_empty() {
            errors.push(
                "Root user not set. Set STRIX_ROOT_USER environment variable or use --root-user flag.".to_string()
            );
        } else if self.root_user.len() < 3 {
            errors.push("Root user must be at least 3 characters long.".to_string());
        }

        if self.root_password.is_empty() {
            errors.push(
                "Root password not set. Set STRIX_ROOT_PASSWORD environment variable or use --root-password flag.".to_string()
            );
        } else if self.root_password.len() < 8 {
            errors
                .push("Root password must be at least 8 characters long for security.".to_string());
        }

        // Check for duplicate ports
        let ports = [
            (self.address, "S3 API"),
            (self.console_address, "Admin/Console"),
            (self.metrics_address, "Metrics"),
        ];
        for i in 0..ports.len() {
            for j in (i + 1)..ports.len() {
                if ports[i].0.port() == ports[j].0.port() {
                    errors.push(format!(
                        "{} and {} cannot use the same port {}.",
                        ports[i].1,
                        ports[j].1,
                        ports[i].0.port()
                    ));
                }
            }
        }

        // Validate multipart expiry
        if self.multipart_expiry_hours == 0 {
            errors.push("Multipart expiry hours must be greater than 0.".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn jwt_secret_bytes(&self) -> anyhow::Result<Option<[u8; 32]>> {
        let Some(secret) = &self.jwt_secret else {
            return Ok(None);
        };

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(secret)
            .map_err(|e| anyhow::anyhow!("Invalid STRIX_JWT_SECRET base64: {}", e))?;

        if decoded.len() < 32 {
            anyhow::bail!(
                "STRIX_JWT_SECRET must decode to at least 32 bytes (got {})",
                decoded.len()
            );
        }

        let mut out = [0u8; 32];
        out.copy_from_slice(&decoded[..32]);
        Ok(Some(out))
    }

    /// Ensure the data directory exists, creating it if necessary.
    fn ensure_data_dir(&self) -> anyhow::Result<()> {
        if !self.data_dir.exists() {
            std::fs::create_dir_all(&self.data_dir).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to create data directory '{}': {}. \
                    Ensure the parent directory exists and you have write permissions.",
                    self.data_dir.display(),
                    e
                )
            })?;
            info!("Created data directory: {}", self.data_dir.display());
        }

        // Verify it's actually a directory
        if !self.data_dir.is_dir() {
            anyhow::bail!(
                "Data path '{}' exists but is not a directory.",
                self.data_dir.display()
            );
        }

        // Check write permissions by trying to create a temp file
        let test_file = self.data_dir.join(".strix-write-test");
        std::fs::write(&test_file, b"test").map_err(|e| {
            anyhow::anyhow!(
                "Data directory '{}' is not writable: {}",
                self.data_dir.display(),
                e
            )
        })?;
        let _ = std::fs::remove_file(&test_file);

        Ok(())
    }
}

/// Wait for shutdown signal (SIGTERM or SIGINT/Ctrl+C).
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, starting graceful shutdown...");
        }
        _ = terminate => {
            info!("Received SIGTERM, starting graceful shutdown...");
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();

    // Validate configuration before proceeding
    if let Err(errors) = config.validate() {
        eprintln!("Configuration errors:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        eprintln!("\nRun with --help for usage information.");
        std::process::exit(1);
    }

    // Initialize telemetry (logging + optional OpenTelemetry)
    let telemetry_config = telemetry::TelemetryConfig {
        log_level: config.log_level.clone(),
        log_json: config.log_json,
        #[cfg(feature = "otel")]
        otlp_endpoint: config.otlp_endpoint.clone(),
        #[cfg(feature = "otel")]
        service_name: config.service_name.clone(),
    };
    let _telemetry_guard = telemetry::init_telemetry(&telemetry_config)?;

    // Initialize Prometheus metrics
    let metrics_handle = metrics::init_metrics();
    metrics::set_server_info(env!("CARGO_PKG_VERSION"));

    info!("Starting Strix v{}", env!("CARGO_PKG_VERSION"));
    info!("Data directory: {}", config.data_dir.display());

    // Ensure data directory exists and is writable
    config.ensure_data_dir()?;

    // Initialize storage
    let store = Arc::new(LocalFsStore::new(&config.data_dir).await?);
    info!("Storage initialized");

    // Initialize IAM database (separate from storage metadata)
    let iam_db_path = config.data_dir.join("meta").join("iam.db");
    tokio::fs::create_dir_all(config.data_dir.join("meta")).await?;
    let iam_db = Connection::open(&iam_db_path).await?;
    let iam_store = Arc::new(
        IamStore::new(
            iam_db,
            config.root_user.clone(),
            config.root_password.clone(),
        )
        .await?,
    );
    info!("IAM initialized");

    // Start background cleanup task for stale multipart uploads
    let cleanup_config = CleanupConfig::new(
        Duration::from_secs(3600),
        Duration::from_secs((config.multipart_expiry_hours as u64) * 3600),
    );
    let _cleanup_handle = start_cleanup_task(store.clone(), cleanup_config);
    info!("Background cleanup task started");

    // Initialize auth provider - uses IAM store for credential lookup
    let auth_provider = Arc::new(SimpleAuthProvider::new(
        config.root_user.clone(),
        config.root_password.clone(),
    ));

    // Create IAM-based S3 authentication
    let iam_auth = IamAuth::new(
        iam_store.clone(),
        config.root_user.clone(),
        config.root_password.clone(),
    );

    // Create S3 service with IAM provider for policy enforcement
    let s3_service = StrixS3Service::with_iam(
        store.clone(),
        auth_provider,
        iam_store.clone(),
        config.root_user.clone(),
    );

    // Build s3s service with IAM authentication
    let s3s_service = {
        let mut builder = S3ServiceBuilder::new(s3_service);
        builder.set_auth(iam_auth);
        builder.build()
    };

    // Create a shared s3s service
    let s3s_service = Arc::new(s3s_service);

    // Health check state
    let health_state = HealthState {
        storage: store.clone(),
        iam: iam_store.clone(),
    };

    // S3 API rate limiter (configurable, 0 = disabled)
    let s3_rate_limiter = if config.s3_rate_limit > 0 {
        // Rate limit: max requests per minute, 30 second lockout
        Some(Arc::new(RateLimiter::new(
            config.s3_rate_limit,
            Duration::from_secs(60),
            Duration::from_secs(30),
        )))
    } else {
        None
    };

    // S3 API router with custom handler
    let s3_app = {
        let s3s_service = s3s_service.clone();
        // Health routes with state
        // Native Strix endpoints + MinIO-compatible endpoints for tool compatibility
        let health_routes = Router::new()
            .route("/health/live", get(health_live))
            .route("/health/ready", get(health_ready))
            .route("/minio/health/live", get(health_live))
            .route("/minio/health/ready", get(health_ready))
            .with_state(health_state.clone());

        // S3 fallback handler
        let s3_fallback = move |req: Request<Body>| {
            let service = s3s_service.clone();
            async move {
                // Convert axum request to s3s request
                let (parts, body) = req.into_parts();

                let source_ip = parts
                    .headers
                    .get("x-forwarded-for")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|v| v.split(',').map(str::trim).find(|s| !s.is_empty()))
                    .map(ToString::to_string)
                    .or_else(|| {
                        parts
                            .headers
                            .get("x-real-ip")
                            .and_then(|h| h.to_str().ok())
                            .map(ToString::to_string)
                    });

                let request_id = parts
                    .headers
                    .get("x-amz-request-id")
                    .and_then(|h| h.to_str().ok())
                    .map(ToString::to_string)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                let content_length = parts
                    .headers
                    .get(header::CONTENT_LENGTH)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);

                let stream = body
                    .into_data_stream()
                    .map(|result| result.map_err(|e| std::io::Error::other(e.to_string())));

                let s3s_body = S3BodyStream::new(Box::pin(stream), content_length).into_s3s_body();
                let mut s3s_req = hyper::Request::from_parts(parts, s3s_body);
                s3s_req.extensions_mut().insert(RequestAuditContext {
                    source_ip,
                    request_id,
                });

                // Call s3s service
                match service.call(s3s_req).await {
                    Ok(response) => {
                        let (parts, body) = response.into_parts();
                        let axum_body = Body::new(body);
                        Ok::<_, std::convert::Infallible>(axum::response::Response::from_parts(
                            parts, axum_body,
                        ))
                    }
                    Err(_) => Ok::<_, std::convert::Infallible>(
                        axum::response::Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::from("Internal Server Error"))
                            .unwrap(),
                    ),
                }
            }
        };

        // Build router with optional rate limiting
        let router = health_routes.fallback(s3_fallback);

        if let Some(rate_limiter) = s3_rate_limiter {
            let rate_limit_state = S3RateLimitState { rate_limiter };
            router
                .layer(middleware::from_fn_with_state(
                    rate_limit_state,
                    s3_rate_limit_middleware,
                ))
                .layer(TraceLayer::new_for_http())
        } else {
            router.layer(TraceLayer::new_for_http())
        }
    };

    // Admin/Console router with embedded GUI
    let presign_config = PresignConfig {
        access_key: config.root_user.clone(),
        secret_key: config.root_password.clone(),
        endpoint: format!("http://{}", config.address),
        region: "us-east-1".to_string(),
    };

    // Configure CSRF protection with the console address
    let csrf_config = CsrfConfig::new(vec![
        format!("http://{}", config.console_address),
        format!("https://{}", config.console_address),
        // Also allow common localhost addresses
        "http://localhost:9001".to_string(),
        "http://127.0.0.1:9001".to_string(),
    ]);

    let admin_auth = match config.jwt_secret_bytes()? {
        Some(secret) => {
            info!("Using configured stable JWT signing secret");
            strix_admin::AuthState::with_secret(secret, Duration::from_secs(3600))
        }
        None => {
            warn!("No STRIX_JWT_SECRET configured; admin sessions will reset on process restart");
            strix_admin::AuthState::new()
        }
    };

    let admin_state = Arc::new(
        AdminState::new(iam_store, store)
            .with_presign(presign_config)
            .with_auth(admin_auth)
            .with_csrf(csrf_config),
    );
    let admin_app = Router::new()
        .nest("/api/v1", admin_router(admin_state))
        .fallback(serve_gui)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    // Metrics router
    let metrics_state = MetricsState {
        handle: metrics_handle,
    };
    let metrics_app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(metrics_state)
        .layer(TraceLayer::new_for_http());

    // Start servers
    let s3_listener = TcpListener::bind(&config.address).await?;
    let admin_listener = TcpListener::bind(&config.console_address).await?;
    let metrics_listener = TcpListener::bind(&config.metrics_address).await?;

    info!("S3 API listening on {}", config.address);
    info!("Admin/Console listening on {}", config.console_address);
    info!("Metrics listening on {}", config.metrics_address);

    // Create a broadcast channel for shutdown signal
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // Clone shutdown receivers for each server
    let mut shutdown_rx1 = shutdown_tx.subscribe();
    let mut shutdown_rx2 = shutdown_tx.subscribe();
    let mut shutdown_rx3 = shutdown_tx.subscribe();

    // Run all servers concurrently with graceful shutdown
    // Both S3 and Admin servers use into_make_service_with_connect_info to enable IP-based rate limiting
    tokio::select! {
        result = axum::serve(s3_listener, s3_app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(async move { let _ = shutdown_rx1.recv().await; }) => {
            if let Err(e) = result {
                warn!("S3 server error: {}", e);
            }
        }
        result = axum::serve(admin_listener, admin_app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(async move { let _ = shutdown_rx2.recv().await; }) => {
            if let Err(e) = result {
                warn!("Admin server error: {}", e);
            }
        }
        result = axum::serve(metrics_listener, metrics_app)
            .with_graceful_shutdown(async move { let _ = shutdown_rx3.recv().await; }) => {
            if let Err(e) = result {
                warn!("Metrics server error: {}", e);
            }
        }
        _ = shutdown_signal() => {
            info!("Initiating graceful shutdown...");
            // Signal all servers to shut down
            let _ = shutdown_tx.send(());
            // Give servers time to drain connections
            tokio::time::sleep(Duration::from_secs(5)).await;
            info!("Shutdown complete");
        }
    }

    Ok(())
}

/// Serve embedded GUI assets.
async fn serve_gui(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the exact file
    if let Some(content) = <GuiAssets as Embed>::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content.data.into_owned()))
            .unwrap();
    }

    // For SPA routing, serve index.html for non-file paths
    if let Some(content) = <GuiAssets as Embed>::get("index.html") {
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(content.data.into_owned()))
            .unwrap();
    }

    // Fallback placeholder if GUI not built
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html")
        .body(Body::from(PLACEHOLDER_HTML))
        .unwrap()
}

/// Placeholder HTML when GUI is not built.
const PLACEHOLDER_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Strix Console</title>
    <script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="bg-gray-100 min-h-screen">
    <div class="flex items-center justify-center min-h-screen">
        <div class="text-center max-w-lg mx-auto p-8">
            <svg class="h-16 w-16 text-indigo-600 mx-auto mb-4" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/>
            </svg>
            <h1 class="text-3xl font-bold text-gray-900 mb-4">Strix Console</h1>
            <p class="text-gray-600 mb-8">
                The web console is not built yet. Build it with:
            </p>
            <pre class="bg-gray-800 text-gray-100 p-4 rounded-md text-left text-sm overflow-x-auto mb-8">
cd crates/strix-gui
trunk build --release</pre>
            <div class="bg-white p-6 rounded-lg shadow-sm">
                <h2 class="text-lg font-semibold text-gray-900 mb-4">Admin API Available</h2>
                <ul class="text-left text-sm text-gray-600 space-y-2">
                    <li><code class="bg-gray-100 px-2 py-1 rounded">GET /api/v1/info</code> - Server info</li>
                    <li><code class="bg-gray-100 px-2 py-1 rounded">GET /api/v1/users</code> - List users</li>
                    <li><code class="bg-gray-100 px-2 py-1 rounded">GET /api/v1/usage</code> - Storage usage</li>
                </ul>
            </div>
        </div>
    </div>
</body>
</html>"#;

/// Health check - liveness probe.
///
/// Returns 200 OK if the process is alive. This is a lightweight check
/// used by orchestrators to determine if the process should be restarted.
async fn health_live() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok",
        storage: None,
        database: None,
        error: None,
    })
}

/// Health check - readiness probe.
///
/// Returns 200 OK if the service is ready to accept traffic.
/// Verifies both storage and database connectivity.
async fn health_ready(State(state): State<HealthState>) -> impl IntoResponse {
    // Check storage by listing buckets (lightweight DB query)
    let storage_ok = state.storage.list_buckets().await.is_ok();

    // Check IAM database by listing users
    let db_ok = state.iam.list_users().await.is_ok();

    if storage_ok && db_ok {
        (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ok",
                storage: Some("ok"),
                database: Some("ok"),
                error: None,
            }),
        )
    } else {
        let mut errors = Vec::new();
        if !storage_ok {
            errors.push("storage unavailable");
        }
        if !db_ok {
            errors.push("database unavailable");
        }

        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "error",
                storage: Some(if storage_ok { "ok" } else { "error" }),
                database: Some(if db_ok { "ok" } else { "error" }),
                error: Some(errors.join(", ")),
            }),
        )
    }
}

/// Prometheus metrics endpoint.
///
/// Renders all registered metrics in Prometheus text format.
async fn metrics_handler(State(state): State<MetricsState>) -> impl IntoResponse {
    let metrics = state.handle.render();
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        metrics,
    )
}
