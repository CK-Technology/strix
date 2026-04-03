//! Alias management commands.

use anyhow::Result;
use clap::Args;
use colored::Colorize;

use crate::commands::AliasCommands;
use crate::config::{Alias, Config};

#[derive(Args)]
pub struct SetArgs {
    /// Alias name
    pub name: String,

    /// Server URL (e.g., http://localhost:9000)
    pub url: String,

    /// Access key
    pub access_key: String,

    /// Secret key
    pub secret_key: String,

    /// Admin API URL (optional)
    #[arg(long)]
    pub admin_url: Option<String>,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Alias name
    pub name: String,
}

pub async fn run(cmd: AliasCommands) -> Result<()> {
    match cmd {
        AliasCommands::Set(args) => set_alias(args).await,
        AliasCommands::Remove(args) => remove_alias(args).await,
        AliasCommands::List => list_aliases().await,
    }
}

async fn set_alias(args: SetArgs) -> Result<()> {
    let mut config = Config::load()?;

    let alias = Alias {
        url: args.url,
        access_key: args.access_key,
        secret_key: args.secret_key,
        admin_url: args.admin_url,
    };

    config.set_alias(args.name.clone(), alias);
    config.save()?;

    println!("{} Added alias `{}`", "✓".green(), args.name);
    Ok(())
}

async fn remove_alias(args: RemoveArgs) -> Result<()> {
    let mut config = Config::load()?;

    if config.remove_alias(&args.name).is_some() {
        config.save()?;
        println!("{} Removed alias `{}`", "✓".green(), args.name);
    } else {
        anyhow::bail!("Alias `{}` not found", args.name);
    }

    Ok(())
}

async fn list_aliases() -> Result<()> {
    let config = Config::load()?;

    if config.aliases.is_empty() {
        println!("No aliases configured.");
        println!();
        println!("Add an alias with:");
        println!("  sx alias set <name> <url> <access-key> <secret-key>");
        return Ok(());
    }

    println!(
        "{:<15} {:<40} {}",
        "ALIAS".bold(),
        "URL".bold(),
        "ACCESS KEY".bold()
    );
    for (name, alias) in &config.aliases {
        println!(
            "{:<15} {:<40} {}",
            name,
            alias.url,
            &alias.access_key[..alias.access_key.len().min(8)]
        );
    }

    Ok(())
}
