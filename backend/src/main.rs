mod auth;
mod db;
mod models;
mod routes;

use axum::{
    routing::{delete, get, post, put},
    Extension, Router,
};
use chrono::{Local, NaiveDate};
use serde::Deserialize;
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{
    fmt::{time::FormatTime, writer::MakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use db::Database;
use routes::static_files::static_handler;

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    database_url: Option<String>,
    port: Option<String>,
    jwt_secret: Option<String>,
    rust_log: Option<String>,
    upload_dir: Option<String>,
    log_to_file: Option<bool>,
    log_dir: Option<String>,
    log_file_name: Option<String>,
    log_rotate_size_mb: Option<u64>,
    log_rotate_days: Option<i64>,
    log_keep_days: Option<i64>,
}

#[derive(Debug, Clone)]
struct LogFileOptions {
    dir: PathBuf,
    file_name: String,
    rotate_size_bytes: u64,
    rotate_days: i64,
    keep_days: i64,
}

#[derive(Clone)]
struct LocalTime;

impl FormatTime for LocalTime {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> fmt::Result {
        write!(w, "{}", Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

fn load_config() -> FileConfig {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let config_path = exe_dir
        .map(|d| d.join("config.toml"))
        .filter(|p| p.exists());

    match config_path {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(cfg) => {
                    println!("Loaded config from: {}", path.display());
                    cfg
                }
                Err(e) => {
                    eprintln!("Failed to parse config.toml: {e}");
                    FileConfig::default()
                }
            },
            Err(e) => {
                eprintln!("Failed to read config.toml: {e}");
                FileConfig::default()
            }
        },
        None => FileConfig::default(),
    }
}

fn cfg_val(env_key: &str, file_val: Option<String>, default: &str) -> String {
    std::env::var(env_key)
        .ok()
        .or(file_val)
        .unwrap_or_else(|| default.to_string())
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn cfg_bool(env_key: &str, file_val: Option<bool>, default: bool) -> bool {
    std::env::var(env_key)
        .ok()
        .and_then(|v| parse_bool(&v))
        .or(file_val)
        .unwrap_or(default)
}

fn cfg_u64(env_key: &str, file_val: Option<u64>, default: u64) -> u64 {
    std::env::var(env_key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .or(file_val)
        .unwrap_or(default)
}

fn cfg_i64(env_key: &str, file_val: Option<i64>, default: i64) -> i64 {
    std::env::var(env_key)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .or(file_val)
        .unwrap_or(default)
}

fn split_file_name(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) => (name[..i].to_string(), name[i..].to_string()),
        None => (name.to_string(), String::new()),
    }
}

struct RotatingFileState {
    opts: LogFileOptions,
    active_path: PathBuf,
    file: Option<File>,
    current_size: u64,
    opened_day: NaiveDate,
    file_stem: String,
    file_ext: String,
}

impl RotatingFileState {
    fn new(opts: LogFileOptions) -> io::Result<Self> {
        fs::create_dir_all(&opts.dir)?;

        let active_path = opts.dir.join(&opts.file_name);
        let (file_stem, file_ext) = split_file_name(&opts.file_name);

        let mut state = Self {
            opts,
            active_path,
            file: None,
            current_size: 0,
            opened_day: Local::now().date_naive(),
            file_stem,
            file_ext,
        };

        state.open_active_file()?;
        Ok(state)
    }

    fn open_active_file(&mut self) -> io::Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.active_path)?;

        self.current_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        self.opened_day = Local::now().date_naive();
        self.file = Some(file);
        Ok(())
    }

    fn should_rotate(&self, incoming_len: usize) -> bool {
        let by_size = self.opts.rotate_size_bytes > 0
            && self.current_size.saturating_add(incoming_len as u64) > self.opts.rotate_size_bytes;

        let by_days = self.opts.rotate_days > 0
            && (Local::now().date_naive() - self.opened_day).num_days() >= self.opts.rotate_days;

        by_size || by_days
    }

    fn rotate_if_needed(&mut self, incoming_len: usize) -> io::Result<()> {
        if self.should_rotate(incoming_len) {
            self.rotate()?;
        }
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        if let Some(mut file) = self.file.take() {
            file.flush()?;
        }

        if self.active_path.exists() {
            let len = fs::metadata(&self.active_path)
                .map(|m| m.len())
                .unwrap_or(0);
            if len > 0 {
                let suffix = Local::now().format("%Y%m%d-%H%M%S");
                let rotated_name = format!("{}-{}{}", self.file_stem, suffix, self.file_ext);
                let rotated_path = self.opts.dir.join(rotated_name);
                let _ = fs::rename(&self.active_path, &rotated_path);
            }
        }

        self.cleanup_old_files();
        self.open_active_file()
    }

    fn cleanup_old_files(&self) {
        if self.opts.keep_days <= 0 {
            return;
        }

        let Some(cutoff) = SystemTime::now().checked_sub(Duration::from_secs(
            (self.opts.keep_days as u64) * 24 * 60 * 60,
        )) else {
            return;
        };

        let prefix = format!("{}-", self.file_stem);

        if let Ok(entries) = fs::read_dir(&self.opts.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };

                if file_name == self.opts.file_name {
                    continue;
                }

                if !file_name.starts_with(&prefix) {
                    continue;
                }

                if !self.file_ext.is_empty() && !file_name.ends_with(&self.file_ext) {
                    continue;
                }

                let Ok(meta) = entry.metadata() else {
                    continue;
                };

                let Ok(modified) = meta.modified() else {
                    continue;
                };

                if modified < cutoff {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }

    fn write_buf(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.rotate_if_needed(buf.len())?;

        if self.file.is_none() {
            self.open_active_file()?;
        }

        let Some(file) = self.file.as_mut() else {
            return Err(io::Error::other("log file unavailable"));
        };

        file.write_all(buf)?;
        self.current_size = self.current_size.saturating_add(buf.len() as u64);
        Ok(buf.len())
    }

    fn flush_buf(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
        }
        Ok(())
    }
}

