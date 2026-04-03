//! Copy command.

use anyhow::Result;
use aws_sdk_s3::primitives::ByteStream;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;

use crate::commands::CpArgs;
use crate::config::{Config, is_local_path, parse_path};
use crate::s3::create_client;
use crate::util::{format_bytes, success};

pub async fn run(args: CpArgs) -> Result<()> {
    let config = Config::load()?;

    let source_local = is_local_path(&args.source);
    let dest_local = is_local_path(&args.dest);

    match (source_local, dest_local) {
        (true, false) => upload(&config, &args.source, &args.dest).await,
        (false, true) => download(&config, &args.source, &args.dest).await,
        (false, false) => copy_remote(&config, &args.source, &args.dest).await,
        (true, true) => anyhow::bail!("Both source and destination are local paths"),
    }
}

async fn upload(config: &Config, source: &str, dest: &str) -> Result<()> {
    let (alias_name, bucket, key) = parse_path(dest)?;

    let alias = config
        .get_alias(&alias_name)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", alias_name))?;

    let bucket = bucket.ok_or_else(|| anyhow::anyhow!("Bucket name required in destination"))?;

    // Determine the key
    let source_path = Path::new(source);
    let key = key.unwrap_or_else(|| {
        source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string()
    });

    // Read file
    let metadata = tokio::fs::metadata(source).await?;
    let file_size = metadata.len();

    let pb = ProgressBar::new(file_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    let body = ByteStream::from_path(source).await?;

    let client = create_client(alias).await?;

    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(body)
        .send()
        .await?;

    pb.finish_and_clear();
    success(&format!(
        "`{}` -> `{}/{}/{}` ({})",
        source,
        alias_name,
        bucket,
        key,
        format_bytes(file_size)
    ));

    Ok(())
}

async fn download(config: &Config, source: &str, dest: &str) -> Result<()> {
    let (alias_name, bucket, key) = parse_path(source)?;

    let alias = config
        .get_alias(&alias_name)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", alias_name))?;

    let bucket = bucket.ok_or_else(|| anyhow::anyhow!("Bucket name required"))?;
    let key = key.ok_or_else(|| anyhow::anyhow!("Object key required"))?;

    let client = create_client(alias).await?;

    let response = client.get_object().bucket(&bucket).key(&key).send().await?;

    let content_length = response.content_length().unwrap_or(0) as u64;

    let pb = ProgressBar::new(content_length);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Determine destination path
    let dest_path = if dest == "." || dest.ends_with('/') {
        let filename = Path::new(&key)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("downloaded");
        if dest == "." {
            filename.to_string()
        } else {
            format!("{}{}", dest, filename)
        }
    } else {
        dest.to_string()
    };

    let body = response.body.collect().await?;
    tokio::fs::write(&dest_path, body.into_bytes()).await?;

    pb.finish_and_clear();
    success(&format!(
        "`{}/{}/{}` -> `{}` ({})",
        alias_name,
        bucket,
        key,
        dest_path,
        format_bytes(content_length)
    ));

    Ok(())
}

async fn copy_remote(config: &Config, source: &str, dest: &str) -> Result<()> {
    let (src_alias, src_bucket, src_key) = parse_path(source)?;
    let (dst_alias, dst_bucket, dst_key) = parse_path(dest)?;

    let src_alias_config = config
        .get_alias(&src_alias)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", src_alias))?;

    let src_bucket = src_bucket.ok_or_else(|| anyhow::anyhow!("Source bucket required"))?;
    let src_key = src_key.ok_or_else(|| anyhow::anyhow!("Source key required"))?;

    let dst_bucket = dst_bucket.ok_or_else(|| anyhow::anyhow!("Destination bucket required"))?;
    let dst_key = dst_key.unwrap_or_else(|| src_key.clone());

    // For now, only support copy within same alias
    if src_alias != dst_alias {
        anyhow::bail!("Cross-alias copy not yet supported");
    }

    let client = create_client(src_alias_config).await?;

    client
        .copy_object()
        .bucket(&dst_bucket)
        .key(&dst_key)
        .copy_source(format!("{}/{}", src_bucket, src_key))
        .send()
        .await?;

    success(&format!(
        "`{}/{}/{}` -> `{}/{}/{}`",
        src_alias, src_bucket, src_key, dst_alias, dst_bucket, dst_key
    ));

    Ok(())
}
