use anyhow::Result;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    FromRow, Row, SqlitePool,
};
use std::{collections::HashMap, str::FromStr};
use uuid::Uuid;

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

        if self.uses_legacy_text_ids().await? {
            self.ensure_legacy_doc_nodes_project_column().await?;
            self.backfill_existing_doc_project_ids_legacy().await?;
            self.migrate_text_primary_keys_to_integer().await?;
        }

        self.ensure_doc_nodes_project_column().await?;
        self.backfill_existing_doc_project_ids().await?;
        self.create_indexes().await?;

        Ok(())
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
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
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
        Ok(())
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
            "SELECT user_id, doc_id, token, password_hash, expires_at, created_at
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
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
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
                "INSERT INTO users_new (username, password_hash, avatar, totp_secret, totp_enabled, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&user.username)
            .bind(&user.password_hash)
            .bind(&user.avatar)
            .bind(&user.totp_secret)
            .bind(user.totp_enabled)
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
                "INSERT INTO shares_new (user_id, doc_id, token, password_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(new_user_id)
            .bind(new_doc_id)
            .bind(&share.token)
            .bind(&share.password_hash)
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
