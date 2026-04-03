//! Access key management commands.

use anyhow::Result;
use clap::Args;
use colored::Colorize;

use crate::admin::AdminClient;
use crate::commands::KeyCommands;
use crate::config::Config;
use crate::util::{format_date, success};

#[derive(Args)]
pub struct ListArgs {
    /// Alias name
    pub alias: String,

    /// Username
    pub username: String,
}

#[derive(Args)]
pub struct CreateArgs {
    /// Alias name
    pub alias: String,

    /// Username
    pub username: String,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Alias name
    pub alias: String,

    /// Access key ID
    pub access_key_id: String,
}

pub async fn run(cmd: KeyCommands) -> Result<()> {
    match cmd {
        KeyCommands::List(args) => list_keys(args).await,
        KeyCommands::Create(args) => create_key(args).await,
        KeyCommands::Remove(args) => remove_key(args).await,
    }
}

async fn list_keys(args: ListArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = config
        .get_alias(&args.alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", args.alias))?;

    let client = AdminClient::new(alias);
    let response = client.list_access_keys(&args.username).await?;

    if response.access_keys.is_empty() {
        println!("No access keys found for user `{}`.", args.username);
        return Ok(());
    }

    println!(
        "{:<25} {:<10} {}",
        "ACCESS KEY".bold(),
        "STATUS".bold(),
        "CREATED".bold()
    );

    for key in &response.access_keys {
        let status = if key.status == "active" {
            key.status.green().to_string()
        } else {
            key.status.red().to_string()
        };

        println!(
            "{:<25} {:<10} {}",
            key.access_key_id,
            status,
            format_date(&key.created_at)
        );
    }

    Ok(())
}

async fn create_key(args: CreateArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = config
        .get_alias(&args.alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", args.alias))?;

    let client = AdminClient::new(alias);
    let key = client.create_access_key(&args.username).await?;

    success(&format!("Created access key for user `{}`", args.username));
    println!();
    println!(
        "{}",
        "Access credentials (save these now!):".yellow().bold()
    );
    println!("  Access Key: {}", key.access_key_id);
    println!("  Secret Key: {}", key.secret_access_key);

    Ok(())
}

async fn remove_key(args: RemoveArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = config
        .get_alias(&args.alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", args.alias))?;

    let client = AdminClient::new(alias);
    client.delete_access_key(&args.access_key_id).await?;

    success(&format!("Removed access key `{}`", args.access_key_id));
    Ok(())
}
