//! Storage backend implementations for Strix.
//!
//! Uses SQLite for metadata and sharded blob storage for objects.

pub mod cleanup;
pub mod db;
pub mod localfs;
pub mod migrations;

pub use cleanup::{CleanupConfig, start_cleanup_task};
pub use localfs::LocalFsStore;
pub use migrations::{CURRENT_VERSION, get_schema_version, run_migrations};