#[derive(Clone)]
struct RotatingFileWriter {
    state: Arc<Mutex<RotatingFileState>>,
}

impl RotatingFileWriter {
    fn new(opts: LogFileOptions) -> io::Result<Self> {
        Ok(Self {
            state: Arc::new(Mutex::new(RotatingFileState::new(opts)?)),
        })
    }
}

struct RotatingFileGuard {
    state: Arc<Mutex<RotatingFileState>>,
}

impl<'a> MakeWriter<'a> for RotatingFileWriter {
    type Writer = RotatingFileGuard;

    fn make_writer(&'a self) -> Self::Writer {
        RotatingFileGuard {
            state: Arc::clone(&self.state),
        }
    }
}

impl Write for RotatingFileGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log writer lock poisoned"))?;
        guard.write_buf(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log writer lock poisoned"))?;
        guard.flush_buf()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let file_cfg = load_config();

    // env > config.toml > defaults
    let rust_log = cfg_val(
        "RUST_LOG",
        file_cfg.rust_log,
        "markflow=info,tower_http=warn",
    );
    let database_url = cfg_val("DATABASE_URL", file_cfg.database_url, "sqlite:markflow.db");
    let port = cfg_val("PORT", file_cfg.port, "3000");
    let jwt_secret = cfg_val(
        "JWT_SECRET",
        file_cfg.jwt_secret,
        "markflow_dev_secret_change_in_production",
    );
    let upload_dir = cfg_val("UPLOAD_DIR", file_cfg.upload_dir, "uploads");

    let log_to_file = cfg_bool("LOG_TO_FILE", file_cfg.log_to_file, false);
    let log_dir = cfg_val("LOG_DIR", file_cfg.log_dir, "logs");
    let log_file_name = cfg_val("LOG_FILE_NAME", file_cfg.log_file_name, "markflow.log");
    let log_rotate_size_mb = cfg_u64("LOG_ROTATE_SIZE_MB", file_cfg.log_rotate_size_mb, 50);
    let log_rotate_days = cfg_i64("LOG_ROTATE_DAYS", file_cfg.log_rotate_days, 1);
    let log_keep_days = cfg_i64("LOG_KEEP_DAYS", file_cfg.log_keep_days, 14);

    std::env::set_var("RUST_LOG", &rust_log);
    std::env::set_var("JWT_SECRET", &jwt_secret);
    std::env::set_var("UPLOAD_DIR", &upload_dir);

    fs::create_dir_all(&upload_dir)?;

    let console_layer = tracing_subscriber::fmt::layer()
        .with_timer(LocalTime)
        .with_target(true)
        .with_ansi(false);

    if log_to_file {
        let writer = RotatingFileWriter::new(LogFileOptions {
            dir: PathBuf::from(&log_dir),
            file_name: log_file_name.clone(),
            rotate_size_bytes: log_rotate_size_mb.saturating_mul(1024 * 1024),
            rotate_days: log_rotate_days,
            keep_days: log_keep_days,
        })?;

        let file_layer = tracing_subscriber::fmt::layer()
            .with_timer(LocalTime)
            .with_target(true)
            .with_ansi(false)
            .with_writer(writer);

        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(&rust_log))
            .with(console_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(&rust_log))
            .with(console_layer)
            .init();
    }

    tracing::info!(
        "Resolved config: port={}, database={}, upload_dir={}, log_to_file={}, log_dir={}, file={}, rotate_size_mb={}, rotate_days={}, keep_days={}",
        port,
        database_url,
        upload_dir,
        log_to_file,
        log_dir,
        log_file_name,
        log_rotate_size_mb,
        log_rotate_days,
        log_keep_days,
    );

    let db = Database::new(&database_url).await?;
    db.migrate().await?;
    let db = Arc::new(db);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api = Router::new()
        .route("/auth/captcha", get(routes::auth::get_captcha))
        .route("/auth/register", post(routes::auth::register))
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/login/2fa", post(routes::auth::login_2fa))
        .route("/auth/me", get(routes::auth::me))
        .route("/auth/profile", put(routes::auth::update_profile))
        .route("/auth/password", put(routes::auth::change_password))
        .route("/auth/2fa/setup", post(routes::auth::setup_2fa))
        .route("/auth/2fa/confirm", post(routes::auth::confirm_2fa))
        .route("/auth/2fa/disable", post(routes::auth::disable_2fa))
        .route("/uploads", post(routes::uploads::upload_file))
        .route(
            "/projects",
            get(routes::projects::list_projects).post(routes::projects::create_project),
        )
        .route(
            "/projects/:id",
            put(routes::projects::update_project).delete(routes::projects::delete_project),
        )
        .route(
            "/docs",
            get(routes::documents::list_tree).post(routes::documents::create_node),
        )
        .route(
            "/docs/:id",
            get(routes::documents::get_node)
                .put(routes::documents::update_node)
                .delete(routes::documents::delete_node),
        )
        .route("/docs/:id/move", put(routes::documents::move_node))
        .route("/shares", post(routes::shares::create_share))
        .route("/shares/doc/:doc_id", get(routes::shares::list_shares))
        .route("/shares/:id", delete(routes::shares::delete_share))
        .route("/s/:token", get(routes::shares::get_share))
        .route("/s/:token/verify", post(routes::shares::verify_share))
        .route("/s/:token/content", get(routes::shares::get_share_content));

    let app = Router::new()
        .nest("/api", api)
        .fallback(static_handler)
        .layer(Extension(db))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("MarkFlow running at http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
