mod cinemeta;
mod config;
mod db;
mod jackett;
mod matcher;
mod models;
mod parser;
mod placeholder_stream;
mod qbittorrent;
mod routes;
mod stream_builder;
mod worker;

use log::info;
use reqwest::Client;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    // Initialize env_logger with a default filter if RUST_LOG is not set
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,mini_media_server_addon=debug"),
    )
    .init();

    let config = config::Config::load();
    let qbit = qbittorrent::QbitClient::new(config.clone());

    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| ".".to_string());
    let db_path = format!("{}/data.db", data_dir);
    let db = db::DbClient::new(&db_path);

    // Ensure admin password is generated/exists
    let _ = db.get_admin_password().unwrap();

    let placeholders_dir =
        std::env::var("PLACEHOLDERS_DIR").unwrap_or_else(|_| "placeholders".to_string());
    let duration_hours = std::env::var("PLACEHOLDER_DURATION_HOURS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(placeholder_stream::DEFAULT_DURATION_HOURS);
    let placeholder_bank = match placeholder_stream::PlaceholderBank::load(
        std::path::Path::new(&placeholders_dir),
        duration_hours,
    ) {
        Ok(bank) => {
            info!(
                "Loaded virtual placeholder bank from {:?} (slot_size={} B, total_len={} MiB, fragments={})",
                placeholders_dir,
                bank.slot_size,
                bank.total_len / (1024 * 1024),
                bank.total_fragments
            );
            Some(Arc::new(bank))
        }
        Err(e) => {
            log::warn!(
                "Could not load placeholder bank from {:?}: {}; falling back to static placeholder.mp4",
                placeholders_dir,
                e
            );
            None
        }
    };

    let state = routes::AppState {
        config: Arc::new(config.clone()),
        qbit: Arc::new(qbit),
        http_client: Client::builder().build().unwrap(),
        db: Arc::new(db),
        admin_session_token: uuid::Uuid::new_v4().to_string(),
        triggered_torrents: Arc::new(RwLock::new(HashSet::new())),
        placeholder_bank,
        live_sessions: Arc::new(placeholder_stream::LiveSessionStore::default()),
    };

    // Spawn background retention worker
    worker::start_retention_worker(state.clone());

    let app = routes::create_router(state);

    let addr = format!("0.0.0.0:{}", config.port);
    info!(
        "Modular Stremio Add-on server is running at http://localhost:{}",
        config.port
    );
    info!("Manifest URL: {}/manifest.json", config.public_url);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
