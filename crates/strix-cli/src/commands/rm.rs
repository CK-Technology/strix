//! Remove command.

use anyhow::Result;

use crate::commands::RmArgs;
use crate::config::{Config, parse_path};
use crate::s3::create_client;
use crate::util::{confirm, success};

pub async fn run(args: RmArgs) -> Result<()> {
    let config = Config::load()?;
    let (alias_name, bucket, key) = parse_path(&args.target)?;

    let alias = config
        .get_alias(&alias_name)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", alias_name))?;

    let bucket = bucket.ok_or_else(|| anyhow::anyhow!("Bucket name required"))?;

    let client = create_client(alias).await?;

    if args.recursive {
        // Delete all objects with prefix
        let prefix = key.unwrap_or_default();

        if !args.force
            && !confirm(&format!(
                "Remove all objects in `{}/{}/{}*`?",
                alias_name, bucket, prefix
            ))
        {
            return Ok(());
        }

        let mut deleted = 0;
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = client
                .list_objects_v2()
                .bucket(&bucket)
                .prefix(&prefix)
                .max_keys(1000);

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let response = request.send().await?;

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

            let count = delete_objects.len();

            let delete = aws_sdk_s3::types::Delete::builder()
                .set_objects(Some(delete_objects))
                .build()?;

            client
                .delete_objects()
                .bucket(&bucket)
                .delete(delete)
                .send()
                .await?;

            deleted += count;

            continuation_token = response.next_continuation_token().map(String::from);
            if continuation_token.is_none() {
                break;
            }
        }

        success(&format!("Removed {} objects", deleted));
    } else {
        // Delete single object
        let key = key.ok_or_else(|| anyhow::anyhow!("Object key required"))?;

        if !args.force && !confirm(&format!("Remove `{}/{}/{}`?", alias_name, bucket, key)) {
            return Ok(());
        }

        client
            .delete_object()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await?;

        success(&format!("Removed `{}/{}/{}`", alias_name, bucket, key));
    }

    Ok(())
}
