//! SQLite-backed IAM storage.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use tokio_rusqlite::Connection as AsyncConnection;

use crate::{
    AccessKey, AccessKeyStatus, Action, AssumeRoleRequest, AuthorizationEffect,
    AuthorizationResult, BucketPolicy, Effect, Group, IamError, IamProvider, OidcConfig, Policy,
    PolicySource, Principal, Resource, Result, SmtpConfig, TemporaryCredential, User, UserStatus,
    generate_access_key_id, generate_secret_key,
};
use crate::smtp::UsageReportSchedule;

/// SQLite-backed IAM store.
pub struct IamStore {
    db: AsyncConnection,
    root_access_key: String,
    root_secret_key: String,
    /// Encryption key for access key secrets (derived from root secret).
    encryption_key: [u8; 32],
}

impl IamStore {
    /// Create a new IAM store with the given database connection.
    pub async fn new(
        db: AsyncConnection,
        root_access_key: String,
        root_secret_key: String,
    ) -> Result<Self> {
        // Set SQLite pragmas for durability and performance
        db.call(|conn| {
            // Enable WAL mode for better concurrency and durability
            conn.execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                PRAGMA busy_timeout = 5000;
                PRAGMA foreign_keys = ON;
                PRAGMA cache_size = -2000;
                "#,
            )?;
            Ok(())
        })
        .await?;

        // Initialize schema
        db.call(|conn| {
            init_iam_schema(conn)?;
            Ok(())
        })
        .await?;

        // Derive encryption key from root secret
        let encryption_key = crate::secrets::derive_encryption_key(&root_secret_key);

        Ok(Self {
            db,
            root_access_key,
            root_secret_key,
            encryption_key,
        })
    }

    /// Get the root user credentials (for authentication).
    pub fn root_credentials(&self) -> (&str, &str) {
        (&self.root_access_key, &self.root_secret_key)
    }

    /// Encrypt a secret for storage.
    fn encrypt_secret(&self, secret: &str) -> Result<String> {
        crate::secrets::encrypt_secret(secret, &self.encryption_key)
    }

    /// Decrypt a stored secret.
    fn decrypt_secret(&self, encrypted: &str) -> Result<String> {
        crate::secrets::decrypt_secret(encrypted, &self.encryption_key)
    }
}

/// Current schema version (for future migrations).
#[allow(dead_code)]
const SCHEMA_VERSION: i32 = 5;

