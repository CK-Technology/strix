//! SMTP configuration endpoints for the admin/console API (root-only).
//!
//! Reads and writes the single SMTP relay configuration and triggers a test
//! email. The SMTP password is write-only: it is accepted on write but never
//! returned on read.

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use strix_iam::{SmtpConfig, UsageReportSchedule};

use crate::auth::AuthenticatedUser;
use crate::handlers::AdminState;
use crate::{ErrorResponse, SendTestEmailRequest, SmtpConfigRequest, SmtpConfigResponse};

fn forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new("Root privileges required")),
    )
        .into_response()
}

fn config_to_response(config: SmtpConfig) -> SmtpConfigResponse {
    SmtpConfigResponse {
        has_password: !config.password.is_empty(),
        enabled: config.enabled,
        host: config.host,
        port: config.port,
        username: config.username,
        from_address: config.from_address,
        from_name: config.from_name,
        use_starttls: config.use_starttls,
        alert_on_delivery_failure: config.alert_on_delivery_failure,
        send_usage_reports: config.send_usage_reports,
        usage_report_schedule: config.usage_report_schedule.as_str().to_string(),
        alert_on_audit_events: config.alert_on_audit_events,
        alert_recipients: config.alert_recipients,
    }
}

/// `GET /admin/smtp` — return the SMTP configuration (root-only, no password).
pub async fn get_smtp_config_admin(
    State(state): State<Arc<AdminState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Response {
    if !user.is_root {
        return forbidden();
    }
    // Use the with-secret variant only to compute `has_password`; the secret
    // itself is dropped by `config_to_response`.
    match state.iam.get_smtp_config_with_secret().await {
        Ok(Some(config)) => Json(config_to_response(config)).into_response(),
        Ok(None) => Json(config_to_response(SmtpConfig::default())).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

/// `PUT /admin/smtp` — create or update the SMTP configuration (root-only).
pub async fn set_smtp_config_admin(
    State(state): State<Arc<AdminState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<SmtpConfigRequest>,
) -> Response {
    if !user.is_root {
        return forbidden();
    }

    let schedule = req
        .usage_report_schedule
        .parse::<UsageReportSchedule>()
        .unwrap_or_default();

    let config = SmtpConfig {
        enabled: req.enabled,
        host: req.host,
        port: req.port,
        username: req.username,
        password: req.password,
        from_address: req.from_address,
        from_name: req.from_name,
        use_starttls: req.use_starttls,
        alert_on_delivery_failure: req.alert_on_delivery_failure,
        send_usage_reports: req.send_usage_reports,
        usage_report_schedule: schedule,
        alert_on_audit_events: req.alert_on_audit_events,
        alert_recipients: req.alert_recipients,
    };

    match state.iam.set_smtp_config(&config).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

/// `POST /admin/smtp/test` — send a test email with the stored config (root-only).
pub async fn send_test_email_admin(
    State(state): State<Arc<AdminState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<SendTestEmailRequest>,
) -> Response {
    if !user.is_root {
        return forbidden();
    }

    let Some(email) = state.email.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Email service is not available")),
        )
            .into_response();
    };

    match email.send_test(req.to).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}
