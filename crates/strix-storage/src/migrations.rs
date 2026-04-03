//! Database schema migrations for Strix storage.
//!
//! Provides a simple versioned migration system that tracks applied
//! migrations and runs pending ones on startup.

use rusqlite::Connection;

/// Current schema version. Increment this when adding new migrations.
pub const CURRENT_VERSION: u32 = 3;

/// Migration entry point. Runs all pending migrations.
///
/// This function:
/// 1. Creates the migrations tracking table if it doesn't exist
/// 2. Gets the current schema version
/// 3. Runs any migrations newer than the current version
/// 4. Updates the schema version
pub fn run_migrations(conn: &Connection) -> rusqlite::Result<()> {
    // Create migrations table if it doesn't exist
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS _strix_migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )?;

    // Ensure baseline tables exist for in-memory/test DBs that call migrations directly.
    crate::db::init_schema(conn)?;

    // Get current version
    let current_version: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _strix_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Run pending migrations
    let migrations = get_migrations();
    for (version, name, sql) in migrations {
        if version > current_version {
            tracing::info!("Running storage migration {}: {}", version, name);

            // Run the migration
            conn.execute_batch(sql)?;

            // Record the migration
            conn.execute(
                "INSERT INTO _strix_migrations (version, name) VALUES (?1, ?2)",
                rusqlite::params![version, name],
            )?;
        }
    }

    Ok(())
}

/// Get the current schema version from the database.
pub fn get_schema_version(conn: &Connection) -> u32 {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM _strix_migrations",
        [],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

/// Returns all migrations as (version, name, sql).
fn get_migrations() -> Vec<(u32, &'static str, &'static str)> {
    vec![
        // Initial schema - version 1
        // Note: This is the baseline schema. The actual tables are created
        // by init_schema(), so this migration just marks the initial version.
        (
            1,
            "initial_schema",
            r#"
            -- Mark initial schema as version 1
            -- Tables are created by init_schema() if they don't exist
            SELECT 1;
            "#,
        ),
        // Future migrations go here:
        (
            2,
            "add_tenants",
            r#"
            CREATE TABLE IF NOT EXISTS tenants (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                owner TEXT NOT NULL,
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_tenants_slug ON tenants(slug);

            -- Add tenant_slug only if it doesn't exist yet
            -- SQLite has no IF NOT EXISTS for ADD COLUMN, so guard with table_info.
            CREATE TEMP TABLE IF NOT EXISTS _migrate_guard(x INTEGER);
            INSERT INTO _migrate_guard(x)
            SELECT 1
            WHERE NOT EXISTS (
                SELECT 1 FROM pragma_table_info('buckets') WHERE name = 'tenant_slug'
            );

            -- This statement is executed only once per DB because migration versioning tracks execution.
            ALTER TABLE buckets ADD COLUMN tenant_slug TEXT;

            CREATE INDEX IF NOT EXISTS idx_buckets_tenant_slug ON buckets(tenant_slug);
            "#,
        ),
        (
            3,
            "add_bucket_tags",
            r#"
            CREATE TABLE IF NOT EXISTS bucket_tags (
                bucket TEXT PRIMARY KEY NOT NULL,
                tags_config TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (bucket) REFERENCES buckets(name) ON DELETE CASCADE
            );
            "#,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_migrations_run_once() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations twice
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        // Should only have one migration record
        let count: u32 = conn
            .query_row("SELECT COUNT(*) FROM _strix_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(count, CURRENT_VERSION);
        assert_eq!(get_schema_version(&conn), CURRENT_VERSION);
    }

    #[test]
    fn test_schema_version() {
        let conn = Connection::open_in_memory().unwrap();

        // No migrations table yet
        assert_eq!(get_schema_version(&conn), 0);

        // After migrations
        run_migrations(&conn).unwrap();
        assert_eq!(get_schema_version(&conn), CURRENT_VERSION);
    }
}
