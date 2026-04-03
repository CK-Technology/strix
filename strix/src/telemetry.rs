//! Telemetry and tracing configuration.
//!
//! Provides structured logging with optional OpenTelemetry integration.

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Telemetry configuration.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Log level filter (e.g., "info", "debug", "strix=debug,tower_http=info").
    pub log_level: String,
    /// Enable JSON log output format.
    pub log_json: bool,
    /// OpenTelemetry endpoint (e.g., "http://localhost:4317").
    #[cfg(feature = "otel")]
    pub otlp_endpoint: Option<String>,
    /// Service name for OpenTelemetry.
    #[cfg(feature = "otel")]
    pub service_name: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            log_json: false,
            #[cfg(feature = "otel")]
            otlp_endpoint: None,
            #[cfg(feature = "otel")]
            service_name: "strix".to_string(),
        }
    }
}

/// Initialize telemetry with the given configuration.
///
/// Sets up structured logging with optional JSON output and OpenTelemetry.
pub fn init_telemetry(config: &TelemetryConfig) -> anyhow::Result<TelemetryGuard> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    #[cfg(feature = "otel")]
    {
        if let Some(endpoint) = &config.otlp_endpoint {
            return init_with_otel(config, env_filter, endpoint);
        }
    }

    // Standard logging without OpenTelemetry
    if config.log_json {
        tracing_subscriber::registry()
            .with(fmt::layer().json().with_current_span(true))
            .with(env_filter)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(env_filter)
            .init();
    }

    Ok(TelemetryGuard::default())
}

/// Initialize telemetry with OpenTelemetry support.
#[cfg(feature = "otel")]
fn init_with_otel(
    config: &TelemetryConfig,
    env_filter: EnvFilter,
    endpoint: &str,
) -> anyhow::Result<TelemetryGuard> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};
    use tracing_opentelemetry::OpenTelemetryLayer;

    // Create OTLP exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create OTLP exporter: {}", e))?;

    // Build resource with service name
    let resource = Resource::builder()
        .with_service_name(config.service_name.clone())
        .build();

    // Create tracer provider
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer("strix");

    // Build subscriber with both fmt and otel layers
    if config.log_json {
        tracing_subscriber::registry()
            .with(fmt::layer().json().with_current_span(true))
            .with(OpenTelemetryLayer::new(tracer))
            .with(env_filter)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(OpenTelemetryLayer::new(tracer))
            .with(env_filter)
            .init();
    }

    tracing::info!(
        endpoint = %endpoint,
        service = %config.service_name,
        "OpenTelemetry tracing enabled"
    );

    Ok(TelemetryGuard {
        #[cfg(feature = "otel")]
        provider: Some(provider),
    })
}

/// Guard that shuts down telemetry when dropped.
#[derive(Default)]
pub struct TelemetryGuard {
    #[cfg(feature = "otel")]
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        if let Some(provider) = self.provider.take() {
            if let Err(e) = provider.shutdown() {
                eprintln!("Error shutting down OpenTelemetry provider: {:?}", e);
            }
        }
    }
}
