use anyhow::Result;
use rand::{distributions::Alphanumeric, Rng};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    FromRow, Row, SqlitePool,
};
use std::{collections::HashMap, str::FromStr};
use uuid::Uuid;

use crate::models::{SystemSettings, User};

pub struct Database {
    pub pool: SqlitePool,
}

#[derive(Debug, Clone, FromRow)]
struct LegacyUser {
    id: String,
    username: String,
    password_hash: String,
    avatar: Option<String>,
    totp_secret: Option<String>,
    totp_enabled: i64,
    created_at: String,
}

#[derive(Debug, Clone, FromRow)]
struct LegacyProject {
    id: String,
    user_id: String,
    name: String,
    description: String,
    background_image: Option<String>,
    sort_order: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
struct LegacyDocNode {
    id: String,
    user_id: String,
    project_id: Option<String>,
    parent_id: Option<String>,
    name: String,
    node_type: String,
    content: Option<String>,
    sort_order: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
struct LegacyShare {
    user_id: String,
    doc_id: String,
    token: String,
    password_hash: Option<String>,
    password_ciphertext: Option<String>,
    expires_at: Option<String>,
    created_at: String,
}

impl Database {
    pub async fn new(url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(url)?.create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        self.create_tables().await?;
        self.ensure_share_columns().await?;

        if self.uses_legacy_text_ids().await? {
            self.ensure_legacy_doc_nodes_project_column().await?;
            self.backfill_existing_doc_project_ids_legacy().await?;
            self.migrate_text_primary_keys_to_integer().await?;
        }

        self.ensure_user_columns().await?;
        self.ensure_agent_provider_columns().await?;
        self.ensure_agent_mcp_schema().await?;
        self.ensure_doc_nodes_project_column().await?;
        self.backfill_existing_doc_project_ids().await?;
        self.create_indexes().await?;

        Ok(())
    }

    async fn ensure_agent_mcp_schema(&self) -> Result<()> {
        self.ensure_agent_mcp_settings_schema().await?;
        self.ensure_agent_mcp_servers_schema().await?;
        self.repair_agent_mcp_settings_rows().await?;
        self.repair_agent_mcp_servers_rows().await?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_mcp_settings_user_id ON agent_mcp_settings(user_id)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_agent_mcp_servers_user_id ON agent_mcp_servers(user_id)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_agent_mcp_servers_enabled ON agent_mcp_servers(enabled)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_agent_mcp_servers_updated_at ON agent_mcp_servers(updated_at)",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn ensure_agent_mcp_settings_schema(&self) -> Result<()> {
        const CREATE_SQL: &str = r#"
            CREATE TABLE IF NOT EXISTS agent_mcp_settings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )
        "#;

        if !self.table_exists("agent_mcp_settings").await? {
            sqlx::query(CREATE_SQL).execute(&self.pool).await?;
            return Ok(());
        }

        if self
            .agent_mcp_settings_needs_rebuild()
            .await?
        {
            self.rebuild_agent_mcp_table(
                "agent_mcp_settings",
                CREATE_SQL,
                &["id", "user_id", "enabled", "created_at", "updated_at"],
            )
            .await?;
        }

        Ok(())
    }

    async fn ensure_agent_mcp_servers_schema(&self) -> Result<()> {
        const CREATE_SQL: &str = r#"
            CREATE TABLE IF NOT EXISTS agent_mcp_servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                transport TEXT NOT NULL CHECK(transport IN ('sse', 'streamable-http', 'stdio')),
                url TEXT,
                command TEXT,
                args_json TEXT NOT NULL DEFAULT '[]',
                env_ciphertext TEXT,
                auth_type TEXT NOT NULL DEFAULT 'none',
                auth_config_ciphertext TEXT,
                custom_headers_ciphertext TEXT,
                last_status TEXT,
                last_error TEXT,
                tools_snapshot TEXT NOT NULL DEFAULT '[]',
                resources_snapshot TEXT NOT NULL DEFAULT '[]',
                prompts_snapshot TEXT NOT NULL DEFAULT '[]',
                last_sync_at TEXT,
                config_version INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )
        "#;

        if !self.table_exists("agent_mcp_servers").await? {
            sqlx::query(CREATE_SQL).execute(&self.pool).await?;
            return Ok(());
        }

