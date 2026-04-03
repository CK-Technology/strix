//! Utility functions for the CLI.

#![allow(dead_code)]

use anyhow::Result;
use colored::Colorize;
use humansize::{BINARY, format_size};

use crate::config::{Alias, Config};

/// Parse an alias name and return the alias configuration.
pub fn parse_alias(config: &Config, name: &str) -> Result<Alias> {
    config
        .get_alias(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Alias `{}` not found", name))
}

/// Format bytes as human-readable size.
pub fn format_bytes(bytes: u64) -> String {
    format_size(bytes, BINARY)
}

/// Format a timestamp as a readable date.
pub fn format_date(timestamp: &str) -> String {
    // Simple extraction of date part
    timestamp.split('T').next().unwrap_or(timestamp).to_string()
}

/// Format a timestamp with time.
pub fn format_datetime(timestamp: &str) -> String {
    // Format: YYYY-MM-DD HH:MM
    let parts: Vec<&str> = timestamp.split('T').collect();
    if parts.len() >= 2 {
        let date = parts[0];
        let time = parts[1]
            .split('.')
            .next()
            .unwrap_or("")
            .chars()
            .take(5)
            .collect::<String>();
        format!("{} {}", date, time)
    } else {
        timestamp.to_string()
    }
}

/// Print a success message.
pub fn success(msg: &str) {
    println!("{} {}", "✓".green(), msg);
}

/// Print an error message.
pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".red(), msg);
}

/// Print a warning message.
pub fn warning(msg: &str) {
    println!("{} {}", "!".yellow(), msg);
}

/// Print an info message.
pub fn info(msg: &str) {
    println!("{} {}", "→".blue(), msg);
}

/// Format a storage class for display.
pub fn format_storage_class(class: &str) -> String {
    match class.to_uppercase().as_str() {
        "STANDARD" => "STANDARD".to_string(),
        "REDUCED_REDUNDANCY" => "RR".to_string(),
        "GLACIER" => "GLACIER".to_string(),
        "DEEP_ARCHIVE" => "ARCHIVE".to_string(),
        other => other.to_string(),
    }
}

/// Format duration in seconds as human-readable.
pub fn format_duration(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", seconds)
    }
}

/// Prompt for confirmation.
pub fn confirm(prompt: &str) -> bool {
    use std::io::{self, Write};

    print!("{} [y/N] ", prompt);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1 MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1 GiB");
    }

    #[test]
    fn test_format_date() {
        assert_eq!(format_date("2024-01-15T10:30:00Z"), "2024-01-15");
        assert_eq!(format_date("2024-12-31"), "2024-12-31");
    }

    #[test]
    fn test_format_datetime() {
        assert_eq!(
            format_datetime("2024-01-15T10:30:00.000Z"),
            "2024-01-15 10:30"
        );
        assert_eq!(format_datetime("2024-01-15T23:59:59Z"), "2024-01-15 23:59");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(59), "59s");
        assert_eq!(format_duration(60), "1m");
        assert_eq!(format_duration(3600), "1h 0m");
        assert_eq!(format_duration(3661), "1h 1m");
        assert_eq!(format_duration(86400), "1d 0h");
        assert_eq!(format_duration(90061), "1d 1h");
    }

    #[test]
    fn test_format_storage_class() {
        assert_eq!(format_storage_class("STANDARD"), "STANDARD");
        assert_eq!(format_storage_class("standard"), "STANDARD");
        assert_eq!(format_storage_class("REDUCED_REDUNDANCY"), "RR");
        assert_eq!(format_storage_class("GLACIER"), "GLACIER");
        assert_eq!(format_storage_class("DEEP_ARCHIVE"), "ARCHIVE");
        assert_eq!(format_storage_class("CUSTOM"), "CUSTOM");
    }
}
