//! S3 client utilities.

use anyhow::Result;
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::{Client, config::Region};

use crate::config::Alias;

/// Create an S3 client for the given alias.
pub async fn create_client(alias: &Alias) -> Result<Client> {
    let credentials = Credentials::new(
        &alias.access_key,
        &alias.secret_key,
        None,
        None,
        "strix-cli",
    );

    let config = aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(credentials)
        .endpoint_url(&alias.url)
        .region(Region::new("us-east-1"))
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .force_path_style(true)
        .build();

    Ok(Client::from_conf(s3_config))
}
