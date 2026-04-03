//! Make bucket command.

use anyhow::Result;

use crate::commands::MbArgs;
use crate::config::{Config, parse_path};
use crate::s3::create_client;
use crate::util::success;

pub async fn run(args: MbArgs) -> Result<()> {
    let config = Config::load()?;
    let (alias_name, bucket, _) = parse_path(&args.target)?;

    let alias = config
        .get_alias(&alias_name)
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", alias_name))?;

    let bucket = bucket.ok_or_else(|| anyhow::anyhow!("Bucket name required"))?;

    let client = create_client(alias).await?;

    let mut request = client.create_bucket().bucket(&bucket);

    if let Some(region) = &args.region {
        use aws_sdk_s3::types::CreateBucketConfiguration;
        let config = CreateBucketConfiguration::builder()
            .location_constraint(region.as_str().into())
            .build();
        request = request.create_bucket_configuration(config);
    }

    request.send().await?;

    success(&format!("Created bucket `{}/{}`", alias_name, bucket));
    Ok(())
}
