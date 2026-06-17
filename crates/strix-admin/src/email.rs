//! SMTP email service for alerts and scheduled reports.
//!
//! Configuration (including the SMTP password, encrypted at rest) lives in the
//! IAM store and is read per-send so changes take effect without a restart.
//! All alert paths are best-effort: a failure to send an alert is logged but
//! never propagated into the operation that triggered it.

use std::sync::Arc;

use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use strix_core::{AuditLogEntry, DeliveryFailureAlerter, NotificationDeliveryAttempt, ObjectStore};
use strix_iam::{IamStore, SmtpConfig};
use strix_storage::LocalFsStore;

/// Errors that can occur while sending mail.
#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    /// SMTP has not been configured.
    #[error("SMTP is not configured")]
    NotConfigured,
    /// SMTP is configured but disabled.
    #[error("SMTP sending is disabled")]
    Disabled,
    /// No recipient could be determined.
    #[error("no recipient specified")]
    NoRecipients,
    /// Failed to read configuration from the store.
    #[error("failed to load SMTP config: {0}")]
    Config(String),
    /// Failed to build the message or transport.
    #[error("failed to build email: {0}")]
    Build(String),
    /// The SMTP transport failed to deliver.
    #[error("SMTP transport error: {0}")]
    Transport(String),
}

/// Sends email via the configured SMTP relay.
#[derive(Clone)]
pub struct EmailService {
    iam: Arc<IamStore>,
    storage: Arc<LocalFsStore>,
}

impl EmailService {
    /// Create a new email service backed by the IAM store (for config) and the
    /// object store (for usage aggregation).
    pub fn new(iam: Arc<IamStore>, storage: Arc<LocalFsStore>) -> Self {
        Self { iam, storage }
    }

    /// Load the active SMTP configuration with the password decrypted.
    ///
    /// Returns an error if SMTP is unconfigured or disabled.
    async fn active_config(&self) -> Result<SmtpConfig, EmailError> {
        let config = self
            .iam
            .get_smtp_config_with_secret()
            .await
            .map_err(|e| EmailError::Config(e.to_string()))?
            .ok_or(EmailError::NotConfigured)?;
        if !config.enabled {
            return Err(EmailError::Disabled);
        }
        Ok(config)
    }

    /// Build an SMTP transport from a configuration.
    fn build_transport(
        config: &SmtpConfig,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, EmailError> {
        let builder = if config.use_starttls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
        }
        .map_err(|e| EmailError::Build(e.to_string()))?;

        let creds = Credentials::new(config.username.clone(), config.password.clone());
        Ok(builder.port(config.port).credentials(creds).build())
    }

