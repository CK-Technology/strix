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

    /// ARN of the notification target (e.g., arn:aws:sqs:...)
    #[arg(short, long)]
    pub arn: String,

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

use super::EventCommands;

pub async fn run(cmd: EventCommands) -> Result<()> {
    match cmd {
        EventCommands::List(args) => list(args).await,
        EventCommands::Add(args) => add(args).await,
        EventCommands::Remove(args) => remove(args).await,
    }
}

async fn list(args: ListArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    let notifications = client.get_bucket_notifications(&args.bucket).await?;

    let mut has_any = false;

    // Queue configurations
    if !notifications.queue_configurations.is_empty() {
        has_any = true;
        println!("Queue Configurations:");
        println!("{:<20} {:<40} EVENTS", "ID", "ARN");
        println!("{}", "-".repeat(80));
        for config in &notifications.queue_configurations {
            println!(
                "{:<20} {:<40} {}",
                config.id.as_deref().unwrap_or("-"),
                config.queue_arn,
                config.events.join(", ")
            );
        }
        println!();
    }

    // Topic configurations
    if !notifications.topic_configurations.is_empty() {
        has_any = true;
        println!("Topic Configurations:");
        println!("{:<20} {:<40} EVENTS", "ID", "ARN");
        println!("{}", "-".repeat(80));
        for config in &notifications.topic_configurations {
            println!(
                "{:<20} {:<40} {}",
                config.id.as_deref().unwrap_or("-"),
                config.topic_arn,
                config.events.join(", ")
            );
        }
        println!();
    }

    // Lambda configurations
    if !notifications.lambda_configurations.is_empty() {
        has_any = true;
        println!("Lambda Configurations:");
        println!("{:<20} {:<40} EVENTS", "ID", "ARN");
        println!("{}", "-".repeat(80));
        for config in &notifications.lambda_configurations {
            println!(
                "{:<20} {:<40} {}",
                config.id.as_deref().unwrap_or("-"),
                config.lambda_arn,
                config.events.join(", ")
            );
        }
        println!();
    }

    if !has_any {
        println!(
            "No event notifications configured for bucket '{}'.",
            args.bucket
        );
    }

    Ok(())
}

async fn add(args: AddArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client
        .create_bucket_notification(
            &args.bucket,
            &args.arn,
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
    let client = AdminClient::new(&alias);

    client
        .delete_bucket_notification(&args.bucket, &args.id)
        .await?;
    println!(
        "Event notification '{}' removed from bucket '{}'.",
        args.id, args.bucket
    );

    Ok(())
}
