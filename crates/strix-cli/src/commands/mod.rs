//! CLI command implementations.

pub mod alias;
pub mod cp;
pub mod event;
pub mod group;
pub mod info;
pub mod key;
pub mod ls;
pub mod mb;
pub mod policy;
pub mod rb;
pub mod rm;
pub mod settings;
pub mod stat;
pub mod usage;
pub mod user;

use clap::{Args, Subcommand};

// === Alias Commands ===

#[derive(Subcommand)]
pub enum AliasCommands {
    /// Set a server alias
    Set(alias::SetArgs),
    /// Remove a server alias
    Remove(alias::RemoveArgs),
    /// List all aliases
    List,
}

// === S3 Commands ===

#[derive(Args)]
pub struct LsArgs {
    /// Target path (alias/bucket/prefix)
    #[arg(default_value = "")]
    pub target: String,

    /// Recursive listing
    #[arg(short, long)]
    pub recursive: bool,

    /// Show versions
    #[arg(long)]
    pub versions: bool,
}

#[derive(Args)]
pub struct CpArgs {
    /// Source path
    pub source: String,

    /// Destination path
    pub dest: String,

    /// Recursive copy
    #[arg(short, long)]
    pub recursive: bool,
}

#[derive(Args)]
pub struct RmArgs {
    /// Target path (alias/bucket/key)
    pub target: String,

    /// Recursive removal
    #[arg(short, long)]
    pub recursive: bool,

    /// Force removal without confirmation
    #[arg(short, long)]
    pub force: bool,

    /// Remove all versions
    #[arg(long)]
    pub versions: bool,
}

#[derive(Args)]
pub struct MbArgs {
    /// Bucket path (alias/bucket)
    pub target: String,

    /// Region
    #[arg(long)]
    pub region: Option<String>,
}

#[derive(Args)]
pub struct RbArgs {
    /// Bucket path (alias/bucket)
    pub target: String,

    /// Force removal (delete all objects first)
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args)]
pub struct StatArgs {
    /// Target path (alias/bucket/key)
    pub target: String,
}

// === Admin Commands ===

#[derive(Subcommand)]
pub enum UserCommands {
    /// List users
    List(user::ListArgs),
    /// Create a user
    Add(user::AddArgs),
    /// Remove a user
    Remove(user::RemoveArgs),
    /// Show user info
    Info(user::InfoArgs),
}

#[derive(Subcommand)]
pub enum KeyCommands {
    /// List access keys for a user
    List(key::ListArgs),
    /// Create an access key
    Create(key::CreateArgs),
    /// Remove an access key
    Remove(key::RemoveArgs),
}

#[derive(Subcommand)]
pub enum GroupCommands {
    /// List groups
    List(group::ListArgs),
    /// Create a group
    Add(group::AddArgs),
    /// Remove a group
    Remove(group::RemoveArgs),
    /// Show group info
    Info(group::InfoArgs),
    /// Add a user to a group
    AddMember(group::AddMemberArgs),
    /// Remove a user from a group
    RemoveMember(group::RemoveMemberArgs),
    /// Attach a policy to a group
    AttachPolicy(group::AttachPolicyArgs),
    /// Detach a policy from a group
    DetachPolicy(group::DetachPolicyArgs),
}

#[derive(Subcommand)]
pub enum PolicyCommands {
    /// List managed policies
    List(policy::ListArgs),
    /// Create a managed policy
    Add(policy::AddArgs),
    /// Remove a managed policy
    Remove(policy::RemoveArgs),
    /// Show policy info
    Info(policy::InfoArgs),
    /// Attach policy to a user
    Attach(policy::AttachArgs),
    /// Detach policy from a user
    Detach(policy::DetachArgs),
}

#[derive(Subcommand)]
pub enum EventCommands {
    /// List event notifications for a bucket
    List(event::ListArgs),
    /// Add an event notification
    Add(event::AddArgs),
    /// Remove an event notification
    Remove(event::RemoveArgs),
}

#[derive(Subcommand)]
pub enum SettingsCommands {
    /// Get server configuration
    Get(settings::GetArgs),
    /// Set a server configuration value
    Set(settings::SetArgs),
}

#[derive(Args)]
pub struct InfoArgs {
    /// Alias name
    pub alias: String,
}

#[derive(Args)]
pub struct UsageArgs {
    /// Alias name
    pub alias: String,
}
