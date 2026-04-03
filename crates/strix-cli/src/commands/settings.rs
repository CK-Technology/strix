//! Server settings/configuration commands.

use anyhow::Result;
use clap::Args;

use crate::admin::AdminClient;
use crate::config::Config;
use crate::util::parse_alias;

/// Arguments for getting settings.
#[derive(Args)]
pub struct GetArgs {
    /// Alias name
    pub alias: String,

    /// Setting key (optional - show all if not provided)
    pub key: Option<String>,
}

/// Arguments for setting a value.
#[derive(Args)]
pub struct SetArgs {
    /// Alias name
    pub alias: String,

    /// Setting key
    pub key: String,

    /// Setting value
    pub value: String,
}

use super::SettingsCommands;

pub async fn run(cmd: SettingsCommands) -> Result<()> {
    match cmd {
        SettingsCommands::Get(args) => get(args).await,
        SettingsCommands::Set(args) => set(args).await,
    }
}

async fn get(args: GetArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    let settings = client.get_config().await?;

    if let Some(key) = &args.key {
        // Show specific key
        if let Some(value) = settings.get(key) {
            println!("{}: {}", key, serde_json::to_string_pretty(value)?);
        } else {
            anyhow::bail!("Setting '{}' not found", key);
        }
    } else {
        // Show all settings
        println!("{}", serde_json::to_string_pretty(&settings)?);
    }

    Ok(())
}

async fn set(args: SetArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    // Parse value as JSON if possible, otherwise use as string
    let value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    client.set_config(&args.key, &value).await?;
    println!("Setting '{}' updated.", args.key);

    Ok(())
}