        if self.agent_mcp_servers_needs_rebuild().await? {
            self.rebuild_agent_mcp_table(
                "agent_mcp_servers",
                CREATE_SQL,
                &[
                    "id",
                    "user_id",
                    "name",
                    "enabled",
                    "transport",
                    "url",
                    "command",
                    "args_json",
                    "env_ciphertext",
                    "auth_type",
                    "auth_config_ciphertext",
                    "custom_headers_ciphertext",
                    "last_status",
                    "last_error",
                    "tools_snapshot",
                    "resources_snapshot",
                    "prompts_snapshot",
                    "last_sync_at",
                    "config_version",
                    "created_at",
                    "updated_at",
                ],
            )
            .await?;
        }

        Ok(())
    }

    async fn agent_mcp_settings_needs_rebuild(&self) -> Result<bool> {
        let required_columns = ["id", "user_id", "enabled", "created_at", "updated_at"];
        let columns = self.table_columns("agent_mcp_settings").await?;
        if required_columns
            .iter()
            .any(|column| !columns.iter().any(|existing| existing == column))
        {
            return Ok(true);
        }

        let sql = self.table_sql("agent_mcp_settings").await?.unwrap_or_default();
        Ok(!sql.contains("enabled INTEGER NOT NULL DEFAULT 0")
            || !sql.contains("created_at TEXT NOT NULL DEFAULT (datetime('now'))")
            || !sql.contains("updated_at TEXT NOT NULL DEFAULT (datetime('now'))")
            || !sql.contains("FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE"))
    }

    async fn agent_mcp_servers_needs_rebuild(&self) -> Result<bool> {
        let required_columns = [
            "id",
            "user_id",
            "name",
            "enabled",
            "transport",
            "url",
            "command",
            "args_json",
            "env_ciphertext",
            "auth_type",
            "auth_config_ciphertext",
            "custom_headers_ciphertext",
            "last_status",
            "last_error",
            "tools_snapshot",
            "resources_snapshot",
            "prompts_snapshot",
            "last_sync_at",
            "config_version",
            "created_at",
            "updated_at",
        ];
        let columns = self.table_columns("agent_mcp_servers").await?;
        if required_columns
            .iter()
            .any(|column| !columns.iter().any(|existing| existing == column))
        {
            return Ok(true);
        }

        let sql = self.table_sql("agent_mcp_servers").await?.unwrap_or_default();
        Ok(!sql.contains("enabled INTEGER NOT NULL DEFAULT 1")
            || !sql.contains("transport TEXT NOT NULL CHECK(transport IN ('sse', 'streamable-http', 'stdio'))")
            || !sql.contains("args_json TEXT NOT NULL DEFAULT '[]'")
            || !sql.contains("auth_type TEXT NOT NULL DEFAULT 'none'")
            || !sql.contains("tools_snapshot TEXT NOT NULL DEFAULT '[]'")
            || !sql.contains("resources_snapshot TEXT NOT NULL DEFAULT '[]'")
            || !sql.contains("prompts_snapshot TEXT NOT NULL DEFAULT '[]'")
            || !sql.contains("config_version INTEGER NOT NULL DEFAULT 1")
            || !sql.contains("updated_at TEXT NOT NULL DEFAULT (datetime('now'))")
            || !sql.contains("FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE"))
    }

