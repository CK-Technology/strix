//! Strix CLI (sx) - Command-line tool for Strix object storage.
//!
//! A user-friendly CLI for S3 and admin operations, similar to MinIO's mc.

mod admin;
mod commands;
mod config;
mod s3;
mod util;

use clap::{Parser, Subcommand};

/// Strix CLI - S3-compatible object storage client
#[derive(Parser)]
#[command(name = "sx", version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Configure server aliases
    #[command(subcommand)]
    Alias(commands::AliasCommands),

    /// List buckets or objects
    Ls(commands::LsArgs),

    /// Copy files/objects
    Cp(commands::CpArgs),

    /// Remove files/objects
    Rm(commands::RmArgs),

    /// Make a bucket
    Mb(commands::MbArgs),

    /// Remove a bucket
    Rb(commands::RbArgs),

    /// Display object/bucket info
    Stat(commands::StatArgs),

    /// Manage users
    #[command(subcommand)]
    User(commands::UserCommands),

    /// Manage access keys
    #[command(subcommand)]
    Key(commands::KeyCommands),

    /// Manage groups
    #[command(subcommand)]
    Group(commands::GroupCommands),

    /// Manage IAM policies
    #[command(subcommand)]
    Policy(commands::PolicyCommands),

    /// Manage event notifications
    #[command(subcommand)]
    Event(commands::EventCommands),

    /// Manage server settings
    #[command(subcommand)]
    Settings(commands::SettingsCommands),

    /// Show server info
    Info(commands::InfoArgs),

    /// Show storage usage
    Usage(commands::UsageArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Alias(cmd) => commands::alias::run(cmd).await,
        Commands::Ls(args) => commands::ls::run(args).await,
        Commands::Cp(args) => commands::cp::run(args).await,
        Commands::Rm(args) => commands::rm::run(args).await,
        Commands::Mb(args) => commands::mb::run(args).await,
        Commands::Rb(args) => commands::rb::run(args).await,
        Commands::Stat(args) => commands::stat::run(args).await,
        Commands::User(cmd) => commands::user::run(cmd).await,
        Commands::Key(cmd) => commands::key::run(cmd).await,
        Commands::Group(cmd) => commands::group::run(cmd).await,
        Commands::Policy(cmd) => commands::policy::run(cmd).await,
        Commands::Event(cmd) => commands::event::run(cmd).await,
        Commands::Settings(cmd) => commands::settings::run(cmd).await,
        Commands::Info(args) => commands::info::run(args).await,
        Commands::Usage(args) => commands::usage::run(args).await,
    }
}
