//! Remove bucket command.

use anyhow::Result;

use crate::commands::RbArgs;
use crate::config::{Config, parse_path};
use crate::s3::create_client;
use crate::util::{success, warning};

pub async fn run(args: RbArgs) -> Result<()> {
    let config = Config::load()?;
    let (alias_name, bucket, _) = parse_path(&args.target)?;

    let alias = config
        .get_alias(&alias_name)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", alias_name))?;

    let bucket = bucket.ok_or_else(|| anyhow::anyhow!("Bucket name required"))?;

    let client = create_client(alias).await?;

    // Check if bucket is empty
    let objects = client
        .list_objects_v2()
        .bucket(&bucket)
        .max_keys(1)
        .send()
        .await?;

    let has_objects = !objects.contents().is_empty();

    if has_objects {
        if args.force {
            // Delete all objects first
            warning(&format!(
                "Bucket `{}` is not empty, removing all objects...",
                bucket
            ));
            delete_all_objects(&client, &bucket).await?;
        } else {
            anyhow::bail!(
                "Bucket `{}` is not empty. Use --force to delete all objects first.",
                bucket
            );
        }
    }

    // Delete the bucket
    client.delete_bucket().bucket(&bucket).send().await?;

    success(&format!("Removed bucket `{}/{}`", alias_name, bucket));
    Ok(())
}

async fn delete_all_objects(client: &aws_sdk_s3::Client, bucket: &str) -> Result<()> {
    loop {
        let response = client
            .list_objects_v2()
            .bucket(bucket)
            .max_keys(1000)
            .send()
            .await?;

        let objects = response.contents();
        if objects.is_empty() {
            break;
        }

        // Delete objects in batch
        let delete_objects: Vec<_> = objects
            .iter()
            .filter_map(|obj| {
                obj.key().map(|k| {
                    aws_sdk_s3::types::ObjectIdentifier::builder()
                        .key(k)
                        .build()
                        .unwrap()
                })
            })
            .collect();

        let delete = aws_sdk_s3::types::Delete::builder()
            .set_objects(Some(delete_objects))
            .build()?;

        client
            .delete_objects()
            .bucket(bucket)
            .delete(delete)
            .send()
            .await?;
    }

    Ok(())
}
