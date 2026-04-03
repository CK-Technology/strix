//! Stat command.

use anyhow::Result;
use colored::Colorize;

use crate::commands::StatArgs;
use crate::config::{Config, parse_path};
use crate::s3::create_client;
use crate::util::{format_bytes, format_datetime};

pub async fn run(args: StatArgs) -> Result<()> {
    let config = Config::load()?;
    let (alias_name, bucket, key) = parse_path(&args.target)?;

    let alias = config
        .get_alias(&alias_name)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", alias_name))?;

    let bucket = bucket.ok_or_else(|| anyhow::anyhow!("Bucket name required"))?;

    let client = create_client(alias).await?;

    if let Some(key) = key {
        // Object stat
        let response = client
            .head_object()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await?;

        println!("{}: {}/{}/{}", "Name".bold(), alias_name, bucket, key);
        println!(
            "{}: {}",
            "Size".bold(),
            format_bytes(response.content_length().unwrap_or(0) as u64)
        );
        println!("{}: {}", "ETag".bold(), response.e_tag().unwrap_or("-"));
        println!(
            "{}: {}",
            "Content-Type".bold(),
            response.content_type().unwrap_or("-")
        );
        println!(
            "{}: {}",
            "Last Modified".bold(),
            response
                .last_modified()
                .map(|d| format_datetime(&d.to_string()))
                .unwrap_or_else(|| "-".to_string())
        );
        if let Some(class) = response.storage_class() {
            println!("{}: {}", "Storage Class".bold(), class.as_str());
        }
    } else {
        // Bucket stat
        let _response = client.head_bucket().bucket(&bucket).send().await?;

        println!("{}: {}/{}", "Name".bold(), alias_name, bucket);
        println!("{}: bucket", "Type".bold());

        // Count objects
        let mut total_objects = 0;
        let mut total_size: u64 = 0;
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = client.list_objects_v2().bucket(&bucket).max_keys(1000);

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let response = request.send().await?;

            let objects = response.contents();
            total_objects += objects.len();
            total_size += objects
                .iter()
                .map(|o| o.size().unwrap_or(0) as u64)
                .sum::<u64>();

            continuation_token = response.next_continuation_token().map(String::from);
            if continuation_token.is_none() {
                break;
            }
        }

        println!("{}: {}", "Objects".bold(), total_objects);
        println!("{}: {}", "Total Size".bold(), format_bytes(total_size));
    }

    Ok(())
}
