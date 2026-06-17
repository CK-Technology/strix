//! Event/notification management commands.

use anyhow::Result;
use clap::Args;

use crate::admin::AdminClient;
use crate::config::Config;
use crate::util::parse_alias;

/// Arguments for listing event notifications.
#[derive(Args)]
pub struct ListArgs {
    /// Alias name
    pub alias: String,

    /// Bucket name
    pub bucket: String,
}

/// Arguments for adding an event notification.
#[derive(Args)]
pub struct AddArgs {
    /// Alias name
    pub alias: String,

    /// Bucket name
    pub bucket: String,

    /// Event types (e.g., s3:ObjectCreated:*, s3:ObjectRemoved:*)
    #[arg(short, long, required = true)]
    pub events: Vec<String>,

    /// Destination endpoint. For webhook: the URL (e.g. http://host/hook).
    /// For kafka: brokers:topic. For amqp/redis: the broker URL.
    #[arg(short = 'u', long)]
    pub endpoint: String,

    /// Destination type: webhook (default), amqp, kafka, or redis.
    /// Only webhook is delivered; others are stored as config-only.
    #[arg(short = 't', long, default_value = "webhook")]
    pub destination_type: String,

    /// Notification ID (auto-generated if not provided)
    #[arg(long)]
    pub id: Option<String>,

    /// Prefix filter
    #[arg(long)]
    pub prefix: Option<String>,

    /// Suffix filter
    #[arg(long)]
    pub suffix: Option<String>,
}

/// Arguments for removing an event notification.
#[derive(Args)]
pub struct RemoveArgs {
    /// Alias name
    pub alias: String,

    /// Bucket name
    pub bucket: String,

    /// Notification ID to remove
    pub id: String,
}

/// Arguments for viewing notification delivery attempts.
#[derive(Args)]
pub struct DeliveriesArgs {
    /// Alias name
    pub alias: String,

    /// Filter by bucket name
    #[arg(long)]
    pub bucket: Option<String>,

    /// Filter by status (success, failed, unsupported)
    #[arg(long)]
    pub status: Option<String>,

    /// Maximum number of records to show
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
}

use super::EventCommands;

pub async fn run(cmd: EventCommands) -> Result<()> {
    match cmd {
        EventCommands::List(args) => list(args).await,
        EventCommands::Add(args) => add(args).await,
        EventCommands::Remove(args) => remove(args).await,
        EventCommands::Deliveries(args) => deliveries(args).await,
    }
}

async fn list(args: ListArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let mut client = AdminClient::new(&alias);

    let notifications = client.get_bucket_notifications(&args.bucket).await?;

    if notifications.rules.is_empty() {
        println!(
            "No event notifications configured for bucket '{}'.",
            args.bucket
        );
        return Ok(());
    }

    println!(
        "{:<24} {:<10} {:<24} {:<10} EVENTS",
        "ID", "TYPE", "DESTINATION", "FILTER"
    );
    println!("{}", "-".repeat(100));
    for rule in &notifications.rules {
        let filter = match (&rule.prefix, &rule.suffix) {
            (Some(p), Some(s)) => format!("{p}*{s}"),
            (Some(p), None) => format!("{p}*"),
            (None, Some(s)) => format!("*{s}"),
            (None, None) => "-".to_string(),
        };
        println!(
            "{:<24} {:<10} {:<24} {:<10} {}",
            rule.id,
            rule.destination_type,
            rule.destination_url,
            filter,
            rule.events.join(", ")
        );
    }

    Ok(())
}

async fn add(args: AddArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let mut client = AdminClient::new(&alias);

    client
        .create_bucket_notification(
            &args.bucket,
            &args.destination_type,
            &args.endpoint,
            &args.events,
            args.id.as_deref(),
            args.prefix.as_deref(),
            args.suffix.as_deref(),
        )
        .await?;

    println!("Event notification added to bucket '{}'.", args.bucket);

    Ok(())
}

async fn remove(args: RemoveArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let mut client = AdminClient::new(&alias);

    client
        .delete_bucket_notification(&args.bucket, &args.id)
        .await?;
    println!(
        "Event notification '{}' removed from bucket '{}'.",
        args.id, args.bucket
    );

    Ok(())
}

async fn deliveries(args: DeliveriesArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let mut client = AdminClient::new(&alias);

    let response = client
        .query_notification_deliveries(args.bucket.as_deref(), args.status.as_deref(), args.limit)
        .await?;

    if response.entries.is_empty() {
        println!("No notification delivery attempts recorded.");
        return Ok(());
    }

    println!(
        "{:<20} {:<12} {:<10} {:<8} {:<6} TARGET",
        "TIME", "BUCKET", "STATUS", "EVENT", "TRIES"
    );
    println!("{}", "-".repeat(90));
    for d in &response.entries {
        let time = d.timestamp.split('.').next().unwrap_or(&d.timestamp);
        let event = d.event_type.rsplit(':').next().unwrap_or(&d.event_type);
        let status = match d.response_code {
            Some(code) => format!("{} ({code})", d.status),
            None => d.status.clone(),
        };
        println!(
            "{:<20} {:<12} {:<10} {:<8} {:<6} {} -> {}",
            time, d.bucket, status, event, d.attempts, d.destination_type, d.target
        );
        if let Some(err) = &d.last_error
            && d.status != "success"
        {
            println!("    last error: {err}");
        }
    }
    println!("\n{} delivery record(s).", response.total);

    Ok(())
}
