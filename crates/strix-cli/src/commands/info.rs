//! Server info command.

use anyhow::Result;
use colored::Colorize;

use crate::admin::AdminClient;
use crate::commands::InfoArgs;
use crate::config::Config;
use crate::util::format_duration;

pub async fn run(args: InfoArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = config
        .get_alias(&args.alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", args.alias))?;

    let client = AdminClient::new(alias);
    let info = client.get_server_info().await?;

    println!("{}", "Server Information".bold());
    println!("  {}: {}", "Version".bold(), info.version);
    println!("  {}: {}", "Mode".bold(), info.mode);
    println!("  {}: {}", "Region".bold(), info.region);
    println!("  {}: {}", "Uptime".bold(), format_duration(info.uptime));
    if let Some(commit) = info.commit {
        println!("  {}: {}", "Commit".bold(), commit);
    }

    Ok(())
}