fn init_iam_schema(conn: &Connection) -> rusqlite::Result<()> {
    // Enable WAL mode for better concurrency and durability
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;",
    )?;

    // Create migrations table if not exists
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        )",
        [],
    )?;

    // Get current version
    let current_version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Apply migrations
    if current_version < 1 {
        // Initial schema (v1)
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS iam_users (
                username TEXT PRIMARY KEY,
                arn TEXT NOT NULL,
                created_at TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                is_root INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS iam_access_keys (
                access_key_id TEXT PRIMARY KEY,
                secret_access_key TEXT NOT NULL,
                username TEXT NOT NULL,
                created_at TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                FOREIGN KEY (username) REFERENCES iam_users(username) ON DELETE CASCADE
            );
            "#,
        )?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (1, datetime('now'))",
            [],
        )?;
    }

    if current_version < 2 {
        // Migration v2: Add password_hash, last_used, and encrypted secrets
        // Add password_hash to users
        let has_password_hash: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('iam_users') WHERE name = 'password_hash'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_password_hash {
            conn.execute("ALTER TABLE iam_users ADD COLUMN password_hash TEXT", [])?;
        }

        // Add last_used to access_keys
        let has_last_used: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('iam_access_keys') WHERE name = 'last_used'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_last_used {
            conn.execute("ALTER TABLE iam_access_keys ADD COLUMN last_used TEXT", [])?;
        }

        // Rename secret_access_key to secret_access_key_encrypted if needed
        // Note: SQLite doesn't support RENAME COLUMN in older versions, but does in 3.25+
        // We'll check if the new column exists, and if not, we need to migrate
        let has_encrypted_col: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('iam_access_keys') WHERE name = 'secret_access_key_encrypted'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_encrypted_col {
            // Try to rename, fallback to recreating table
            let result = conn.execute(
                "ALTER TABLE iam_access_keys RENAME COLUMN secret_access_key TO secret_access_key_encrypted",
                [],
            );

            if result.is_err() {
                // Older SQLite - need to recreate table
                conn.execute_batch(
                    r#"
                    CREATE TABLE IF NOT EXISTS iam_access_keys_new (
                        access_key_id TEXT PRIMARY KEY,
                        secret_access_key_encrypted TEXT NOT NULL,
                        username TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        status TEXT NOT NULL DEFAULT 'active',
                        last_used TEXT,
                        FOREIGN KEY (username) REFERENCES iam_users(username) ON DELETE CASCADE
                    );

                    INSERT INTO iam_access_keys_new (access_key_id, secret_access_key_encrypted, username, created_at, status)
                    SELECT access_key_id, secret_access_key, username, created_at, status FROM iam_access_keys;

                    DROP TABLE iam_access_keys;

                    ALTER TABLE iam_access_keys_new RENAME TO iam_access_keys;
                    "#,
                )?;
            }
        }

        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (2, datetime('now'))",
            [],
        )?;
    }

    // Create remaining tables (idempotent)
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_access_keys_username ON iam_access_keys(username);

        CREATE TABLE IF NOT EXISTS iam_user_policies (
            username TEXT NOT NULL,
            policy_name TEXT NOT NULL,
            policy_document TEXT NOT NULL,
            PRIMARY KEY (username, policy_name),
            FOREIGN KEY (username) REFERENCES iam_users(username) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS bucket_policies (
            bucket TEXT PRIMARY KEY,
            policy_document TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS iam_groups (
            name TEXT PRIMARY KEY,
            arn TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS iam_group_members (
            group_name TEXT NOT NULL,
            username TEXT NOT NULL,
            PRIMARY KEY (group_name, username),
            FOREIGN KEY (group_name) REFERENCES iam_groups(name) ON DELETE CASCADE,
            FOREIGN KEY (username) REFERENCES iam_users(username) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_group_members_username ON iam_group_members(username);

        CREATE TABLE IF NOT EXISTS iam_group_policies (
            group_name TEXT NOT NULL,
            policy_name TEXT NOT NULL,
            policy_document TEXT NOT NULL,
            PRIMARY KEY (group_name, policy_name),
            FOREIGN KEY (group_name) REFERENCES iam_groups(name) ON DELETE CASCADE
        );

        -- Standalone IAM policies (managed policies)
        CREATE TABLE IF NOT EXISTS iam_policies (
            name TEXT PRIMARY KEY,
            description TEXT,
            policy_document TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- STS temporary credentials (session_token stored as hash only for security)
        CREATE TABLE IF NOT EXISTS iam_temporary_credentials (
            access_key_id TEXT PRIMARY KEY,
            secret_access_key_encrypted TEXT NOT NULL,
            session_token_hash TEXT NOT NULL UNIQUE,
            assumed_identity TEXT NOT NULL,
            expiration TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (assumed_identity) REFERENCES iam_users(username) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_temp_creds_expiration ON iam_temporary_credentials(expiration);
        CREATE INDEX IF NOT EXISTS idx_temp_creds_session_hash ON iam_temporary_credentials(session_token_hash);

        -- Rate limiting for login attempts (persistent across restarts)
        CREATE TABLE IF NOT EXISTS rate_limits (
            key TEXT PRIMARY KEY,
            attempts INTEGER NOT NULL DEFAULT 0,
            first_attempt_at TEXT NOT NULL,
            locked_until TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_rate_limits_locked ON rate_limits(locked_until);

        -- OIDC/SSO providers (client_secret stored encrypted at rest)
        CREATE TABLE IF NOT EXISTS oidc_providers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            issuer_url TEXT NOT NULL,
            client_id TEXT NOT NULL,
            client_secret_enc TEXT NOT NULL,
            redirect_uri TEXT NOT NULL,
            scopes TEXT NOT NULL,
            username_claim TEXT NOT NULL,
            groups_claim TEXT,
            auto_create_users INTEGER NOT NULL DEFAULT 1,
            default_policy TEXT,
            group_policy_mappings TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- SMTP relay configuration (single row; password stored encrypted at rest)
        CREATE TABLE IF NOT EXISTS smtp_config (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            enabled INTEGER NOT NULL DEFAULT 0,
            host TEXT NOT NULL DEFAULT '',
            port INTEGER NOT NULL DEFAULT 587,
            username TEXT NOT NULL DEFAULT '',
            password_enc TEXT NOT NULL DEFAULT '',
            from_address TEXT NOT NULL DEFAULT '',
            from_name TEXT,
            use_starttls INTEGER NOT NULL DEFAULT 1,
            alert_on_delivery_failure INTEGER NOT NULL DEFAULT 0,
            send_usage_reports INTEGER NOT NULL DEFAULT 0,
            usage_report_schedule TEXT NOT NULL DEFAULT 'weekly',
            alert_on_audit_events INTEGER NOT NULL DEFAULT 0,
            alert_recipients TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL
        );
        "#,
    )?;
    Ok(())
}

#[async_trait]
impl IamProvider for IamStore {
    async fn create_user(&self, username: &str) -> Result<User> {
        if username == "root" || username == self.root_access_key {
            return Err(IamError::UserExists(username.to_string()));
        }

        let user = User::new(username.to_string());
        let user_clone = user.clone();

        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO iam_users (username, arn, created_at, status, is_root) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        user_clone.username,
                        user_clone.arn,
                        user_clone.created_at.to_rfc3339(),
                        user_clone.status.as_str(),
                        user_clone.is_root as i32,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| {
                if e.to_string().contains("UNIQUE constraint") {
                    IamError::UserExists(username.to_string())
                } else {
                    IamError::Database(e.to_string())
                }
            })?;

        Ok(user)
    }

    async fn delete_user(&self, username: &str) -> Result<()> {
        if username == "root" || username == self.root_access_key {
            return Err(IamError::CannotDeleteRoot);
        }

        let username = username.to_string();
        let username_for_err = username.clone();
        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute(
                    "DELETE FROM iam_users WHERE username = ?1",
                    params![username],
                )?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::UserNotFound(username_for_err));
        }

        Ok(())
    }

    async fn get_user(&self, username: &str) -> Result<User> {
        // Check for root user (may be configured with custom username)
        if username == "root" || username == self.root_access_key {
            return Ok(User::root_with_username(&self.root_access_key));
        }

        let username = username.to_string();
        self.db
            .call(move |conn| {
                let user = conn
                    .query_row(
                        "SELECT username, arn, created_at, status, is_root, password_hash FROM iam_users WHERE username = ?1",
                        params![username],
                        |row| {
                            let created_at: String = row.get(2)?;
                            let status: String = row.get(3)?;
                            Ok(User {
                                username: row.get(0)?,
                                arn: row.get(1)?,
                                created_at: DateTime::parse_from_rfc3339(&created_at)
                                    .map(|dt| dt.with_timezone(&Utc))
                                    .unwrap_or_else(|_| Utc::now()),
                                status: status.parse().unwrap_or(UserStatus::Inactive),
                                is_root: row.get::<_, i32>(4)? != 0,
                                password_hash: row.get(5)?,
                            })
                        },
                    )
                    .optional()?;

                match user {
                    Some(u) => Ok(u),
                    None => Err(tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows)),
                }
            })
            .await
            .map_err(|e| {
                if matches!(e, tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows)) {
                    IamError::UserNotFound("unknown".to_string())
                } else {
                    IamError::Database(e.to_string())
                }
            })
    }

    async fn set_user_password(&self, username: &str, password: &str) -> Result<()> {
        // Validate password strength and hash with argon2id
        let hash = crate::password::validate_and_hash_password(password)?;

        let username = username.to_string();
        let username_for_err = username.clone();

        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute(
                    "UPDATE iam_users SET password_hash = ?1 WHERE username = ?2",
                    params![hash, username],
                )?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::UserNotFound(username_for_err));
        }

        Ok(())
    }

    async fn verify_user_password(&self, username: &str, password: &str) -> Result<bool> {
        // Root user authenticates directly against the configured root password
        // root_access_key holds the username from STRIX_ROOT_USER
        // root_secret_key holds the password from STRIX_ROOT_PASSWORD
        if username == self.root_access_key {
            return Ok(password == self.root_secret_key);
        }

        let user = self.get_user(username).await?;

        match user.password_hash {
            Some(hash) => crate::password::verify_password(password, &hash),
            None => Ok(false), // No password set
        }
    }

    async fn list_users(&self) -> Result<Vec<User>> {
        self.db
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT username, arn, created_at, status, is_root, password_hash FROM iam_users ORDER BY username",
                )?;
                let users = stmt
                    .query_map([], |row| {
                        let created_at: String = row.get(2)?;
                        let status: String = row.get(3)?;
                        Ok(User {
                            username: row.get(0)?,
                            arn: row.get(1)?,
                            created_at: DateTime::parse_from_rfc3339(&created_at)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|_| Utc::now()),
                            status: status.parse().unwrap_or(UserStatus::Inactive),
                            is_root: row.get::<_, i32>(4)? != 0,
                            password_hash: row.get(5)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(users)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    async fn update_user_status(&self, username: &str, status: UserStatus) -> Result<()> {
        if username == "root" || username == self.root_access_key {
            return Err(IamError::CannotDeleteRoot);
        }

        let username = username.to_string();
        let username_for_err = username.clone();
        let status_str = status.as_str().to_string();

        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute(
                    "UPDATE iam_users SET status = ?1 WHERE username = ?2",
                    params![status_str, username],
                )?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::UserNotFound(username_for_err));
        }

        Ok(())
    }

    async fn create_access_key(&self, username: &str) -> Result<AccessKey> {
        if username == "root" || username == self.root_access_key {
            return Err(IamError::CannotModifyRootKeys);
        }

        // Check user exists
        let _ = self.get_user(username).await?;

        // Check max keys (2 per user like AWS)
        let existing = self.list_access_keys(username).await?;
        if existing.len() >= 2 {
            return Err(IamError::MaxAccessKeysExceeded);
        }

        let secret = generate_secret_key();
        let encrypted_secret = self.encrypt_secret(&secret)?;

        let access_key = AccessKey {
            access_key_id: generate_access_key_id(),
            secret_access_key: Some(secret), // Only returned this one time
            username: username.to_string(),
            created_at: Utc::now(),
            status: AccessKeyStatus::Active,
            last_used: None,
        };

        let key_id = access_key.access_key_id.clone();
        let key_username = access_key.username.clone();
        let key_created_at = access_key.created_at.to_rfc3339();
        let key_status = access_key.status.as_str().to_string();

        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO iam_access_keys (access_key_id, secret_access_key_encrypted, username, created_at, status) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        key_id,
                        encrypted_secret,
                        key_username,
                        key_created_at,
                        key_status,
                    ],
                )?;
                Ok(())
            })
            .await?;

        Ok(access_key)
    }

    async fn delete_access_key(&self, access_key_id: &str) -> Result<()> {
        // Check if it's a root key
        if access_key_id == self.root_access_key {
            return Err(IamError::CannotModifyRootKeys);
        }

        let access_key_id = access_key_id.to_string();
        let access_key_id_for_err = access_key_id.clone();
        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute(
                    "DELETE FROM iam_access_keys WHERE access_key_id = ?1",
                    params![access_key_id],
                )?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::AccessKeyNotFound(access_key_id_for_err));
        }

        Ok(())
    }

    async fn list_access_keys(&self, username: &str) -> Result<Vec<AccessKey>> {
        if username == "root" || username == self.root_access_key {
            // Return the root access key (without secret)
            return Ok(vec![AccessKey {
                access_key_id: self.root_access_key.clone(),
                secret_access_key: None,
                username: self.root_access_key.clone(),
                created_at: Utc::now(),
                status: AccessKeyStatus::Active,
                last_used: None,
            }]);
        }

        let username = username.to_string();
        self.db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT access_key_id, username, created_at, status, last_used FROM iam_access_keys WHERE username = ?1 ORDER BY created_at",
                )?;
                let keys = stmt
                    .query_map(params![username], |row| {
                        let created_at: String = row.get(2)?;
                        let status: String = row.get(3)?;
                        let last_used: Option<String> = row.get(4)?;
                        Ok(AccessKey {
                            access_key_id: row.get(0)?,
                            secret_access_key: None, // Never return secret after creation
                            username: row.get(1)?,
                            created_at: DateTime::parse_from_rfc3339(&created_at)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|_| Utc::now()),
                            status: status.parse().unwrap_or(AccessKeyStatus::Inactive),
                            last_used: last_used.and_then(|s| {
                                DateTime::parse_from_rfc3339(&s)
                                    .map(|dt| dt.with_timezone(&Utc))
                                    .ok()
                            }),
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(keys)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    async fn get_access_key(&self, access_key_id: &str) -> Result<AccessKey> {
        if access_key_id == self.root_access_key {
            return Ok(AccessKey {
                access_key_id: self.root_access_key.clone(),
                secret_access_key: None,
                username: "root".to_string(),
                created_at: Utc::now(),
                status: AccessKeyStatus::Active,
                last_used: None,
            });
        }

        let access_key_id = access_key_id.to_string();
        self.db
            .call(move |conn| {
                let key = conn
                    .query_row(
                        "SELECT access_key_id, username, created_at, status, last_used FROM iam_access_keys WHERE access_key_id = ?1",
                        params![access_key_id],
                        |row| {
                            let created_at: String = row.get(2)?;
                            let status: String = row.get(3)?;
                            let last_used: Option<String> = row.get(4)?;
                            Ok(AccessKey {
                                access_key_id: row.get(0)?,
                                secret_access_key: None,
                                username: row.get(1)?,
                                created_at: DateTime::parse_from_rfc3339(&created_at)
                                    .map(|dt| dt.with_timezone(&Utc))
                                    .unwrap_or_else(|_| Utc::now()),
                                status: status.parse().unwrap_or(AccessKeyStatus::Inactive),
                                last_used: last_used.and_then(|s| {
                                    DateTime::parse_from_rfc3339(&s)
                                        .map(|dt| dt.with_timezone(&Utc))
                                        .ok()
                                }),
                            })
                        },
                    )
                    .optional()?;

                match key {
                    Some(k) => Ok(k),
                    None => Err(tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows)),
                }
            })
            .await
            .map_err(|e| {
                if matches!(e, tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows)) {
                    IamError::AccessKeyNotFound("unknown".to_string())
                } else {
                    IamError::Database(e.to_string())
                }
            })
    }

    async fn update_access_key_status(
        &self,
        access_key_id: &str,
        status: AccessKeyStatus,
    ) -> Result<()> {
        if access_key_id == self.root_access_key {
            return Err(IamError::CannotModifyRootKeys);
        }

        let access_key_id = access_key_id.to_string();
        let access_key_id_for_err = access_key_id.clone();
        let status_str = status.as_str().to_string();

        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute(
                    "UPDATE iam_access_keys SET status = ?1 WHERE access_key_id = ?2",
                    params![status_str, access_key_id],
                )?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::AccessKeyNotFound(access_key_id_for_err));
        }

        Ok(())
    }

    async fn get_credentials(&self, access_key_id: &str) -> Result<Option<(AccessKey, User)>> {
        // Check root first
        if access_key_id == self.root_access_key {
            let key = AccessKey {
                access_key_id: self.root_access_key.clone(),
                secret_access_key: Some(self.root_secret_key.clone()),
                username: "root".to_string(),
                created_at: Utc::now(),
                status: AccessKeyStatus::Active,
                last_used: None,
            };
            return Ok(Some((key, User::root())));
        }

        let access_key_id = access_key_id.to_string();
        let result = self.db
            .call(move |conn| {
                let result = conn
                    .query_row(
                        r#"
                        SELECT
                            k.access_key_id, k.secret_access_key_encrypted, k.username, k.created_at, k.status, k.last_used,
                            u.arn, u.created_at, u.status, u.is_root, u.password_hash
                        FROM iam_access_keys k
                        JOIN iam_users u ON k.username = u.username
                        WHERE k.access_key_id = ?1 AND k.status = 'active' AND u.status = 'active'
                        "#,
                        params![access_key_id],
                        |row| {
                            let encrypted_secret: String = row.get(1)?;
                            let key_created_at: String = row.get(3)?;
                            let key_status: String = row.get(4)?;
                            let key_last_used: Option<String> = row.get(5)?;
                            let user_created_at: String = row.get(7)?;
                            let user_status: String = row.get(8)?;
                            let password_hash: Option<String> = row.get(10)?;

                            Ok((
                                row.get::<_, String>(0)?,  // access_key_id
                                encrypted_secret,
                                row.get::<_, String>(2)?,  // username
                                key_created_at,
                                key_status,
                                key_last_used,
                                row.get::<_, String>(6)?,  // user arn
                                user_created_at,
                                user_status,
                                row.get::<_, i32>(9)?,     // is_root
                                password_hash,
                            ))
                        },
                    )
                    .optional()?;

                Ok(result)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))?;

        // Decrypt the secret outside the closure
        match result {
            Some((
                key_id,
                encrypted_secret,
                username,
                key_created_at,
                key_status,
                key_last_used,
                user_arn,
                user_created_at,
                user_status,
                is_root,
                password_hash,
            )) => {
                let decrypted_secret = self.decrypt_secret(&encrypted_secret)?;

                let key = AccessKey {
                    access_key_id: key_id,
                    secret_access_key: Some(decrypted_secret),
                    username: username.clone(),
                    created_at: DateTime::parse_from_rfc3339(&key_created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    status: key_status.parse().unwrap_or(AccessKeyStatus::Active),
                    last_used: key_last_used.and_then(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .ok()
                    }),
                };

                let user = User {
                    username,
                    arn: user_arn,
                    created_at: DateTime::parse_from_rfc3339(&user_created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    status: user_status.parse().unwrap_or(UserStatus::Active),
                    is_root: is_root != 0,
                    password_hash,
                };

                Ok(Some((key, user)))
            }
            None => Ok(None),
        }
    }

    async fn update_access_key_last_used(&self, access_key_id: &str) -> Result<()> {
        if access_key_id == self.root_access_key {
            return Ok(()); // Root key last_used not tracked
        }

        let access_key_id = access_key_id.to_string();
        let now = Utc::now().to_rfc3339();

        self.db
            .call(move |conn| {
                conn.execute(
                    "UPDATE iam_access_keys SET last_used = ?1 WHERE access_key_id = ?2",
                    params![now, access_key_id],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    async fn attach_user_policy(&self, username: &str, policy: &Policy) -> Result<()> {
        if username == "root" || username == self.root_access_key {
            return Ok(()); // Root always has full access
        }

        // Check user exists
        let _ = self.get_user(username).await?;

        let username = username.to_string();
        let policy_name = policy.name.clone();
        let policy_doc =
            serde_json::to_string(policy).map_err(|e| IamError::InvalidPolicy(e.to_string()))?;

        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO iam_user_policies (username, policy_name, policy_document) VALUES (?1, ?2, ?3)",
                    params![username, policy_name, policy_doc],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    async fn detach_user_policy(&self, username: &str, policy_name: &str) -> Result<()> {
        let username = username.to_string();
        let policy_name = policy_name.to_string();
        let policy_name_for_err = policy_name.clone();

        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute(
                    "DELETE FROM iam_user_policies WHERE username = ?1 AND policy_name = ?2",
                    params![username, policy_name],
                )?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::PolicyNotFound(policy_name_for_err));
        }

        Ok(())
    }

    async fn list_user_policies(&self, username: &str) -> Result<Vec<Policy>> {
        if username == "root" || username == self.root_access_key {
            return Ok(vec![crate::policy::admin_policy()]);
        }

        let username = username.to_string();
        self.db
            .call(move |conn| {
                let mut stmt = conn
                    .prepare("SELECT policy_document FROM iam_user_policies WHERE username = ?1")?;
                let policies = stmt
                    .query_map(params![username], |row| {
                        let doc: String = row.get(0)?;
                        Ok(doc)
                    })?
                    .filter_map(|r| r.ok())
                    .filter_map(|doc| serde_json::from_str(&doc).ok())
                    .collect::<Vec<Policy>>();
                Ok(policies)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    async fn is_authorized(
        &self,
        username: &str,
        action: &Action,
        resource: &Resource,
    ) -> Result<bool> {
        // Root is always authorized
        if username == "root" || username == self.root_access_key {
            return Ok(true);
        }

        // Collect all policies: user policies + group policies
        let mut all_policies = self.list_user_policies(username).await?;

        // Get user's groups and their policies
        let groups = self.list_user_groups(username).await?;
        for group in groups {
            let group_policies = self.list_group_policies(&group.name).await?;
            all_policies.extend(group_policies);
        }

        // Evaluate policies (deny takes precedence per AWS IAM semantics)
        let mut allowed = false;
        for policy in &all_policies {
            match policy.evaluate(action, resource) {
                Some(Effect::Deny) => return Ok(false),
                Some(Effect::Allow) => allowed = true,
                None => {}
            }
        }

        Ok(allowed)
    }

    async fn is_authorized_detailed(
        &self,
        username: &str,
        action: &Action,
        resource: &Resource,
    ) -> Result<AuthorizationResult> {
        // Root is always authorized
        if username == "root" || username == self.root_access_key {
            return Ok(AuthorizationResult {
                allowed: true,
                effect: AuthorizationEffect::RootAccess,
                matched_policy: None,
                matched_statement: None,
                policy_source: None,
            });
        }

        // Collect user policies with source tracking
        let user_policies = self.list_user_policies(username).await?;

        // First pass: check for explicit denies (deny takes precedence)
        for policy in &user_policies {
            if let Some(Effect::Deny) = policy.evaluate(action, resource) {
                return Ok(AuthorizationResult {
                    allowed: false,
                    effect: AuthorizationEffect::ExplicitDeny,
                    matched_policy: Some(policy.name.clone()),
                    matched_statement: None,
                    policy_source: Some(PolicySource::User(username.to_string())),
                });
            }
        }

        // Check group policies for denies
        let groups = self.list_user_groups(username).await?;
        for group in &groups {
            let group_policies = self.list_group_policies(&group.name).await?;
            for policy in &group_policies {
                if let Some(Effect::Deny) = policy.evaluate(action, resource) {
                    return Ok(AuthorizationResult {
                        allowed: false,
                        effect: AuthorizationEffect::ExplicitDeny,
                        matched_policy: Some(policy.name.clone()),
                        matched_statement: None,
                        policy_source: Some(PolicySource::Group(group.name.clone())),
                    });
                }
            }
        }

        // Second pass: check for explicit allows
        for policy in &user_policies {
            if let Some(Effect::Allow) = policy.evaluate(action, resource) {
                return Ok(AuthorizationResult {
                    allowed: true,
                    effect: AuthorizationEffect::ExplicitAllow,
                    matched_policy: Some(policy.name.clone()),
                    matched_statement: None,
                    policy_source: Some(PolicySource::User(username.to_string())),
                });
            }
        }

        for group in &groups {
            let group_policies = self.list_group_policies(&group.name).await?;
            for policy in &group_policies {
                if let Some(Effect::Allow) = policy.evaluate(action, resource) {
                    return Ok(AuthorizationResult {
                        allowed: true,
                        effect: AuthorizationEffect::ExplicitAllow,
                        matched_policy: Some(policy.name.clone()),
                        matched_statement: None,
                        policy_source: Some(PolicySource::Group(group.name.clone())),
                    });
                }
            }
        }

        // No matching allow - implicit deny
        Ok(AuthorizationResult {
            allowed: false,
            effect: AuthorizationEffect::ImplicitDeny,
            matched_policy: None,
            matched_statement: None,
            policy_source: None,
        })
    }

    async fn set_bucket_policy(&self, bucket: &str, policy: &BucketPolicy) -> Result<()> {
        let bucket = bucket.to_string();
        let policy_doc =
            serde_json::to_string(policy).map_err(|e| IamError::InvalidPolicy(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        let now_clone = now.clone();

        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO bucket_policies (bucket, policy_document, created_at, updated_at) VALUES (?1, ?2, COALESCE((SELECT created_at FROM bucket_policies WHERE bucket = ?1), ?3), ?4)",
                    params![bucket, policy_doc, now, now_clone],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    async fn get_bucket_policy(&self, bucket: &str) -> Result<Option<BucketPolicy>> {
        let bucket = bucket.to_string();
        self.db
            .call(move |conn| {
                let result = conn
                    .query_row(
                        "SELECT policy_document FROM bucket_policies WHERE bucket = ?1",
                        params![bucket],
                        |row| {
                            let doc: String = row.get(0)?;
                            Ok(doc)
                        },
                    )
                    .optional()?;

                match result {
                    Some(doc) => {
                        let policy: BucketPolicy = serde_json::from_str(&doc)
                            .map_err(|e| tokio_rusqlite::Error::Other(Box::new(e)))?;
                        Ok(Some(policy))
                    }
                    None => Ok(None),
                }
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    async fn delete_bucket_policy(&self, bucket: &str) -> Result<()> {
        let bucket = bucket.to_string();
        let bucket_for_err = bucket.clone();

        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute(
                    "DELETE FROM bucket_policies WHERE bucket = ?1",
                    params![bucket],
                )?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::PolicyNotFound(format!(
                "Bucket policy for '{}'",
                bucket_for_err
            )));
        }

        Ok(())
    }

    async fn is_authorized_by_bucket_policy(
        &self,
        bucket: &str,
        principal: &Principal,
        action: &Action,
        resource: &Resource,
    ) -> Result<Option<Effect>> {
        let policy = self.get_bucket_policy(bucket).await?;

        match policy {
            Some(p) => Ok(p.evaluate(principal, action, resource)),
            None => Ok(None),
        }
    }

    // === Group Operations ===

    async fn create_group(&self, name: &str) -> Result<Group> {
        let group = Group::new(name.to_string());
        let group_clone = group.clone();

        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO iam_groups (name, arn, created_at) VALUES (?1, ?2, ?3)",
                    params![
                        group_clone.name,
                        group_clone.arn,
                        group_clone.created_at.to_rfc3339(),
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| {
                if e.to_string().contains("UNIQUE constraint") {
                    IamError::GroupExists(name.to_string())
                } else {
                    IamError::Database(e.to_string())
                }
            })?;

        Ok(group)
    }

    async fn delete_group(&self, name: &str) -> Result<()> {
        let name = name.to_string();
        let name_for_err = name.clone();

        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute("DELETE FROM iam_groups WHERE name = ?1", params![name])?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::GroupNotFound(name_for_err));
        }

        Ok(())
    }

    async fn get_group(&self, name: &str) -> Result<Group> {
        let name = name.to_string();
        let name_clone = name.clone();

        self.db
            .call(move |conn| {
                // Get group info
                let group_row = conn
                    .query_row(
                        "SELECT name, arn, created_at FROM iam_groups WHERE name = ?1",
                        params![name],
                        |row| {
                            let created_at: String = row.get(2)?;
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                created_at,
                            ))
                        },
                    )
                    .optional()?;

                match group_row {
                    Some((group_name, arn, created_at)) => {
                        // Get members
                        let mut stmt = conn.prepare(
                            "SELECT username FROM iam_group_members WHERE group_name = ?1",
                        )?;
                        let members: Vec<String> = stmt
                            .query_map(params![name_clone], |row| row.get(0))?
                            .filter_map(|r| r.ok())
                            .collect();

                        // Get policies
                        let mut stmt = conn.prepare(
                            "SELECT policy_name FROM iam_group_policies WHERE group_name = ?1",
                        )?;
                        let policies: Vec<String> = stmt
                            .query_map(params![name_clone], |row| row.get(0))?
                            .filter_map(|r| r.ok())
                            .collect();

                        Ok(Group {
                            name: group_name,
                            arn,
                            created_at: DateTime::parse_from_rfc3339(&created_at)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|_| Utc::now()),
                            members,
                            policies,
                        })
                    }
                    None => Err(tokio_rusqlite::Error::Rusqlite(
                        rusqlite::Error::QueryReturnedNoRows,
                    )),
                }
            })
            .await
            .map_err(|e| {
                if matches!(
                    e,
                    tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows)
                ) {
                    IamError::GroupNotFound("unknown".to_string())
                } else {
                    IamError::Database(e.to_string())
                }
            })
    }

    async fn list_groups(&self) -> Result<Vec<Group>> {
        self.db
            .call(|conn| {
                let mut stmt =
                    conn.prepare("SELECT name, arn, created_at FROM iam_groups ORDER BY name")?;
                let groups: Vec<(String, String, String)> = stmt
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                    .filter_map(|r| r.ok())
                    .collect();

                let mut result = Vec::new();
                for (name, arn, created_at) in groups {
                    // Get members
                    let mut stmt = conn
                        .prepare("SELECT username FROM iam_group_members WHERE group_name = ?1")?;
                    let members: Vec<String> = stmt
                        .query_map(params![name], |row| row.get(0))?
                        .filter_map(|r| r.ok())
                        .collect();

                    // Get policies
                    let mut stmt = conn.prepare(
                        "SELECT policy_name FROM iam_group_policies WHERE group_name = ?1",
                    )?;
                    let policies: Vec<String> = stmt
                        .query_map(params![name], |row| row.get(0))?
                        .filter_map(|r| r.ok())
                        .collect();

                    result.push(Group {
                        name,
                        arn,
                        created_at: DateTime::parse_from_rfc3339(&created_at)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                        members,
                        policies,
                    });
                }
                Ok(result)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    async fn add_user_to_group(&self, group_name: &str, username: &str) -> Result<()> {
        // Check group exists
        let _ = self.get_group(group_name).await?;
        // Check user exists
        let _ = self.get_user(username).await?;

        let group_name = group_name.to_string();
        let username = username.to_string();

        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO iam_group_members (group_name, username) VALUES (?1, ?2)",
                    params![group_name, username],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    async fn remove_user_from_group(&self, group_name: &str, username: &str) -> Result<()> {
        let group_name = group_name.to_string();
        let username = username.to_string();

        self.db
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM iam_group_members WHERE group_name = ?1 AND username = ?2",
                    params![group_name, username],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    async fn attach_group_policy(&self, group_name: &str, policy: &Policy) -> Result<()> {
        // Check group exists
        let _ = self.get_group(group_name).await?;

        let group_name = group_name.to_string();
        let policy_name = policy.name.clone();
        let policy_doc =
            serde_json::to_string(policy).map_err(|e| IamError::InvalidPolicy(e.to_string()))?;

        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO iam_group_policies (group_name, policy_name, policy_document) VALUES (?1, ?2, ?3)",
                    params![group_name, policy_name, policy_doc],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    async fn detach_group_policy(&self, group_name: &str, policy_name: &str) -> Result<()> {
        let group_name = group_name.to_string();
        let policy_name = policy_name.to_string();
        let policy_name_for_err = policy_name.clone();

        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute(
                    "DELETE FROM iam_group_policies WHERE group_name = ?1 AND policy_name = ?2",
                    params![group_name, policy_name],
                )?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::PolicyNotFound(policy_name_for_err));
        }

        Ok(())
    }

    async fn list_group_policies(&self, group_name: &str) -> Result<Vec<Policy>> {
        let group_name = group_name.to_string();

        self.db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT policy_document FROM iam_group_policies WHERE group_name = ?1",
                )?;
                let policies = stmt
                    .query_map(params![group_name], |row| {
                        let doc: String = row.get(0)?;
                        Ok(doc)
                    })?
                    .filter_map(|r| r.ok())
                    .filter_map(|doc| serde_json::from_str(&doc).ok())
                    .collect::<Vec<Policy>>();
                Ok(policies)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    async fn list_user_groups(&self, username: &str) -> Result<Vec<Group>> {
        let username = username.to_string();

        self.db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT g.name, g.arn, g.created_at
                    FROM iam_groups g
                    JOIN iam_group_members m ON g.name = m.group_name
                    WHERE m.username = ?1
                    ORDER BY g.name
                    "#,
                )?;
                let groups: Vec<(String, String, String)> = stmt
                    .query_map(params![username], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();

                let mut result = Vec::new();
                for (name, arn, created_at) in groups {
                    // Get members
                    let mut stmt = conn
                        .prepare("SELECT username FROM iam_group_members WHERE group_name = ?1")?;
                    let members: Vec<String> = stmt
                        .query_map(params![name], |row| row.get(0))?
                        .filter_map(|r| r.ok())
                        .collect();

                    // Get policies
                    let mut stmt = conn.prepare(
                        "SELECT policy_name FROM iam_group_policies WHERE group_name = ?1",
                    )?;
                    let policies: Vec<String> = stmt
                        .query_map(params![name], |row| row.get(0))?
                        .filter_map(|r| r.ok())
                        .collect();

                    result.push(Group {
                        name,
                        arn,
                        created_at: DateTime::parse_from_rfc3339(&created_at)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                        members,
                        policies,
                    });
                }
                Ok(result)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    // === Standalone Policy Operations ===

    async fn create_policy(&self, policy: &Policy, description: Option<&str>) -> Result<()> {
        let policy_name = policy.name.clone();
        let policy_doc =
            serde_json::to_string(policy).map_err(|e| IamError::InvalidPolicy(e.to_string()))?;
        let description = description.map(|s| s.to_string());
        let now = Utc::now().to_rfc3339();
        let now_clone = now.clone();

        self.db
            .call(move |conn| {
                conn.execute(
                    r#"
                    INSERT INTO iam_policies (name, description, policy_document, created_at, updated_at)
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT(name) DO UPDATE SET
                        description = excluded.description,
                        policy_document = excluded.policy_document,
                        updated_at = excluded.updated_at
                    "#,
                    params![policy_name, description, policy_doc, now, now_clone],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    async fn delete_policy(&self, policy_name: &str) -> Result<()> {
        let policy_name = policy_name.to_string();
        let policy_name_for_err = policy_name.clone();

        let rows = self
            .db
            .call(move |conn| {
                let rows = conn.execute(
                    "DELETE FROM iam_policies WHERE name = ?1",
                    params![policy_name],
                )?;
                Ok(rows)
            })
            .await?;

        if rows == 0 {
            return Err(IamError::PolicyNotFound(policy_name_for_err));
        }

        Ok(())
    }

    async fn get_policy(&self, policy_name: &str) -> Result<Policy> {
        let policy_name = policy_name.to_string();

        self.db
            .call(move |conn| {
                let result = conn
                    .query_row(
                        "SELECT policy_document FROM iam_policies WHERE name = ?1",
                        params![policy_name],
                        |row| {
                            let doc: String = row.get(0)?;
                            Ok(doc)
                        },
                    )
                    .optional()?;

                match result {
                    Some(doc) => {
                        let policy: Policy = serde_json::from_str(&doc)
                            .map_err(|e| tokio_rusqlite::Error::Other(Box::new(e)))?;
                        Ok(policy)
                    }
                    None => Err(tokio_rusqlite::Error::Rusqlite(
                        rusqlite::Error::QueryReturnedNoRows,
                    )),
                }
            })
            .await
            .map_err(|e| {
                if matches!(
                    e,
                    tokio_rusqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows)
                ) {
                    IamError::PolicyNotFound("unknown".to_string())
                } else {
                    IamError::Database(e.to_string())
                }
            })
    }

    async fn list_policies(&self) -> Result<Vec<(Policy, Option<String>)>> {
        self.db
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT policy_document, description FROM iam_policies ORDER BY name",
                )?;
                let policies = stmt
                    .query_map([], |row| {
                        let doc: String = row.get(0)?;
                        let description: Option<String> = row.get(1)?;
                        Ok((doc, description))
                    })?
                    .filter_map(|r| r.ok())
                    .filter_map(|(doc, desc)| {
                        serde_json::from_str::<Policy>(&doc).ok().map(|p| (p, desc))
                    })
                    .collect::<Vec<_>>();
                Ok(policies)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    // === STS Operations ===

    async fn assume_role(&self, request: AssumeRoleRequest) -> Result<TemporaryCredential> {
        // Verify user exists and is active
        let user = self.get_user(&request.username).await?;
        if user.status != UserStatus::Active {
            return Err(IamError::InvalidCredentials);
        }

        // Generate credentials
        let access_key_id = crate::generate_temp_access_key_id();
        let secret_access_key = crate::generate_secret_key();
        let session_token = crate::generate_session_token();

        // Hash session token for storage (we store the hash, compare on lookup)
        let session_token_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(session_token.as_bytes());
            hex::encode(hasher.finalize())
        };

        // Duration: default 3600, min 900, max 43200
        let duration = request.duration_seconds.unwrap_or(3600).clamp(900, 43200);
        let expiration = Utc::now() + chrono::Duration::seconds(duration as i64);

        // Encrypt the secret
        let encrypted_secret = self.encrypt_secret(&secret_access_key)?;

        let cred = TemporaryCredential {
            access_key_id: access_key_id.clone(),
            secret_access_key: Some(secret_access_key),
            session_token: session_token.clone(),
            expiration,
            assumed_identity: request.username.clone(),
        };

        // Store in database (session token stored as hash only for security)
        let access_key_id_db = access_key_id.clone();
        let assumed_identity = request.username.clone();
        let expiration_str = expiration.to_rfc3339();

        self.db
            .call(move |conn| {
                conn.execute(
                    r#"INSERT INTO iam_temporary_credentials
                       (access_key_id, secret_access_key_encrypted, session_token_hash,
                        assumed_identity, expiration)
                       VALUES (?1, ?2, ?3, ?4, ?5)"#,
                    params![
                        access_key_id_db,
                        encrypted_secret,
                        session_token_hash,
                        assumed_identity,
                        expiration_str,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))?;

        Ok(cred)
    }

    async fn get_temp_credentials(
        &self,
        access_key_id: &str,
    ) -> Result<Option<(TemporaryCredential, User)>> {
        let access_key_id = access_key_id.to_string();
        let now = Utc::now().to_rfc3339();
        let encryption_key = self.encryption_key;

        self.db
            .call(move |conn| {
                // Note: session_token is not stored in plaintext for security.
                // The client must provide the token which is validated via hash comparison.
                let result = conn
                    .query_row(
                        r#"SELECT tc.access_key_id, tc.secret_access_key_encrypted,
                                  tc.expiration, tc.assumed_identity,
                                  u.username, u.arn, u.created_at, u.status, u.is_root, u.password_hash
                           FROM iam_temporary_credentials tc
                           JOIN iam_users u ON tc.assumed_identity = u.username
                           WHERE tc.access_key_id = ?1 AND tc.expiration > ?2"#,
                        params![access_key_id, now],
                        |row| {
                            let access_key_id: String = row.get(0)?;
                            let secret_encrypted: String = row.get(1)?;
                            let expiration_str: String = row.get(2)?;
                            let assumed_identity: String = row.get(3)?;
                            let username: String = row.get(4)?;
                            let arn: String = row.get(5)?;
                            let created_at_str: String = row.get(6)?;
                            let status_str: String = row.get(7)?;
                            let is_root: i32 = row.get(8)?;
                            let password_hash: Option<String> = row.get(9)?;
                            Ok((
                                access_key_id,
                                secret_encrypted,
                                expiration_str,
                                assumed_identity,
                                username,
                                arn,
                                created_at_str,
                                status_str,
                                is_root,
                                password_hash,
                            ))
                        },
                    )
                    .optional()?;

                match result {
                    Some((
                        ak_id,
                        secret_enc,
                        exp_str,
                        identity,
                        username,
                        arn,
                        created_at_str,
                        status_str,
                        is_root,
                        password_hash,
                    )) => {
                        // Decrypt secret
                        let secret = crate::secrets::decrypt_secret(&secret_enc, &encryption_key)
                            .map_err(|e| {
                                tokio_rusqlite::Error::Other(Box::new(std::io::Error::other(
                                    e.to_string(),
                                )))
                            })?;

                        let expiration = DateTime::parse_from_rfc3339(&exp_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());

                        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());

                        let status = status_str
                            .parse::<UserStatus>()
                            .unwrap_or(UserStatus::Inactive);

                        // Note: session_token is empty here since we don't store plaintext.
                        // Token validation is done separately via validate_session_token.
                        let cred = TemporaryCredential {
                            access_key_id: ak_id,
                            secret_access_key: Some(secret),
                            session_token: String::new(), // Not stored in DB for security
                            expiration,
                            assumed_identity: identity,
                        };

                        let user = User {
                            username,
                            arn,
                            created_at,
                            status,
                            is_root: is_root != 0,
                            password_hash,
                        };

                        Ok(Some((cred, user)))
                    }
                    None => Ok(None),
                }
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    async fn validate_session_token(
        &self,
        access_key_id: &str,
        session_token: &str,
    ) -> Result<bool> {
        // Hash the provided token and compare
        let token_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(session_token.as_bytes());
            hex::encode(hasher.finalize())
        };

        let access_key_id = access_key_id.to_string();
        let now = Utc::now().to_rfc3339();

        self.db
            .call(move |conn| {
                let result = conn
                    .query_row(
                        r#"SELECT 1 FROM iam_temporary_credentials
                           WHERE access_key_id = ?1 AND session_token_hash = ?2 AND expiration > ?3"#,
                        params![access_key_id, token_hash, now],
                        |_| Ok(true),
                    )
                    .optional()?;
                Ok(result.unwrap_or(false))
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }
}

/// Rate limiting configuration.
#[derive(Clone, Copy)]
pub struct RateLimitConfig {
    /// Maximum attempts before lockout.
    pub max_attempts: u32,
    /// Time window for counting attempts (seconds).
    pub window_secs: u64,
    /// Lockout duration after max attempts (seconds).
    pub lockout_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            window_secs: 60,
            lockout_secs: 15 * 60,
        }
    }
}

/// Rate limiting result.
#[derive(Debug)]
pub struct RateLimitStatus {
    /// Whether the key is currently locked out.
    pub is_locked: bool,
    /// Number of attempts in current window.
    pub attempts: u32,
    /// Seconds remaining in lockout (if locked).
    pub lockout_remaining_secs: Option<u64>,
}

impl IamStore {
    /// Record a failed login attempt for persistent rate limiting.
    ///
    /// The key should be an IP address or "ip:<addr>" / "user:<username>" format.
    pub async fn record_login_failure(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> Result<RateLimitStatus> {
        let key = key.to_string();
        let now = Utc::now();
        let window_start = now - chrono::Duration::seconds(config.window_secs as i64);
        let lockout_until = now + chrono::Duration::seconds(config.lockout_secs as i64);
        let max_attempts = config.max_attempts;
        let lockout_secs = config.lockout_secs;

        self.db
            .call(move |conn| {
                // Check existing record
                let existing: Option<(i32, String, Option<String>)> = conn
                    .query_row(
                        "SELECT attempts, first_attempt_at, locked_until FROM rate_limits WHERE key = ?1",
                        params![&key],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;

                match existing {
                    Some((attempts, first_attempt_str, locked_until_str)) => {
                        // Check if currently locked
                        if let Some(ref locked_str) = locked_until_str
                            && let Ok(locked_dt) = DateTime::parse_from_rfc3339(locked_str)
                        {
                            let locked_dt = locked_dt.with_timezone(&Utc);
                            if locked_dt > now {
                                let remaining = (locked_dt - now).num_seconds().max(0) as u64;
                                return Ok(RateLimitStatus {
                                    is_locked: true,
                                    attempts: attempts as u32,
                                    lockout_remaining_secs: Some(remaining),
                                });
                            }
                        }

                        // Check if window expired
                        let first_attempt = DateTime::parse_from_rfc3339(&first_attempt_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or(now);

                        if first_attempt < window_start {
                            // Window expired, reset
                            conn.execute(
                                "UPDATE rate_limits SET attempts = 1, first_attempt_at = ?1, locked_until = NULL WHERE key = ?2",
                                params![now.to_rfc3339(), &key],
                            )?;
                            Ok(RateLimitStatus {
                                is_locked: false,
                                attempts: 1,
                                lockout_remaining_secs: None,
                            })
                        } else {
                            // Increment attempts
                            let new_attempts = attempts + 1;
                            if new_attempts >= max_attempts as i32 {
                                // Lock out
                                conn.execute(
                                    "UPDATE rate_limits SET attempts = ?1, locked_until = ?2 WHERE key = ?3",
                                    params![new_attempts, lockout_until.to_rfc3339(), &key],
                                )?;
                                Ok(RateLimitStatus {
                                    is_locked: true,
                                    attempts: new_attempts as u32,
                                    lockout_remaining_secs: Some(lockout_secs),
                                })
                            } else {
                                conn.execute(
                                    "UPDATE rate_limits SET attempts = ?1 WHERE key = ?2",
                                    params![new_attempts, &key],
                                )?;
                                Ok(RateLimitStatus {
                                    is_locked: false,
                                    attempts: new_attempts as u32,
                                    lockout_remaining_secs: None,
                                })
                            }
                        }
                    }
                    None => {
                        // First attempt
                        conn.execute(
                            "INSERT INTO rate_limits (key, attempts, first_attempt_at) VALUES (?1, 1, ?2)",
                            params![&key, now.to_rfc3339()],
                        )?;
                        Ok(RateLimitStatus {
                            is_locked: false,
                            attempts: 1,
                            lockout_remaining_secs: None,
                        })
                    }
                }
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    /// Clear login attempts for a key (call on successful login).
    pub async fn clear_login_attempts(&self, key: &str) -> Result<()> {
        let key = key.to_string();
        self.db
            .call(move |conn| {
                conn.execute("DELETE FROM rate_limits WHERE key = ?1", params![&key])?;
                Ok(())
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    /// Check if a key is currently rate limited.
    pub async fn is_rate_limited(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> Result<RateLimitStatus> {
        let key = key.to_string();
        let now = Utc::now();
        let window_start = now - chrono::Duration::seconds(config.window_secs as i64);
        let max_attempts = config.max_attempts;

        self.db
            .call(move |conn| {
                let existing: Option<(i32, String, Option<String>)> = conn
                    .query_row(
                        "SELECT attempts, first_attempt_at, locked_until FROM rate_limits WHERE key = ?1",
                        params![&key],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;

                match existing {
                    Some((attempts, first_attempt_str, locked_until_str)) => {
                        // Check if currently locked
                        if let Some(ref locked_str) = locked_until_str
                            && let Ok(locked_dt) = DateTime::parse_from_rfc3339(locked_str)
                        {
                            let locked_dt = locked_dt.with_timezone(&Utc);
                            if locked_dt > now {
                                let remaining = (locked_dt - now).num_seconds().max(0) as u64;
                                return Ok(RateLimitStatus {
                                    is_locked: true,
                                    attempts: attempts as u32,
                                    lockout_remaining_secs: Some(remaining),
                                });
                            }
                        }

                        // Check if within window
                        let first_attempt = DateTime::parse_from_rfc3339(&first_attempt_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or(now);

                        if first_attempt < window_start {
                            // Window expired
                            Ok(RateLimitStatus {
                                is_locked: false,
                                attempts: 0,
                                lockout_remaining_secs: None,
                            })
                        } else {
                            Ok(RateLimitStatus {
                                is_locked: attempts >= max_attempts as i32,
                                attempts: attempts as u32,
                                lockout_remaining_secs: None,
                            })
                        }
                    }
                    None => Ok(RateLimitStatus {
                        is_locked: false,
                        attempts: 0,
                        lockout_remaining_secs: None,
                    }),
                }
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    /// Cleanup expired rate limit entries.
    pub async fn cleanup_rate_limits(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        self.db
            .call(move |conn| {
                // Delete entries where lockout has expired and no recent attempts
                let deleted = conn.execute(
                    "DELETE FROM rate_limits WHERE locked_until IS NOT NULL AND locked_until < ?1",
                    params![&now],
                )?;
                Ok(deleted as u64)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    // === OIDC Provider Operations ===

    /// Count configured OIDC providers (used to decide env-seeding on boot).
    pub async fn count_oidc_providers(&self) -> Result<i64> {
        self.db
            .call(|conn| {
                let count: i64 =
                    conn.query_row("SELECT COUNT(*) FROM oidc_providers", [], |row| row.get(0))?;
                Ok(count)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    /// List all configured OIDC providers (secrets decrypted).
    pub async fn list_oidc_providers(&self) -> Result<Vec<OidcConfig>> {
        let rows = self
            .db
            .call(|conn| {
                let mut stmt = conn.prepare(OIDC_SELECT_COLUMNS)?;
                let rows = stmt
                    .query_map([], oidc_row_to_raw)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))?;

        rows.into_iter().map(|r| self.raw_to_oidc_config(r)).collect()
    }

    /// Get a single OIDC provider by ID (secret decrypted), if present.
    pub async fn get_oidc_provider(&self, id: &str) -> Result<Option<OidcConfig>> {
        let id = id.to_string();
        let raw = self
            .db
            .call(move |conn| {
                let row = conn
                    .query_row(
                        &format!("{OIDC_SELECT_COLUMNS} WHERE id = ?1"),
                        params![id],
                        oidc_row_to_raw,
                    )
                    .optional()?;
                Ok(row)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))?;

        match raw {
            Some(r) => Ok(Some(self.raw_to_oidc_config(r)?)),
            None => Ok(None),
        }
    }

    /// Create a new OIDC provider (client secret encrypted at rest).
    pub async fn create_oidc_provider(&self, config: &OidcConfig) -> Result<()> {
        let raw = self.oidc_config_to_raw(config)?;
        let id_for_err = config.id.clone();
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO oidc_providers (
                        id, name, enabled, issuer_url, client_id, client_secret_enc,
                        redirect_uri, scopes, username_claim, groups_claim,
                        auto_create_users, default_policy, group_policy_mappings,
                        created_at, updated_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                        datetime('now'), datetime('now')
                    )",
                    params![
                        raw.id,
                        raw.name,
                        raw.enabled as i32,
                        raw.issuer_url,
                        raw.client_id,
                        raw.client_secret_enc,
                        raw.redirect_uri,
                        raw.scopes,
                        raw.username_claim,
                        raw.groups_claim,
                        raw.auto_create_users as i32,
                        raw.default_policy,
                        raw.group_policy_mappings,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| {
                if e.to_string().contains("UNIQUE constraint") {
                    IamError::Oidc(format!("provider '{id_for_err}' already exists"))
                } else {
                    IamError::Database(e.to_string())
                }
            })
    }

    /// Update an existing OIDC provider. The client secret is only rewritten
    /// when `config.client_secret` is non-empty (write-only field semantics).
    pub async fn update_oidc_provider(&self, config: &OidcConfig) -> Result<()> {
        let raw = self.oidc_config_to_raw(config)?;
        let rewrite_secret = !config.client_secret.is_empty();
        let id_for_err = config.id.clone();
        let rows = self
            .db
            .call(move |conn| {
                let rows = if rewrite_secret {
                    conn.execute(
                        "UPDATE oidc_providers SET
                            name = ?2, enabled = ?3, issuer_url = ?4, client_id = ?5,
                            client_secret_enc = ?6, redirect_uri = ?7, scopes = ?8,
                            username_claim = ?9, groups_claim = ?10, auto_create_users = ?11,
                            default_policy = ?12, group_policy_mappings = ?13,
                            updated_at = datetime('now')
                        WHERE id = ?1",
                        params![
                            raw.id,
                            raw.name,
                            raw.enabled as i32,
                            raw.issuer_url,
                            raw.client_id,
                            raw.client_secret_enc,
                            raw.redirect_uri,
                            raw.scopes,
                            raw.username_claim,
                            raw.groups_claim,
                            raw.auto_create_users as i32,
                            raw.default_policy,
                            raw.group_policy_mappings,
                        ],
                    )?
                } else {
                    conn.execute(
                        "UPDATE oidc_providers SET
                            name = ?2, enabled = ?3, issuer_url = ?4, client_id = ?5,
                            redirect_uri = ?6, scopes = ?7, username_claim = ?8,
                            groups_claim = ?9, auto_create_users = ?10, default_policy = ?11,
                            group_policy_mappings = ?12, updated_at = datetime('now')
                        WHERE id = ?1",
                        params![
                            raw.id,
                            raw.name,
                            raw.enabled as i32,
                            raw.issuer_url,
                            raw.client_id,
                            raw.redirect_uri,
                            raw.scopes,
                            raw.username_claim,
                            raw.groups_claim,
                            raw.auto_create_users as i32,
                            raw.default_policy,
                            raw.group_policy_mappings,
                        ],
                    )?
                };
                Ok(rows)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))?;

        if rows == 0 {
            return Err(IamError::Oidc(format!("provider '{id_for_err}' not found")));
        }
        Ok(())
    }

    /// Delete an OIDC provider by ID.
    pub async fn delete_oidc_provider(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        let id_for_err = id.clone();
        let rows = self
            .db
            .call(move |conn| {
                let rows =
                    conn.execute("DELETE FROM oidc_providers WHERE id = ?1", params![id])?;
                Ok(rows)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))?;

        if rows == 0 {
            return Err(IamError::Oidc(format!("provider '{id_for_err}' not found")));
        }
        Ok(())
    }

    /// Get the SMTP relay configuration. Returns `None` if never configured.
    ///
    /// The returned `password` field is always empty; the plaintext secret is
    /// never surfaced (write-only semantics).
    pub async fn get_smtp_config(&self) -> Result<Option<SmtpConfig>> {
        let raw = self
            .db
            .call(|conn| {
                let row = conn
                    .query_row(SMTP_SELECT_COLUMNS, [], smtp_row_to_raw)
                    .optional()?;
                Ok(row)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))?;

        Ok(raw.map(|r| r.into_config_without_secret()))
    }

    /// Get the SMTP relay configuration with the password decrypted.
    ///
    /// Intended for internal use (building the mail transport), never for API
    /// responses.
    pub async fn get_smtp_config_with_secret(&self) -> Result<Option<SmtpConfig>> {
        let raw = self
            .db
            .call(|conn| {
                let row = conn
                    .query_row(SMTP_SELECT_COLUMNS, [], smtp_row_to_raw)
                    .optional()?;
                Ok(row)
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))?;

        match raw {
            Some(r) => {
                let password = if r.password_enc.is_empty() {
                    String::new()
                } else {
                    self.decrypt_secret(&r.password_enc)?
                };
                let mut config = r.into_config_without_secret();
                config.password = password;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    /// Insert or replace the SMTP relay configuration.
    ///
    /// The password is only rewritten when `config.password` is non-empty
    /// (write-only field semantics); otherwise the previously stored encrypted
    /// password is preserved.
    pub async fn set_smtp_config(&self, config: &SmtpConfig) -> Result<()> {
        let password_enc = if config.password.is_empty() {
            None
        } else {
            Some(self.encrypt_secret(&config.password)?)
        };
        let recipients = serde_json::to_string(&config.alert_recipients)
            .map_err(|e| IamError::Smtp(format!("failed to serialize recipients: {e}")))?;
        let raw = SmtpConfigRaw {
            enabled: config.enabled,
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
            password_enc: password_enc.clone().unwrap_or_default(),
            from_address: config.from_address.clone(),
            from_name: config.from_name.clone(),
            use_starttls: config.use_starttls,
            alert_on_delivery_failure: config.alert_on_delivery_failure,
            send_usage_reports: config.send_usage_reports,
            usage_report_schedule: config.usage_report_schedule.as_str().to_string(),
            alert_on_audit_events: config.alert_on_audit_events,
            alert_recipients: recipients,
        };

        self.db
            .call(move |conn| {
                // Determine the password to store: keep existing when not rewriting.
                let stored_password: String = match password_enc {
                    Some(enc) => enc,
                    None => conn
                        .query_row(
                            "SELECT password_enc FROM smtp_config WHERE id = 1",
                            [],
                            |row| row.get(0),
                        )
                        .optional()?
                        .unwrap_or_default(),
                };

                conn.execute(
                    "INSERT INTO smtp_config (
                        id, enabled, host, port, username, password_enc, from_address,
                        from_name, use_starttls, alert_on_delivery_failure,
                        send_usage_reports, usage_report_schedule, alert_on_audit_events,
                        alert_recipients, updated_at
                    ) VALUES (
                        1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                        datetime('now')
                    )
                    ON CONFLICT(id) DO UPDATE SET
                        enabled = ?1, host = ?2, port = ?3, username = ?4,
                        password_enc = ?5, from_address = ?6, from_name = ?7,
                        use_starttls = ?8, alert_on_delivery_failure = ?9,
                        send_usage_reports = ?10, usage_report_schedule = ?11,
                        alert_on_audit_events = ?12, alert_recipients = ?13,
                        updated_at = datetime('now')",
                    params![
                        raw.enabled as i32,
                        raw.host,
                        raw.port,
                        raw.username,
                        stored_password,
                        raw.from_address,
                        raw.from_name,
                        raw.use_starttls as i32,
                        raw.alert_on_delivery_failure as i32,
                        raw.send_usage_reports as i32,
                        raw.usage_report_schedule,
                        raw.alert_on_audit_events as i32,
                        raw.alert_recipients,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| IamError::Database(e.to_string()))
    }

    /// Convert a decrypted `OidcConfig` into a storable row (secret encrypted).
    fn oidc_config_to_raw(&self, config: &OidcConfig) -> Result<OidcProviderRaw> {
        let client_secret_enc = if config.client_secret.is_empty() {
            String::new()
        } else {
            self.encrypt_secret(&config.client_secret)?
        };
        let group_policy_mappings = serde_json::to_string(&config.group_policy_mappings)
            .map_err(|e| IamError::Oidc(format!("failed to serialize group mappings: {e}")))?;
        Ok(OidcProviderRaw {
            id: config.id.clone(),
            name: config.name.clone(),
            enabled: config.enabled,
            issuer_url: config.issuer_url.clone(),
            client_id: config.client_id.clone(),
            client_secret_enc,
            redirect_uri: config.redirect_uri.clone(),
            scopes: config.scopes.join(" "),
            username_claim: config.username_claim.clone(),
            groups_claim: config.groups_claim.clone(),
            auto_create_users: config.auto_create_users,
            default_policy: config.default_policy.clone(),
            group_policy_mappings,
        })
    }

    /// Convert a stored row into an `OidcConfig`, decrypting the client secret.
    fn raw_to_oidc_config(&self, raw: OidcProviderRaw) -> Result<OidcConfig> {
        let client_secret = if raw.client_secret_enc.is_empty() {
            String::new()
        } else {
            self.decrypt_secret(&raw.client_secret_enc)?
        };
        let scopes = raw
            .scopes
            .split_whitespace()
            .map(String::from)
            .collect::<Vec<_>>();
        let group_policy_mappings =
            serde_json::from_str(&raw.group_policy_mappings).unwrap_or_default();
        Ok(OidcConfig {
            id: raw.id,
            name: raw.name,
            enabled: raw.enabled,
            issuer_url: raw.issuer_url,
            client_id: raw.client_id,
            client_secret,
            redirect_uri: raw.redirect_uri,
            scopes,
            username_claim: raw.username_claim,
            groups_claim: raw.groups_claim,
            auto_create_users: raw.auto_create_users,
            default_policy: raw.default_policy,
            group_policy_mappings,
        })
    }
}

/// Column list shared by OIDC provider SELECT queries.
const OIDC_SELECT_COLUMNS: &str = "SELECT id, name, enabled, issuer_url, client_id, \
    client_secret_enc, redirect_uri, scopes, username_claim, groups_claim, \
    auto_create_users, default_policy, group_policy_mappings FROM oidc_providers";

/// Raw OIDC provider row (client secret still encrypted).
struct OidcProviderRaw {
    id: String,
    name: String,
    enabled: bool,
    issuer_url: String,
    client_id: String,
    client_secret_enc: String,
    redirect_uri: String,
    scopes: String,
    username_claim: String,
    groups_claim: Option<String>,
    auto_create_users: bool,
    default_policy: Option<String>,
    group_policy_mappings: String,
}

/// Column list shared by SMTP config SELECT queries.
const SMTP_SELECT_COLUMNS: &str = "SELECT enabled, host, port, username, password_enc, \
    from_address, from_name, use_starttls, alert_on_delivery_failure, send_usage_reports, \
    usage_report_schedule, alert_on_audit_events, alert_recipients FROM smtp_config WHERE id = 1";

/// Raw SMTP config row (password still encrypted).
struct SmtpConfigRaw {
    enabled: bool,
    host: String,
    port: u16,
    username: String,
    password_enc: String,
    from_address: String,
    from_name: Option<String>,
    use_starttls: bool,
    alert_on_delivery_failure: bool,
    send_usage_reports: bool,
    usage_report_schedule: String,
    alert_on_audit_events: bool,
    alert_recipients: String,
}

impl SmtpConfigRaw {
    /// Build an `SmtpConfig` with an empty password (write-only semantics).
    fn into_config_without_secret(self) -> SmtpConfig {
        let alert_recipients = serde_json::from_str(&self.alert_recipients).unwrap_or_default();
        let usage_report_schedule = self
            .usage_report_schedule
            .parse::<UsageReportSchedule>()
            .unwrap_or_default();
        SmtpConfig {
            enabled: self.enabled,
            host: self.host,
            port: self.port,
            username: self.username,
            password: String::new(),
            from_address: self.from_address,
            from_name: self.from_name,
            use_starttls: self.use_starttls,
            alert_on_delivery_failure: self.alert_on_delivery_failure,
            send_usage_reports: self.send_usage_reports,
            usage_report_schedule,
            alert_on_audit_events: self.alert_on_audit_events,
            alert_recipients,
        }
    }
}

/// Map a `smtp_config` row (in `SMTP_SELECT_COLUMNS` order) to a raw struct.
fn smtp_row_to_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<SmtpConfigRaw> {
    Ok(SmtpConfigRaw {
        enabled: row.get::<_, i32>(0)? != 0,
        host: row.get(1)?,
        port: row.get::<_, i64>(2)? as u16,
        username: row.get(3)?,
        password_enc: row.get(4)?,
        from_address: row.get(5)?,
        from_name: row.get(6)?,
        use_starttls: row.get::<_, i32>(7)? != 0,
        alert_on_delivery_failure: row.get::<_, i32>(8)? != 0,
        send_usage_reports: row.get::<_, i32>(9)? != 0,
        usage_report_schedule: row.get(10)?,
        alert_on_audit_events: row.get::<_, i32>(11)? != 0,
        alert_recipients: row.get(12)?,
    })
}

/// Map a `oidc_providers` row (in `OIDC_SELECT_COLUMNS` order) to a raw struct.
fn oidc_row_to_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<OidcProviderRaw> {
    Ok(OidcProviderRaw {
        id: row.get(0)?,
        name: row.get(1)?,
        enabled: row.get::<_, i32>(2)? != 0,
        issuer_url: row.get(3)?,
        client_id: row.get(4)?,
        client_secret_enc: row.get(5)?,
        redirect_uri: row.get(6)?,
        scopes: row.get(7)?,
        username_claim: row.get(8)?,
        groups_claim: row.get(9)?,
        auto_create_users: row.get::<_, i32>(10)? != 0,
        default_policy: row.get(11)?,
        group_policy_mappings: row.get(12)?,
    })
}

#[cfg(test)]
mod oidc_provider_tests {
    use super::*;

    async fn test_store() -> IamStore {
        let db = AsyncConnection::open_in_memory()
            .await
            .expect("open in-memory db");
        IamStore::new(db, "root".to_string(), "root-secret".to_string())
            .await
            .expect("create store")
    }

    fn sample_provider(id: &str) -> OidcConfig {
        OidcConfig {
            id: id.to_string(),
            name: "Test IdP".to_string(),
            enabled: true,
            issuer_url: "https://idp.example.com".to_string(),
            client_id: "client-abc".to_string(),
            client_secret: "super-secret-value".to_string(),
            redirect_uri: "https://console.example.com/api/v1/auth/callback".to_string(),
            scopes: vec!["openid".to_string(), "email".to_string()],
            username_claim: "email".to_string(),
            groups_claim: None,
            auto_create_users: true,
            default_policy: None,
            group_policy_mappings: std::collections::HashMap::new(),
        }
    }

    #[tokio::test]
    async fn create_get_roundtrip_decrypts_secret() {
        let store = test_store().await;
        store
            .create_oidc_provider(&sample_provider("idp1"))
            .await
            .expect("create provider");

        let got = store
            .get_oidc_provider("idp1")
            .await
            .expect("get provider")
            .expect("provider present");
        assert_eq!(got.client_secret, "super-secret-value");
        assert_eq!(got.client_id, "client-abc");
        assert_eq!(got.scopes, vec!["openid".to_string(), "email".to_string()]);
        assert_eq!(store.count_oidc_providers().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn secret_is_encrypted_at_rest() {
        let store = test_store().await;
        store
            .create_oidc_provider(&sample_provider("idp1"))
            .await
            .expect("create provider");

        // Read the raw stored ciphertext directly; it must not be the plaintext.
        let stored: String = store
            .db
            .call(|conn| {
                let s: String = conn.query_row(
                    "SELECT client_secret_enc FROM oidc_providers WHERE id = 'idp1'",
                    [],
                    |row| row.get(0),
                )?;
                Ok(s)
            })
            .await
            .unwrap();
        assert_ne!(stored, "super-secret-value");
        assert!(!stored.contains("super-secret-value"));
    }

    #[tokio::test]
    async fn update_preserves_secret_when_empty() {
        let store = test_store().await;
        store
            .create_oidc_provider(&sample_provider("idp1"))
            .await
            .expect("create provider");

        // Update with empty client_secret => existing secret preserved.
        let mut updated = sample_provider("idp1");
        updated.name = "Renamed".to_string();
        updated.client_secret = String::new();
        store
            .update_oidc_provider(&updated)
            .await
            .expect("update provider");

        let got = store.get_oidc_provider("idp1").await.unwrap().unwrap();
        assert_eq!(got.name, "Renamed");
        assert_eq!(got.client_secret, "super-secret-value");
    }

    #[tokio::test]
    async fn update_rewrites_secret_when_present() {
        let store = test_store().await;
        store
            .create_oidc_provider(&sample_provider("idp1"))
            .await
            .expect("create provider");

        let mut updated = sample_provider("idp1");
        updated.client_secret = "new-secret".to_string();
        store
            .update_oidc_provider(&updated)
            .await
            .expect("update provider");

        let got = store.get_oidc_provider("idp1").await.unwrap().unwrap();
        assert_eq!(got.client_secret, "new-secret");
    }

    #[tokio::test]
    async fn delete_removes_provider() {
        let store = test_store().await;
        store
            .create_oidc_provider(&sample_provider("idp1"))
            .await
            .expect("create provider");
        store
            .delete_oidc_provider("idp1")
            .await
            .expect("delete provider");
        assert!(store.get_oidc_provider("idp1").await.unwrap().is_none());
        assert!(store.delete_oidc_provider("idp1").await.is_err());
    }

    #[tokio::test]
    async fn duplicate_create_fails() {
        let store = test_store().await;
        store
            .create_oidc_provider(&sample_provider("idp1"))
            .await
            .expect("create provider");
        assert!(store.create_oidc_provider(&sample_provider("idp1")).await.is_err());
    }
}

#[cfg(test)]
mod smtp_config_tests {
    use super::*;

    async fn test_store() -> IamStore {
        let db = AsyncConnection::open_in_memory()
            .await
            .expect("open in-memory db");
        IamStore::new(db, "root".to_string(), "root-secret".to_string())
            .await
            .expect("create store")
    }

    fn sample_config() -> SmtpConfig {
        SmtpConfig {
            enabled: true,
            host: "mail.smtp2go.com".to_string(),
            port: 587,
            username: "relay-user".to_string(),
            password: "relay-password".to_string(),
            from_address: "alerts@example.com".to_string(),
            from_name: Some("Strix Alerts".to_string()),
            use_starttls: true,
            alert_on_delivery_failure: true,
            send_usage_reports: true,
            usage_report_schedule: UsageReportSchedule::Daily,
            alert_on_audit_events: true,
            alert_recipients: vec!["ops@example.com".to_string()],
        }
    }

    #[tokio::test]
    async fn get_is_none_before_set() {
        let store = test_store().await;
        assert!(store.get_smtp_config().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn set_get_roundtrip_hides_password() {
        let store = test_store().await;
        store.set_smtp_config(&sample_config()).await.expect("set config");

        let got = store
            .get_smtp_config()
            .await
            .expect("get config")
            .expect("config present");
        assert!(got.enabled);
        assert_eq!(got.host, "mail.smtp2go.com");
        assert_eq!(got.port, 587);
        assert_eq!(got.username, "relay-user");
        assert_eq!(got.from_address, "alerts@example.com");
        assert_eq!(got.from_name.as_deref(), Some("Strix Alerts"));
        assert!(got.alert_on_delivery_failure);
        assert!(got.send_usage_reports);
        assert_eq!(got.usage_report_schedule, UsageReportSchedule::Daily);
        assert!(got.alert_on_audit_events);
        assert_eq!(got.alert_recipients, vec!["ops@example.com".to_string()]);
        // Read API never returns the password.
        assert!(got.password.is_empty());
    }

    #[tokio::test]
    async fn with_secret_decrypts_password() {
        let store = test_store().await;
        store.set_smtp_config(&sample_config()).await.expect("set config");

        let got = store
            .get_smtp_config_with_secret()
            .await
            .expect("get config")
            .expect("config present");
        assert_eq!(got.password, "relay-password");
    }

    #[tokio::test]
    async fn password_is_encrypted_at_rest() {
        let store = test_store().await;
        store.set_smtp_config(&sample_config()).await.expect("set config");

        let stored: String = store
            .db
            .call(|conn| {
                let s: String = conn.query_row(
                    "SELECT password_enc FROM smtp_config WHERE id = 1",
                    [],
                    |row| row.get(0),
                )?;
                Ok(s)
            })
            .await
            .unwrap();
        assert_ne!(stored, "relay-password");
        assert!(!stored.contains("relay-password"));
    }

    #[tokio::test]
    async fn update_preserves_password_when_empty() {
        let store = test_store().await;
        store.set_smtp_config(&sample_config()).await.expect("set config");

        // Re-save with an empty password => existing secret preserved.
        let mut updated = sample_config();
        updated.host = "mail.example.net".to_string();
        updated.password = String::new();
        store.set_smtp_config(&updated).await.expect("update config");

        let got = store
            .get_smtp_config_with_secret()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.host, "mail.example.net");
        assert_eq!(got.password, "relay-password");
    }

    #[tokio::test]
    async fn update_rewrites_password_when_present() {
        let store = test_store().await;
        store.set_smtp_config(&sample_config()).await.expect("set config");

        let mut updated = sample_config();
        updated.password = "new-password".to_string();
        store.set_smtp_config(&updated).await.expect("update config");

        let got = store
            .get_smtp_config_with_secret()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.password, "new-password");
    }
}
