//! Group management commands.

use anyhow::Result;
use clap::Args;

use crate::admin::AdminClient;
use crate::config::Config;
use crate::util::parse_alias;

/// Arguments for listing groups.
#[derive(Args)]
pub struct ListArgs {
    /// Alias name
    pub alias: String,
}

/// Arguments for adding a group.
#[derive(Args)]
pub struct AddArgs {
    /// Alias name
    pub alias: String,

    /// Group name to create
    pub name: String,
}

/// Arguments for removing a group.
#[derive(Args)]
pub struct RemoveArgs {
    /// Alias name
    pub alias: String,

    /// Group name to remove
    pub name: String,
}

/// Arguments for showing group info.
#[derive(Args)]
pub struct InfoArgs {
    /// Alias name
    pub alias: String,

    /// Group name
    pub name: String,
}

/// Arguments for adding a user to a group.
#[derive(Args)]
pub struct AddMemberArgs {
    /// Alias name
    pub alias: String,

    /// Group name
    pub group: String,

    /// Username to add
    pub username: String,
}

/// Arguments for removing a user from a group.
#[derive(Args)]
pub struct RemoveMemberArgs {
    /// Alias name
    pub alias: String,

    /// Group name
    pub group: String,

    /// Username to remove
    pub username: String,
}

/// Arguments for attaching a policy to a group.
#[derive(Args)]
pub struct AttachPolicyArgs {
    /// Alias name
    pub alias: String,

    /// Group name
    pub group: String,

    /// Policy name to attach
    pub policy: String,
}

/// Arguments for detaching a policy from a group.
#[derive(Args)]
pub struct DetachPolicyArgs {
    /// Alias name
    pub alias: String,

    /// Group name
    pub group: String,

    /// Policy name to detach
    pub policy: String,
}

use super::GroupCommands;

pub async fn run(cmd: GroupCommands) -> Result<()> {
    match cmd {
        GroupCommands::List(args) => list(args).await,
        GroupCommands::Add(args) => add(args).await,
        GroupCommands::Remove(args) => remove(args).await,
        GroupCommands::Info(args) => info(args).await,
        GroupCommands::AddMember(args) => add_member(args).await,
        GroupCommands::RemoveMember(args) => remove_member(args).await,
        GroupCommands::AttachPolicy(args) => attach_policy(args).await,
        GroupCommands::DetachPolicy(args) => detach_policy(args).await,
    }
}

async fn list(args: ListArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    let groups = client.list_groups().await?;

    if groups.groups.is_empty() {
        println!("No groups found.");
        return Ok(());
    }

    println!("{:<20} {:<12} {:<24}", "NAME", "MEMBERS", "CREATED");
    println!("{}", "-".repeat(60));

    for group in groups.groups {
        println!(
            "{:<20} {:<12} {:<24}",
            group.name, group.member_count, group.created_at
        );
    }

    Ok(())
}

async fn add(args: AddArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client.create_group(&args.name).await?;
    println!("Group '{}' created successfully.", args.name);

    Ok(())
}

async fn remove(args: RemoveArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client.delete_group(&args.name).await?;
    println!("Group '{}' removed successfully.", args.name);

    Ok(())
}

async fn info(args: InfoArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    let group = client.get_group(&args.name).await?;

    println!("Name:       {}", group.name);
    println!("ARN:        {}", group.arn);
    println!("Created:    {}", group.created_at);

    if !group.members.is_empty() {
        println!("\nMembers:");
        for member in &group.members {
            println!("  - {}", member);
        }
    } else {
        println!("\nMembers:    (none)");
    }

    if !group.policies.is_empty() {
        println!("\nPolicies:");
        for policy in &group.policies {
            println!("  - {}", policy);
        }
    } else {
        println!("\nPolicies:   (none)");
    }

    Ok(())
}

async fn add_member(args: AddMemberArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client
        .add_user_to_group(&args.group, &args.username)
        .await?;
    println!("User '{}' added to group '{}'.", args.username, args.group);

    Ok(())
}

async fn remove_member(args: RemoveMemberArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client
        .remove_user_from_group(&args.group, &args.username)
        .await?;
    println!(
        "User '{}' removed from group '{}'.",
        args.username, args.group
    );

    Ok(())
}

async fn attach_policy(args: AttachPolicyArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client
        .attach_policy_to_group(&args.group, &args.policy)
        .await?;
    println!(
        "Policy '{}' attached to group '{}'.",
        args.policy, args.group
    );

    Ok(())
}

async fn detach_policy(args: DetachPolicyArgs) -> Result<()> {
    let config = Config::load()?;
    let alias = parse_alias(&config, &args.alias)?;
    let client = AdminClient::new(&alias);

    client
        .detach_policy_from_group(&args.group, &args.policy)
        .await?;
    println!(
        "Policy '{}' detached from group '{}'.",
        args.policy, args.group
    );

    Ok(())
}