    async fn rebuild_agent_mcp_table(
        &self,
        table: &str,
        create_sql: &str,
        copy_columns: &[&str],
    ) -> Result<()> {
        let existing_columns = self.table_columns(table).await?;
        let copy_columns = copy_columns
            .iter()
            .copied()
            .filter(|column| existing_columns.iter().any(|existing| existing == column))
            .collect::<Vec<_>>();
        let temp_table = format!("{table}_legacy_repair");

        let mut tx = self.pool.begin().await?;
        sqlx::query(&format!("DROP TABLE IF EXISTS {temp_table}"))
            .execute(&mut *tx)
            .await?;
        sqlx::query(&format!("ALTER TABLE {table} RENAME TO {temp_table}"))
            .execute(&mut *tx)
            .await?;
        sqlx::query(create_sql).execute(&mut *tx).await?;

        if !copy_columns.is_empty() && copy_columns.iter().any(|column| *column == "user_id") {
            let columns = copy_columns.join(", ");
            sqlx::query(&format!(
                "INSERT INTO {table} ({columns}) SELECT {columns} FROM {temp_table} WHERE EXISTS (SELECT 1 FROM users WHERE users.id = {temp_table}.user_id)"
            ))
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(&format!("DROP TABLE {temp_table}"))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn repair_agent_mcp_settings_rows(&self) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM agent_mcp_settings
             WHERE NOT EXISTS (
                 SELECT 1
                   FROM users
                  WHERE users.id = agent_mcp_settings.user_id
             )
                OR rowid NOT IN (
                 SELECT MAX(rowid)
                   FROM agent_mcp_settings
                  GROUP BY user_id
             )
        "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn repair_agent_mcp_servers_rows(&self) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM agent_mcp_servers
             WHERE NOT EXISTS (
                 SELECT 1
                   FROM users
                  WHERE users.id = agent_mcp_servers.user_id
             )
        "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn table_columns(&self, table: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter()
            .map(|row| row.try_get::<String, _>("name").map_err(Into::into))
            .collect()
    }

    async fn table_sql(&self, table: &str) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    async fn create_tables(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                avatar TEXT,
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                is_super_admin INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                background_image TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS doc_nodes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                project_id INTEGER,
                parent_id INTEGER,
                name TEXT NOT NULL,
                node_type TEXT NOT NULL CHECK(node_type IN ('dir', 'doc')),
                content TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL,
                FOREIGN KEY (parent_id) REFERENCES doc_nodes(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS shares (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                doc_id INTEGER NOT NULL,
                token TEXT NOT NULL UNIQUE,
                password_hash TEXT,
                password_ciphertext TEXT,
                expires_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (doc_id) REFERENCES doc_nodes(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS uploads (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                original_name TEXT NOT NULL,
                stored_path TEXT NOT NULL,
                content_type TEXT,
                size INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS system_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                registration_enabled INTEGER NOT NULL DEFAULT 1,
                upload_max_bytes INTEGER NOT NULL DEFAULT 20971520,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agent_providers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'openai',
                base_url TEXT NOT NULL,
                api_key_ciphertext TEXT NOT NULL,
                api TEXT NOT NULL DEFAULT 'responses',
                anthropic_version TEXT,
                remote_models TEXT NOT NULL DEFAULT '[]',
                is_active INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agent_models (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider_id INTEGER NOT NULL,
                alias TEXT NOT NULL,
                model_id TEXT NOT NULL,
                display_name TEXT,
                config TEXT NOT NULL DEFAULT '{}',
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (provider_id) REFERENCES agent_providers(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn ensure_user_columns(&self) -> Result<()> {
        let columns = sqlx::query("PRAGMA table_info(users)")
            .fetch_all(&self.pool)
            .await?;

        let has_is_super_admin = columns.iter().any(|col| {
            col.try_get::<String, _>("name")
                .map(|name| name == "is_super_admin")
                .unwrap_or(false)
        });
        let has_is_active = columns.iter().any(|col| {
            col.try_get::<String, _>("name")
                .map(|name| name == "is_active")
                .unwrap_or(false)
        });
        let has_updated_at = columns.iter().any(|col| {
            col.try_get::<String, _>("name")
                .map(|name| name == "updated_at")
                .unwrap_or(false)
        });

        if !has_is_super_admin {
            sqlx::query("ALTER TABLE users ADD COLUMN is_super_admin INTEGER NOT NULL DEFAULT 0")
                .execute(&self.pool)
                .await?;
        }

        if !has_is_active {
            sqlx::query("ALTER TABLE users ADD COLUMN is_active INTEGER NOT NULL DEFAULT 1")
                .execute(&self.pool)
                .await?;
        }

        if !has_updated_at {
            sqlx::query(
                "ALTER TABLE users ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'))",
            )
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn ensure_agent_provider_columns(&self) -> Result<()> {
        // Full reconstruction: the legacy single-table layout (with model_configs /
        // enabled_models / custom_models columns) is incompatible with the new
        // provider + model two-table design. Detect the legacy shape and drop it so
        // create_tables can rebuild the current schema. Existing provider rows are
        // discarded by design (no compatibility/migration path).
        let columns = sqlx::query("PRAGMA table_info(agent_providers)")
            .fetch_all(&self.pool)
            .await?;

        let is_legacy = columns.iter().any(|col| {
            col.try_get::<String, _>("name")
                .map(|name| {
                    name == "model_configs"
                        || name == "enabled_models"
                        || name == "custom_models"
                        || name == "provider_kind"
                })
                .unwrap_or(false)
        });

        if is_legacy {
            sqlx::query("DROP TABLE IF EXISTS agent_models")
                .execute(&self.pool)
                .await?;
            sqlx::query("DROP TABLE IF EXISTS agent_providers")
                .execute(&self.pool)
                .await?;
            self.create_tables().await?;
        }

        Ok(())
    }

    async fn ensure_share_columns(&self) -> Result<()> {
        let columns = sqlx::query("PRAGMA table_info(shares)")
            .fetch_all(&self.pool)
            .await?;

        let has_password_ciphertext = columns.iter().any(|col| {
            col.try_get::<String, _>("name")
                .map(|name| name == "password_ciphertext")
                .unwrap_or(false)
        });

        if !has_password_ciphertext {
            sqlx::query("ALTER TABLE shares ADD COLUMN password_ciphertext TEXT")
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    async fn create_indexes(&self) -> Result<()> {
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_projects_user_id ON projects(user_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_doc_nodes_user_id ON doc_nodes(user_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_doc_nodes_project_id ON doc_nodes(project_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_doc_nodes_parent_id ON doc_nodes(parent_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_shares_token ON shares(token)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_shares_doc_id ON shares(doc_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_uploads_user_id ON uploads(user_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_uploads_kind ON uploads(kind)")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_agent_providers_user_id ON agent_providers(user_id)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_agent_providers_user_active ON agent_providers(user_id, is_active)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_agent_models_provider_id ON agent_models(provider_id)",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn bootstrap_system_settings(
        &self,
        registration_enabled: bool,
        upload_max_bytes: i64,
    ) -> Result<SystemSettings> {
        if let Some(settings) =
            sqlx::query_as::<_, SystemSettings>("SELECT * FROM system_settings WHERE id = 1")
                .fetch_optional(&self.pool)
                .await?
        {
            return Ok(settings);
        }

        sqlx::query(
            "INSERT INTO system_settings (id, registration_enabled, upload_max_bytes)
             VALUES (1, ?, ?)",
        )
        .bind(if registration_enabled { 1 } else { 0 })
        .bind(upload_max_bytes)
        .execute(&self.pool)
        .await?;

        self.get_system_settings().await
    }

    pub async fn get_system_settings(&self) -> Result<SystemSettings> {
        let settings =
            sqlx::query_as::<_, SystemSettings>("SELECT * FROM system_settings WHERE id = 1")
                .fetch_one(&self.pool)
                .await?;
        Ok(settings)
    }

    pub async fn update_system_settings(
        &self,
        registration_enabled: bool,
        upload_max_bytes: i64,
    ) -> Result<SystemSettings> {
        sqlx::query(
            "UPDATE system_settings
             SET registration_enabled = ?, upload_max_bytes = ?, updated_at = datetime('now')
             WHERE id = 1",
        )
        .bind(if registration_enabled { 1 } else { 0 })
        .bind(upload_max_bytes)
        .execute(&self.pool)
        .await?;

        self.get_system_settings().await
    }

    pub async fn ensure_super_admin(&self) -> Result<Option<String>> {
        let existing: Option<User> = sqlx::query_as("SELECT * FROM users WHERE username = 'admin'")
            .fetch_optional(&self.pool)
            .await?;

        if let Some(user) = existing {
            if user.is_super_admin != 1 || user.is_active != 1 {
                sqlx::query(
                    "UPDATE users
                     SET is_super_admin = 1, is_active = 1, updated_at = datetime('now')
                     WHERE id = ?",
                )
                .bind(user.id)
                .execute(&self.pool)
                .await?;
            }
            return Ok(None);
        }

        let password: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(20)
            .map(char::from)
            .collect();
        let password_hash = bcrypt::hash(&password, 10)?;

        sqlx::query(
            "INSERT INTO users
             (username, password_hash, avatar, totp_secret, totp_enabled, is_super_admin, is_active)
             VALUES ('admin', ?, NULL, NULL, 0, 1, 1)",
        )
        .bind(password_hash)
        .execute(&self.pool)
        .await?;

        Ok(Some(password))
    }

    async fn uses_legacy_text_ids(&self) -> Result<bool> {
        if !self.table_exists("users").await? {
            return Ok(false);
        }

        let columns = sqlx::query("PRAGMA table_info(users)")
            .fetch_all(&self.pool)
            .await?;

        let id_type = columns.iter().find_map(|col| {
            let name = col.try_get::<String, _>("name").ok()?;
            if name == "id" {
                col.try_get::<String, _>("type").ok()
            } else {
                None
            }
        });

        Ok(id_type
            .map(|ty| ty.eq_ignore_ascii_case("TEXT"))
            .unwrap_or(false))
    }

    async fn table_exists(&self, table: &str) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(table)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists > 0)
    }

    async fn ensure_doc_nodes_project_column(&self) -> Result<()> {
        let columns = sqlx::query("PRAGMA table_info(doc_nodes)")
            .fetch_all(&self.pool)
            .await?;

        let has_project_id = columns.iter().any(|col| {
            col.try_get::<String, _>("name")
                .map(|name| name == "project_id")
                .unwrap_or(false)
        });

        if !has_project_id {
            sqlx::query("ALTER TABLE doc_nodes ADD COLUMN project_id INTEGER")
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    async fn ensure_legacy_doc_nodes_project_column(&self) -> Result<()> {
        let columns = sqlx::query("PRAGMA table_info(doc_nodes)")
            .fetch_all(&self.pool)
            .await?;

        let has_project_id = columns.iter().any(|col| {
            col.try_get::<String, _>("name")
                .map(|name| name == "project_id")
                .unwrap_or(false)
        });

        if !has_project_id {
            sqlx::query("ALTER TABLE doc_nodes ADD COLUMN project_id TEXT")
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    async fn backfill_existing_doc_project_ids(&self) -> Result<()> {
        let users: Vec<i64> = sqlx::query_scalar("SELECT id FROM users")
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        for user_id in users {
            let docs_without_project: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM doc_nodes WHERE user_id = ? AND project_id IS NULL",
            )
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

            if docs_without_project == 0 {
                continue;
            }

            let mut default_project_id: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM projects WHERE user_id = ? ORDER BY sort_order ASC, created_at ASC LIMIT 1",
            )
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;

            if default_project_id.is_none() {
                let created_id = sqlx::query(
                    "INSERT INTO projects (user_id, name, description, background_image, sort_order)
                     VALUES (?, '默认项目', '系统迁移自动创建的项目', NULL, 0)",
                )
                .bind(user_id)
                .execute(&self.pool)
                .await?
                .last_insert_rowid();
                default_project_id = Some(created_id);
            }

            sqlx::query(
                "UPDATE doc_nodes SET project_id = ? WHERE user_id = ? AND project_id IS NULL",
            )
            .bind(default_project_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn backfill_existing_doc_project_ids_legacy(&self) -> Result<()> {
        let users: Vec<String> = sqlx::query_scalar("SELECT id FROM users")
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        for user_id in users {
            let docs_without_project: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM doc_nodes WHERE user_id = ? AND project_id IS NULL",
            )
            .bind(&user_id)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

            if docs_without_project == 0 {
                continue;
            }

            let mut default_project_id: Option<String> = sqlx::query_scalar(
                "SELECT id FROM projects WHERE user_id = ? ORDER BY sort_order ASC, created_at ASC LIMIT 1",
            )
            .bind(&user_id)
            .fetch_optional(&self.pool)
            .await?;

            if default_project_id.is_none() {
                let created_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO projects (id, user_id, name, description, background_image, sort_order)
                     VALUES (?, ?, '默认项目', '系统迁移自动创建的项目', NULL, 0)",
                )
                .bind(&created_id)
                .bind(&user_id)
                .execute(&self.pool)
                .await?;
                default_project_id = Some(created_id);
            }

            sqlx::query(
                "UPDATE doc_nodes SET project_id = ? WHERE user_id = ? AND project_id IS NULL",
            )
            .bind(default_project_id)
            .bind(&user_id)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn migrate_text_primary_keys_to_integer(&self) -> Result<()> {
        let users: Vec<LegacyUser> =
            sqlx::query_as("SELECT * FROM users ORDER BY created_at ASC, id ASC")
                .fetch_all(&self.pool)
                .await?;

        let projects: Vec<LegacyProject> = sqlx::query_as(
            "SELECT * FROM projects ORDER BY sort_order ASC, created_at ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let doc_nodes: Vec<LegacyDocNode> = sqlx::query_as(
            "SELECT * FROM doc_nodes ORDER BY sort_order ASC, created_at ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let shares: Vec<LegacyShare> = sqlx::query_as(
            "SELECT user_id, doc_id, token, password_hash, password_ciphertext, expires_at, created_at
             FROM shares
             ORDER BY created_at ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut tx = self.pool.begin().await?;

        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *tx)
            .await?;

        for table in ["shares_new", "doc_nodes_new", "projects_new", "users_new"] {
            sqlx::query(&format!("DROP TABLE IF EXISTS {table}"))
                .execute(&mut *tx)
                .await?;
        }

        sqlx::query(
            r#"
            CREATE TABLE users_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                avatar TEXT,
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                is_super_admin INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE projects_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                background_image TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users_new(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE doc_nodes_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                project_id INTEGER,
                parent_id INTEGER,
                name TEXT NOT NULL,
                node_type TEXT NOT NULL CHECK(node_type IN ('dir', 'doc')),
                content TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users_new(id) ON DELETE CASCADE,
                FOREIGN KEY (project_id) REFERENCES projects_new(id) ON DELETE SET NULL,
                FOREIGN KEY (parent_id) REFERENCES doc_nodes_new(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE shares_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                doc_id INTEGER NOT NULL,
                token TEXT NOT NULL UNIQUE,
                password_hash TEXT,
                password_ciphertext TEXT,
                expires_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users_new(id) ON DELETE CASCADE,
                FOREIGN KEY (doc_id) REFERENCES doc_nodes_new(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&mut *tx)
        .await?;

        let mut user_map = HashMap::new();
        for user in users {
            let new_id = sqlx::query(
                "INSERT INTO users_new
                 (username, password_hash, avatar, totp_secret, totp_enabled, is_super_admin, is_active, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, 0, 1, ?, ?)",
            )
            .bind(&user.username)
            .bind(&user.password_hash)
            .bind(&user.avatar)
            .bind(&user.totp_secret)
            .bind(user.totp_enabled)
            .bind(&user.created_at)
            .bind(&user.created_at)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            user_map.insert(user.id, new_id);
        }

        let mut project_map = HashMap::new();
        for project in projects {
            let new_user_id = *user_map
                .get(&project.user_id)
                .expect("legacy project user_id should exist");
            let new_id = sqlx::query(
                "INSERT INTO projects_new
                 (user_id, name, description, background_image, sort_order, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(new_user_id)
            .bind(&project.name)
            .bind(&project.description)
            .bind(&project.background_image)
            .bind(project.sort_order)
            .bind(&project.created_at)
            .bind(&project.updated_at)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            project_map.insert(project.id, new_id);
        }

        let mut node_map = HashMap::new();
        for node in &doc_nodes {
            let new_user_id = *user_map
                .get(&node.user_id)
                .expect("legacy doc node user_id should exist");
            let new_project_id = node
                .project_id
                .as_ref()
                .and_then(|id| project_map.get(id).copied());
            let new_id = sqlx::query(
                "INSERT INTO doc_nodes_new
                 (user_id, project_id, parent_id, name, node_type, content, sort_order, created_at, updated_at)
                 VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?)",
            )
            .bind(new_user_id)
            .bind(new_project_id)
            .bind(&node.name)
            .bind(&node.node_type)
            .bind(&node.content)
            .bind(node.sort_order)
            .bind(&node.created_at)
            .bind(&node.updated_at)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            node_map.insert(node.id.clone(), new_id);
        }

        for node in &doc_nodes {
            let Some(new_parent_id) = node
                .parent_id
                .as_ref()
                .and_then(|id| node_map.get(id).copied())
            else {
                continue;
            };

            let new_id = *node_map
                .get(&node.id)
                .expect("legacy doc node id should exist");

            sqlx::query("UPDATE doc_nodes_new SET parent_id = ? WHERE id = ?")
                .bind(new_parent_id)
                .bind(new_id)
                .execute(&mut *tx)
                .await?;
        }

        for share in shares {
            let new_user_id = *user_map
                .get(&share.user_id)
                .expect("legacy share user_id should exist");
            let new_doc_id = *node_map
                .get(&share.doc_id)
                .expect("legacy share doc_id should exist");

            sqlx::query(
                "INSERT INTO shares_new (user_id, doc_id, token, password_hash, password_ciphertext, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(new_user_id)
            .bind(new_doc_id)
            .bind(&share.token)
            .bind(&share.password_hash)
            .bind(&share.password_ciphertext)
            .bind(&share.expires_at)
            .bind(&share.created_at)
            .execute(&mut *tx)
            .await?;
        }

        for table in ["shares", "doc_nodes", "projects", "users"] {
            sqlx::query(&format!("DROP TABLE {table}"))
                .execute(&mut *tx)
                .await?;
        }

        for (from, to) in [
            ("users_new", "users"),
            ("projects_new", "projects"),
            ("doc_nodes_new", "doc_nodes"),
            ("shares_new", "shares"),
        ] {
            sqlx::query(&format!("ALTER TABLE {from} RENAME TO {to}"))
                .execute(&mut *tx)
                .await?;
        }

        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn table_columns(db: &Database, table: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(&db.pool)
            .await?;

        rows.into_iter()
            .map(|row| row.try_get::<String, _>("name").map_err(Into::into))
            .collect()
    }

    async fn index_names(db: &Database, table: &str) -> Result<Vec<(String, bool)>> {
        let rows = sqlx::query(&format!("PRAGMA index_list({table})"))
            .fetch_all(&db.pool)
            .await?;

        rows.into_iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>("name")?,
                    row.try_get::<i64, _>("unique")? == 1,
                ))
            })
            .collect()
    }

    async fn table_sql(db: &Database, table: &str) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_optional(&db.pool)
            .await
            .map_err(Into::into)
    }

    async fn foreign_keys(db: &Database, table: &str) -> Result<Vec<(String, String, String)>> {
        let rows = sqlx::query(&format!("PRAGMA foreign_key_list({table})"))
            .fetch_all(&db.pool)
            .await?;

        rows.into_iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>("table")?,
                    row.try_get::<String, _>("from")?,
                    row.try_get::<String, _>("on_delete")?,
                ))
            })
            .collect()
    }

    #[tokio::test]
    async fn migrate_creates_mcp_tables_and_preserves_agent_provider_rows() -> Result<()> {
        let db = Database::new("sqlite::memory:").await?;
        db.migrate().await?;

        sqlx::query(
            r#"
            INSERT INTO users (username, password_hash)
            VALUES ('mcp-tester', 'hash')
        "#,
        )
        .execute(&db.pool)
        .await?;
        let user_id = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM users WHERE username = 'mcp-tester' LIMIT 1",
        )
        .fetch_one(&db.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO agent_providers (
                user_id, name, kind, base_url, api_key_ciphertext, api, remote_models, is_active
            ) VALUES (?, 'primary', 'openai', 'https://api.openai.com/v1', 'ciphertext', 'responses', '[]', 1)
        "#,
        )
        .bind(user_id)
        .execute(&db.pool)
        .await?;

        db.migrate().await?;

        let provider_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agent_providers WHERE user_id = ?")
                .bind(user_id)
                .fetch_one(&db.pool)
                .await?;
        assert_eq!(provider_count, 1);

        let settings_columns = table_columns(&db, "agent_mcp_settings").await?;
        assert!(settings_columns.contains(&"user_id".to_string()));
        assert!(settings_columns.contains(&"enabled".to_string()));

        let settings_indexes = index_names(&db, "agent_mcp_settings").await?;
        assert!(settings_indexes.iter().any(|(name, unique)| {
            name.contains("agent_mcp_settings") && *unique
        }));

        let server_columns = table_columns(&db, "agent_mcp_servers").await?;
        assert!(server_columns.contains(&"config_version".to_string()));

        let server_indexes = index_names(&db, "agent_mcp_servers").await?;
        assert!(server_indexes
            .iter()
            .any(|(name, _)| name.contains("user_id")));
        assert!(server_indexes
            .iter()
            .any(|(name, _)| name.contains("enabled")));
        assert!(server_indexes
            .iter()
            .any(|(name, _)| name.contains("updated_at")));

        Ok(())
    }

    #[tokio::test]
    async fn migrate_repairs_partial_mcp_tables_and_restores_indexes() -> Result<()> {
        let db = Database::new("sqlite::memory:").await?;

        sqlx::query(
            r#"
            CREATE TABLE agent_mcp_settings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&db.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE agent_mcp_servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                transport TEXT NOT NULL,
                url TEXT,
                command TEXT,
                args_json TEXT NOT NULL DEFAULT '[]',
                env_ciphertext TEXT,
                auth_type TEXT NOT NULL DEFAULT 'none',
                auth_config_ciphertext TEXT,
                custom_headers_ciphertext TEXT,
                tools_snapshot TEXT NOT NULL DEFAULT '[]',
                resources_snapshot TEXT NOT NULL DEFAULT '[]',
                prompts_snapshot TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&db.pool)
        .await?;

        db.migrate().await?;

        let settings_sql = table_sql(&db, "agent_mcp_settings")
            .await?
            .expect("settings sql should exist");
        assert!(
            settings_sql.contains("updated_at TEXT NOT NULL DEFAULT (datetime('now'))"),
            "settings schema should restore updated_at default and constraint"
        );
        assert!(
            settings_sql.contains("FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE"),
            "settings schema should restore foreign key"
        );

        let settings_columns = table_columns(&db, "agent_mcp_settings").await?;
        assert!(settings_columns.contains(&"updated_at".to_string()));
        assert_eq!(
            foreign_keys(&db, "agent_mcp_settings").await?,
            vec![("users".to_string(), "user_id".to_string(), "CASCADE".to_string())]
        );
        let server_columns = table_columns(&db, "agent_mcp_servers").await?;
        assert!(server_columns.contains(&"updated_at".to_string()));
        assert!(server_columns.contains(&"config_version".to_string()));
        assert!(server_columns.contains(&"last_status".to_string()));
        assert!(server_columns.contains(&"last_error".to_string()));
        assert!(server_columns.contains(&"last_sync_at".to_string()));

        let server_sql = table_sql(&db, "agent_mcp_servers")
            .await?
            .expect("server sql should exist");
        assert!(
            server_sql.contains("CHECK(transport IN ('sse', 'streamable-http', 'stdio'))"),
            "server schema should restore transport check constraint"
        );
        assert!(
            server_sql.contains("FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE"),
            "server schema should restore foreign key"
        );
        assert!(
            server_sql.contains("config_version INTEGER NOT NULL DEFAULT 1"),
            "server schema should restore config_version default"
        );
        assert_eq!(
            foreign_keys(&db, "agent_mcp_servers").await?,
            vec![("users".to_string(), "user_id".to_string(), "CASCADE".to_string())]
        );

        let settings_indexes = index_names(&db, "agent_mcp_settings").await?;
        assert!(settings_indexes
            .iter()
            .any(|(name, unique)| name.contains("user_id") && *unique));

        let server_indexes = index_names(&db, "agent_mcp_servers").await?;
        assert!(server_indexes
            .iter()
            .any(|(name, _)| name.contains("user_id")));
        assert!(server_indexes
            .iter()
            .any(|(name, _)| name.contains("enabled")));
        assert!(server_indexes
            .iter()
            .any(|(name, _)| name.contains("updated_at")));

        Ok(())
    }

    #[tokio::test]
    async fn migrate_repairs_dirty_mcp_data_with_orphans_and_duplicate_settings() -> Result<()> {
        let db = Database::new("sqlite::memory:").await?;

        sqlx::query(
            r#"
            CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL,
                password_hash TEXT NOT NULL,
                avatar TEXT,
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                is_super_admin INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&db.pool)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO users (id, username, password_hash)
            VALUES (1, 'live-user', 'hash')
        "#,
        )
        .execute(&db.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE agent_mcp_settings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&db.pool)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO agent_mcp_settings (user_id, enabled, created_at)
            VALUES (1, 0, '2024-01-01T00:00:00Z'),
                   (1, 1, '2024-01-02T00:00:00Z')
        "#,
        )
        .execute(&db.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE agent_mcp_servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                transport TEXT NOT NULL,
                url TEXT,
                command TEXT,
                args_json TEXT NOT NULL DEFAULT '[]',
                env_ciphertext TEXT,
                auth_type TEXT NOT NULL DEFAULT 'none',
                auth_config_ciphertext TEXT,
                custom_headers_ciphertext TEXT,
                tools_snapshot TEXT NOT NULL DEFAULT '[]',
                resources_snapshot TEXT NOT NULL DEFAULT '[]',
                prompts_snapshot TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&db.pool)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO agent_mcp_servers (
                user_id, name, enabled, transport, url, command, args_json,
                env_ciphertext, auth_type, auth_config_ciphertext,
                custom_headers_ciphertext, tools_snapshot, resources_snapshot,
                prompts_snapshot, created_at
            ) VALUES
                (1, 'live-server', 1, 'sse', 'https://example.com', NULL, '[]', NULL, 'none', NULL, NULL, '[]', '[]', '[]', '2024-01-01T00:00:00Z'),
                (999, 'orphan-server', 1, 'sse', 'https://example.org', NULL, '[]', NULL, 'none', NULL, NULL, '[]', '[]', '[]', '2024-01-02T00:00:00Z')
        "#,
        )
        .execute(&db.pool)
        .await?;

        db.migrate().await?;

        let settings_rows = sqlx::query(
            "SELECT id, user_id, enabled FROM agent_mcp_settings ORDER BY id",
        )
        .fetch_all(&db.pool)
        .await?;
        assert_eq!(settings_rows.len(), 1);
        assert_eq!(settings_rows[0].try_get::<i64, _>("user_id")?, 1);
        assert_eq!(settings_rows[0].try_get::<i64, _>("enabled")?, 1);

        let server_rows = sqlx::query(
            "SELECT id, user_id, name FROM agent_mcp_servers ORDER BY id",
        )
        .fetch_all(&db.pool)
        .await?;
        assert_eq!(server_rows.len(), 1);
        assert_eq!(server_rows[0].try_get::<i64, _>("user_id")?, 1);
        assert_eq!(server_rows[0].try_get::<String, _>("name")?, "live-server");

        Ok(())
    }
}
