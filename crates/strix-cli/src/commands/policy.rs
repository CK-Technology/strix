//! Policy management commands.

use anyhow::Result;
use clap::Args;

use crate::admin::AdminClient;
use crate::config::Config;
use crate::util::parse_alias;

/// Arguments for listing policies.
#[derive(Args)]
pub struct ListArgs {
    /// Alias name
    pub alias: String,
}

/// Arguments for adding a policy.
#[derive(Args)]
pub struct AddArgs {
    /// Alias name
    pub alias: String,

    /// Policy name
    pub name: String,

    /// Policy document (JSON string or @file path)
    pub document: String,

    /// Policy description
    #[arg(short, long)]
    pub description: Option<String>,
}

/// Arguments for removing a policy.
#[derive(Args)]
pub struct RemoveArgs {
    /// Alias name
    pub alias: String,

    /// Policy name
    pub name: String,
}

/// Arguments for showing policy info.
#[derive(Args)]
pub struct InfoArgs {
    /// Alias name
    pub alias: String,

    /// Policy name
    pub name: String,
}

/// Arguments for attaching policy to user.
#[derive(Args)]
pub struct AttachArgs {
    /// Alias name
    pub alias: String,

    /// Policy name
    pub policy: String,

    /// Username to attach policy to
    pub username: String,
}

/// Arguments for detaching policy from user.
#[derive(Args)]
pub struct DetachArgs {
    /// Alias name
    pub alias: String,

    /// Policy name
    pub policy: String,

    /// Username to detach policy from
    pub username: String,
}

use super::PolicyCommands;

pub async fn run(cmd: PolicyCommands) -> Result<()> {
    match cmd {
        PolicyCommands::List(args) => list(args).await,
        PolicyCommands::Add(args) => add(args).await,
        PolicyCommands::Remove(args) => remove(args).await,
        PolicyCommands::Info(args) => info(args).await,
        PolicyCommands::Attach(args) => attach(args).await,
        PolicyCommands::Detach(args) => detach(args).await,
    }
}

async fn list(args: ListArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    let policies = client.list_policies().await?;

    if policies.policies.is_empty() {
        println!("No managed policies found.");
        return Ok(());
    }

    println!("{:<30} {:<24} DESCRIPTION", "NAME", "CREATED");
    println!("{}", "-".repeat(80));

    for policy in policies.policies {
        println!(
            "{:<30} {:<24} {}",
            policy.name,
            policy.created_at,
            policy.description.unwrap_or_default()
        );
    }

    Ok(())
}

async fn add(args: AddArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    // Read document from file if it starts with @
    let document = if args.document.starts_with('@') {
        let path = &args.document[1..];
        std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read policy file '{}': {}", path, e))?
    } else {
        args.document
    };

    // Validate JSON
    serde_json::from_str::<serde_json::Value>(&document)
        .map_err(|e| anyhow::anyhow!("Invalid policy JSON: {}", e))?;

    client
        .create_policy(&args.name, &document, args.description.as_deref())
        .await?;
    println!("Policy '{}' created successfully.", args.name);

    Ok(())
}

async fn remove(args: RemoveArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client.delete_policy(&args.name).await?;
    println!("Policy '{}' removed successfully.", args.name);

    Ok(())
}

async fn info(args: InfoArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    let policy = client.get_policy(&args.name).await?;

    println!("Name:        {}", policy.name);
    println!("ARN:         {}", policy.arn);
    println!("Created:     {}", policy.created_at);
    if let Some(desc) = &policy.description {
        println!("Description: {}", desc);
    }

    println!("\nDocument:");
    // Pretty-print the policy document
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&policy.document) {
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{}", policy.document);
    }

    Ok(())
}

async fn attach(args: AttachArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client
        .attach_policy_to_user(&args.username, &args.policy)
        .await?;
    println!(
        "Policy '{}' attached to user '{}'.",
        args.policy, args.username
    );

    Ok(())
}

async fn detach(args: DetachArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client
        .detach_policy_from_user(&args.username, &args.policy)
        .await?;
    println!(
        "Policy '{}' detached from user '{}'.",
        args.policy, args.username
    );

    Ok(())
}
