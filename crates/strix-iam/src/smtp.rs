//! SMTP relay configuration for outbound email (alerts and reports).
//!
//! A single configuration row is persisted in the IAM database with the SMTP
//! password encrypted at rest. The password is write-only across the admin API
//! (never returned in responses) using the same semantics as OIDC client
//! secrets.

use serde::{Deserialize, Serialize};

/// How often scheduled usage reports are emailed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UsageReportSchedule {
    /// Send a usage digest once per day.
    Daily,
    /// Send a usage digest once per week.
    #[default]
    Weekly,
}

impl UsageReportSchedule {
    /// Interval between reports.
    pub fn interval(self) -> std::time::Duration {
        match self {
            UsageReportSchedule::Daily => std::time::Duration::from_secs(24 * 60 * 60),
            UsageReportSchedule::Weekly => std::time::Duration::from_secs(7 * 24 * 60 * 60),
        }
    }

    /// Stable string form for storage.
    pub fn as_str(self) -> &'static str {
        match self {
            UsageReportSchedule::Daily => "daily",
            UsageReportSchedule::Weekly => "weekly",
        }
    }
}

impl std::str::FromStr for UsageReportSchedule {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "daily" => Ok(UsageReportSchedule::Daily),
            "weekly" => Ok(UsageReportSchedule::Weekly),
            _ => Err(()),
        }
    }
}

/// SMTP relay configuration.
///
/// The `password` field holds the plaintext secret only in memory; it is
/// encrypted before storage and returned empty from read APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// Whether email sending is enabled.
    pub enabled: bool,
    /// SMTP server hostname (e.g. "mail.smtp2go.com").
    pub host: String,
    /// SMTP server port (typically 587 for STARTTLS, 465 for implicit TLS).
    pub port: u16,
    /// SMTP username.
    pub username: String,
    /// SMTP password (write-only; empty in read responses).
    #[serde(default)]
    pub password: String,
    /// From address for outbound mail.
    pub from_address: String,
    /// Optional display name for the From header.
    #[serde(default)]
    pub from_name: Option<String>,
    /// Use STARTTLS (port 587). When false, implicit TLS (port 465) is used.
    #[serde(default = "default_true")]
    pub use_starttls: bool,
    /// Email an alert when a notification delivery fails.
    #[serde(default)]
    pub alert_on_delivery_failure: bool,
    /// Email periodic storage usage reports.
    #[serde(default)]
    pub send_usage_reports: bool,
    /// Schedule for usage reports.
    #[serde(default)]
    pub usage_report_schedule: UsageReportSchedule,
    /// Email an alert on security/audit events (failed logins, policy changes).
    #[serde(default)]
    pub alert_on_audit_events: bool,
    /// Recipients for alerts and reports.
    #[serde(default)]
    pub alert_recipients: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: 587,
            username: String::new(),
            password: String::new(),
            from_address: String::new(),
            from_name: Some("Strix".to_string()),
            use_starttls: true,
            alert_on_delivery_failure: false,
            send_usage_reports: false,
            usage_report_schedule: UsageReportSchedule::Weekly,
            alert_on_audit_events: false,
            alert_recipients: Vec::new(),
        }
    }
}

impl SmtpConfig {
    /// Build a configuration pre-filled with SMTP2Go relay defaults.
    pub fn smtp2go(
        username: impl Into<String>,
        password: impl Into<String>,
        from_address: impl Into<String>,
    ) -> Self {
        Self {
            enabled: true,
            host: "mail.smtp2go.com".to_string(),
            port: 587,
            username: username.into(),
            password: password.into(),
            from_address: from_address.into(),
            use_starttls: true,
            ..Self::default()
        }
    }
}
