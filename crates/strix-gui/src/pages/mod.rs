//! Application pages.

mod dashboard;
mod login;
mod buckets;
mod users;
mod groups;
mod policies;
mod access_keys;
mod metrics;
mod logs;
mod audit;
mod events;
mod configuration;
mod openid;
mod settings;
mod billing;
mod tenants;
mod not_found;
mod placeholder;

pub use dashboard::Dashboard;
pub use login::Login;
pub use buckets::{Buckets, BucketDetail};
pub use users::{Users, UserDetail};
pub use groups::{Groups, GroupDetail};
pub use policies::Policies;
pub use access_keys::AccessKeys;
pub use metrics::Metrics;
pub use logs::Logs;
pub use audit::Audit;
pub use events::Events;
pub use configuration::Configuration;
pub use settings::Settings;
pub use billing::BillingExports;
pub use tenants::Tenants;
pub use not_found::NotFound;

pub use openid::OpenId;

// Placeholder pages for features in development
pub use placeholder::License;
