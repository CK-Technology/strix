//! User management commands.

use anyhow::Result;
use clap::Args;
use colored::Colorize;

use crate::admin::AdminClient;
use crate::commands::UserCommands;
use crate::config::Config;
use crate::util::{format_date, success};

#[derive(Args)]
pub struct ListArgs {
    /// Alias name
    pub alias: String,
}

#[derive(Args)]
pub struct AddArgs {
    /// Alias name
    pub alias: String,

    /// Username
    pub username: String,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Alias name
    pub alias: String,

    /// Username
    pub username: String,
}

#[derive(Args)]
pub struct InfoArgs {
    /// Alias name
    pub alias: String,

    /// Username
    pub username: String,
}

pub async fn run(cmd: UserCommands) -> Result<()> {
    match cmd {
        UserCommands::List(args) => list_users(args).await,
        UserCommands::Add(args) => add_user(args).await,
        UserCommands::Remove(args) => remove_user(args).await,
        UserCommands::Info(args) => user_info(args).await,
    }
}

async fn list_users(args: ListArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = config
        .get_alias(&args.alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", args.alias))?;

    let client = AdminClient::new(alias);
    let response = client.list_users().await?;

    if response.users.is_empty() {
        println!("No users found.");
        return Ok(());
    }

    println!(
        "{:<20} {:<10} {:<20} {}",
        "USERNAME".bold(),
        "STATUS".bold(),
        "CREATED".bold(),
        "POLICIES".bold()
    );

    for user in &response.users {
        let status = if user.status == "active" {
            user.status.green().to_string()
        } else {
            user.status.red().to_string()
        };

        let policies = if user.policies.is_empty() {
            "-".to_string()
        } else {
            user.policies.join(", ")
        };

        println!(
            "{:<20} {:<10} {:<20} {}",
            user.username,
            status,
            format_date(&user.created_at),
            policies
        );
    }

    Ok(())
}

async fn add_user(args: AddArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = config
        .get_alias(&args.alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", args.alias))?;

    let client = AdminClient::new(alias);
    let response = client.create_user(&args.username).await?;

    success(&format!("Created user `{}`", response.user.username));

    if let Some(key) = response.access_key {
        println!();
        println!(
            "{}",
            "Access credentials (save these now!):".yellow().bold()
        );
        println!("  Access Key: {}", key.access_key_id);
        if let Some(secret) = key.secret_access_key {
            println!("  Secret Key: {}", secret);
        }
    }

    Ok(())
}

async fn remove_user(args: RemoveArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = config
        .get_alias(&args.alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", args.alias))?;

    let client = AdminClient::new(alias);
    client.delete_user(&args.username).await?;

    success(&format!("Removed user `{}`", args.username));
    Ok(())
}

async fn user_info(args: InfoArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = config
        .get_alias(&args.alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", args.alias))?;

    let client = AdminClient::new(alias);
    let user = client.get_user(&args.username).await?;

    println!("{}: {}", "Username".bold(), user.username);
    println!("{}: {}", "ARN".bold(), user.arn);
    println!("{}: {}", "Status".bold(), user.status);
    println!("{}: {}", "Created".bold(), format_date(&user.created_at));
    println!(
        "{}: {}",
        "Policies".bold(),
        if user.policies.is_empty() {
            "-".to_string()
        } else {
            user.policies.join(", ")
        }
    );

    Ok(())
}
