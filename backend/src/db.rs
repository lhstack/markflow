use anyhow::Result;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use std::str::FromStr;
use uuid::Uuid;

pub struct Database {
    pub pool: SqlitePool,
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
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                avatar TEXT,
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
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
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                project_id TEXT,
                parent_id TEXT,
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
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                token TEXT NOT NULL UNIQUE,
                password_hash TEXT,
                expires_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (doc_id) REFERENCES doc_nodes(id) ON DELETE CASCADE
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        self.ensure_doc_nodes_project_column().await?;
        self.backfill_existing_doc_project_ids().await?;

        // Indexes
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

        Ok(())
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
            sqlx::query("ALTER TABLE doc_nodes ADD COLUMN project_id TEXT")
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    async fn backfill_existing_doc_project_ids(&self) -> Result<()> {
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
                "SELECT id FROM projects WHERE user_id = ? ORDER BY sort_order ASC, created_at ASC LIMIT 1"
            )
            .bind(&user_id)
            .fetch_optional(&self.pool)
            .await?;

            if default_project_id.is_none() {
                let created_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO projects (id, user_id, name, description, background_image, sort_order)
                     VALUES (?, ?, '默认项目', '系统迁移自动创建的项目', NULL, 0)"
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
}