    /// Resolve the recipient list for alerts/reports, falling back to the
    /// configured From address when none are set.
    fn recipients<'a>(config: &'a SmtpConfig, override_to: Option<&'a str>) -> Vec<String> {
        if let Some(to) = override_to {
            return vec![to.to_string()];
        }
        if !config.alert_recipients.is_empty() {
            return config.alert_recipients.clone();
        }
        if !config.from_address.is_empty() {
            return vec![config.from_address.clone()];
        }
        Vec::new()
    }

    /// Send a plain-text email to each recipient using the given configuration.
    async fn send_with(
        config: &SmtpConfig,
        recipients: &[String],
        subject: &str,
        body: &str,
    ) -> Result<(), EmailError> {
        if recipients.is_empty() {
            return Err(EmailError::NoRecipients);
        }

        let from_addr = config
            .from_address
            .parse::<Address>()
            .map_err(|e| EmailError::Build(format!("invalid from address: {e}")))?;
        let from = Mailbox::new(config.from_name.clone(), from_addr);

        let transport = Self::build_transport(config)?;

        for recipient in recipients {
            let to = recipient
                .parse::<Mailbox>()
                .map_err(|e| EmailError::Build(format!("invalid recipient '{recipient}': {e}")))?;
            let message = Message::builder()
                .from(from.clone())
                .to(to)
                .subject(subject)
                .header(ContentType::TEXT_PLAIN)
                .body(body.to_string())
                .map_err(|e| EmailError::Build(e.to_string()))?;

            transport
                .send(message)
                .await
                .map_err(|e| EmailError::Transport(e.to_string()))?;
        }
        Ok(())
    }

    /// Send a test email to verify SMTP settings. Uses the stored config; the
    /// optional `to` overrides the recipient (defaults to the From address).
    pub async fn send_test(&self, to: Option<String>) -> Result<(), EmailError> {
        let config = self.active_config().await?;
        let recipients = Self::recipients(&config, to.as_deref());
        let body = format!(
            "This is a test email from Strix.\n\nIf you received this message, your SMTP relay \
             ({}:{}) is configured correctly.",
            config.host, config.port
        );
        Self::send_with(&config, &recipients, "Strix SMTP test", &body).await
    }

    /// The interval at which usage reports should be sent, or `None` when SMTP
    /// is disabled/unconfigured or usage reports are turned off.
    pub async fn usage_report_interval(&self) -> Option<std::time::Duration> {
        match self.active_config().await {
            Ok(c) if c.send_usage_reports => Some(c.usage_report_schedule.interval()),
            _ => None,
        }
    }

    /// Build and send a storage usage report to the configured recipients.
    pub async fn send_usage_report(&self) -> Result<(), EmailError> {
        let config = self.active_config().await?;
        if !config.send_usage_reports {
            return Err(EmailError::Disabled);
        }

        let body = self.build_usage_report().await?;
        let recipients = Self::recipients(&config, None);
        Self::send_with(&config, &recipients, "Strix storage usage report", &body).await
    }

    /// Aggregate per-bucket usage into a plain-text report body.
    async fn build_usage_report(&self) -> Result<String, EmailError> {
        let buckets = self
            .storage
            .list_buckets()
            .await
            .map_err(|e| EmailError::Build(e.to_string()))?;

        let mut lines = vec![
            "Strix storage usage report".to_string(),
            format!("Generated: {}", chrono::Utc::now().to_rfc3339()),
            String::new(),
            format!("{:<32} {:>14} {:>16}", "Bucket", "Objects", "Size (bytes)"),
            "-".repeat(64),
        ];

        let mut total_objects: u64 = 0;
        let mut total_size: u64 = 0;
        for bucket in &buckets {
            let (count, size) = self
                .storage
                .get_bucket_usage(&bucket.name)
                .await
                .unwrap_or((0, 0));
            total_objects += count;
            total_size += size;
            lines.push(format!("{:<32} {:>14} {:>16}", bucket.name, count, size));
        }

        lines.push("-".repeat(64));
        lines.push(format!(
            "{:<32} {:>14} {:>16}",
            format!("TOTAL ({} buckets)", buckets.len()),
            total_objects,
            total_size
        ));

        Ok(lines.join("\n"))
    }

    /// Best-effort security alert for a privileged or denied admin operation.
    ///
    /// Sends nothing when SMTP is disabled, audit alerts are off, or the event
    /// is not security-relevant.
    pub async fn maybe_alert_audit(&self, entry: &AuditLogEntry) {
        if !Self::audit_is_security_relevant(entry) {
            return;
        }
        let config = match self.active_config().await {
            Ok(c) if c.alert_on_audit_events => c,
            _ => return,
        };

        let body = format!(
            "A security-relevant event occurred on Strix.\n\n\
             Operation: {}\n\
             Principal: {}\n\
             Source IP: {}\n\
             Status:    {}\n\
             Time:      {}\n\
             Request:   {}",
            entry.operation,
            entry.principal.as_deref().unwrap_or("(unauthenticated)"),
            entry.source_ip.as_deref().unwrap_or("(unknown)"),
            entry.status_code,
            entry.timestamp.to_rfc3339(),
            entry.request_id,
        );
        let recipients = Self::recipients(&config, None);
        if let Err(e) = Self::send_with(&config, &recipients, "Strix security alert", &body).await {
            tracing::warn!("Failed to send security alert email: {}", e);
        }
    }

    /// Whether an admin audit entry warrants a security alert.
    ///
    /// Flags denied requests (401/403) and privileged mutations to identity,
    /// policy, key, provider, or SMTP configuration.
    fn audit_is_security_relevant(entry: &AuditLogEntry) -> bool {
        if entry.status_code == 401 || entry.status_code == 403 {
            return true;
        }
        let op = entry.operation.as_str();
        let is_mutation = op.contains("POST ") || op.contains("PUT ") || op.contains("DELETE ");
        if !is_mutation {
            return false;
        }
        [
            "/users",
            "/groups",
            "/policies",
            "/access-keys",
            "/oidc",
            "/smtp",
        ]
        .iter()
        .any(|seg| op.contains(seg))
    }
}

#[async_trait]
impl DeliveryFailureAlerter for EmailService {
    async fn alert_delivery_failure(&self, attempt: &NotificationDeliveryAttempt) {
        let config = match self.active_config().await {
            Ok(c) if c.alert_on_delivery_failure => c,
            _ => return,
        };

        let body = format!(
            "A notification delivery failed on Strix.\n\n\
             Bucket:      {}\n\
             Rule:        {}\n\
             Destination: {} ({})\n\
             Event:       {}\n\
             Object:      {}\n\
             Attempts:    {}\n\
             Status:      {}\n\
             HTTP code:   {}\n\
             Last error:  {}\n\
             Time:        {}",
            attempt.bucket,
            attempt.rule_id,
            attempt.target,
            attempt.destination_type,
            attempt.event_type,
            attempt.object_key,
            attempt.attempts,
            attempt.status,
            attempt
                .response_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "-".to_string()),
            attempt.last_error.as_deref().unwrap_or("(none)"),
            attempt.timestamp.to_rfc3339(),
        );
        let recipients = Self::recipients(&config, None);
        if let Err(e) = Self::send_with(
            &config,
            &recipients,
            "Strix notification delivery failure",
            &body,
        )
        .await
        {
            tracing::warn!("Failed to send delivery-failure alert email: {}", e);
        }
    }
}
