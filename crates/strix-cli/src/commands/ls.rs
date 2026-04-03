//! List command.

use anyhow::Result;
use colored::Colorize;

use crate::commands::LsArgs;
use crate::config::{Config, parse_path};
use crate::s3::create_client;
use crate::util::{format_bytes, format_datetime};

pub async fn run(args: LsArgs) -> Result<()> {
    let config = Config::load()?;

    // If no target, list all aliases
    if args.target.is_empty() {
        return list_aliases(&config);
    }

    let (alias_name, bucket, prefix) = parse_path(&args.target)?;

    let alias = config
        .get_alias(&alias_name)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", alias_name))?;

    let client = create_client(alias).await?;

    // If no bucket, list buckets
    if bucket.is_none() {
        return list_buckets(&client).await;
    }

    let bucket = bucket.unwrap();
    let prefix = prefix.unwrap_or_default();

    // List objects
    list_objects(&client, &bucket, &prefix, args.recursive).await
}

fn list_aliases(config: &Config) -> Result<()> {
    if config.aliases.is_empty() {
        println!("No aliases configured.");
        return Ok(());
    }

    for name in config.aliases.keys() {
        println!("{}/", name.blue());
    }

    Ok(())
}

async fn list_buckets(client: &aws_sdk_s3::Client) -> Result<()> {
    let response = client.list_buckets().send().await?;

    for bucket in response.buckets() {
        if let Some(name) = bucket.name() {
            let created = bucket
                .creation_date()
                .map(|d| d.to_string())
                .unwrap_or_default();
            println!("{} {}/", format_datetime(&created), name.blue());
        }
    }

    Ok(())
}

async fn list_objects(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    prefix: &str,
    recursive: bool,
) -> Result<()> {
    let delimiter = if recursive {
        None
    } else {
        Some("/".to_string())
    };

    let mut request = client.list_objects_v2().bucket(bucket).prefix(prefix);

    if let Some(d) = delimiter {
        request = request.delimiter(d);
    }

    let response = request.send().await?;

    // Print common prefixes (directories)
    for prefix in response.common_prefixes() {
        if let Some(p) = prefix.prefix() {
            println!("{:>19} {:>10} {}", "", "PRE", p.blue());
        }
    }

    // Print objects
    for obj in response.contents() {
        let key = obj.key().unwrap_or("");
        let size = obj.size().unwrap_or(0);
        let modified = obj
            .last_modified()
            .map(|d| d.to_string())
            .unwrap_or_default();

        println!(
            "{} {:>10} {}",
            format_datetime(&modified),
            format_bytes(size as u64),
            key
        );
    }

    Ok(())
}
