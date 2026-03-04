use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use anyhow::Result;
use std::str::FromStr;

pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    pub async fn new(url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                avatar TEXT,
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#).execute(&self.pool).await?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS doc_nodes (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                parent_id TEXT,
                name TEXT NOT NULL,
                node_type TEXT NOT NULL CHECK(node_type IN ('dir', 'doc')),
                content TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (parent_id) REFERENCES doc_nodes(id) ON DELETE CASCADE
            )
        "#).execute(&self.pool).await?;

        sqlx::query(r#"
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
        "#).execute(&self.pool).await?;

        // Indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_doc_nodes_user_id ON doc_nodes(user_id)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_doc_nodes_parent_id ON doc_nodes(parent_id)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_shares_token ON shares(token)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_shares_doc_id ON shares(doc_id)").execute(&self.pool).await?;

        Ok(())
    }
}
