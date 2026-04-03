//! Storage usage command.

use anyhow::Result;
use colored::Colorize;

use crate::admin::AdminClient;
use crate::commands::UsageArgs;
use crate::config::Config;
use crate::util::format_bytes;

pub async fn run(args: UsageArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = config
        .get_alias(&args.alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", args.alias))?;

    let client = AdminClient::new(alias);
    let usage = client.get_storage_usage().await?;

    println!("{}", "Storage Usage".bold());
    println!("  {}: {}", "Total Buckets".bold(), usage.total_buckets);
    println!("  {}: {}", "Total Objects".bold(), usage.total_objects);
    println!(
        "  {}: {}",
        "Total Size".bold(),
        format_bytes(usage.total_size)
    );
    println!();

    if !usage.buckets.is_empty() {
        println!("{}", "Buckets:".bold());
        println!(
            "  {:<30} {:>12} {:>15}",
            "NAME".bold(),
            "OBJECTS".bold(),
            "SIZE".bold()
        );

        for bucket in &usage.buckets {
            println!(
                "  {:<30} {:>12} {:>15}",
                bucket.name,
                bucket.object_count,
                format_bytes(bucket.total_size)
            );
        }
    }

    Ok(())
}
