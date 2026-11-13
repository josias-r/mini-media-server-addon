mod cinemeta;
mod config;
mod db;
mod jackett;
mod matcher;
mod models;
mod parser;
mod qbittorrent;
mod routes;
mod stream_builder;
mod worker;

use log::info;
use reqwest::Client;
use std::sync::Arc;

fn check_dependencies() {
    use std::process::Command;

    if Command::new("ffmpeg").arg("-version").output().is_err() {
        panic!(
            "ERROR: ffmpeg is not installed or not in PATH. It is required for stream remuxing."
        );
    }

    if Command::new("ffprobe").arg("-version").output().is_err() {
        panic!(
            "ERROR: ffprobe is not installed or not in PATH. It is required for stream remuxing."
        );
    }
}

#[tokio::main]
async fn main() {
    // Initialize env_logger with a default filter if RUST_LOG is not set
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,mini_media_server_addon=debug"),
    )
    .init();

    check_dependencies();

    let config = config::Config::load();
    let qbit = qbittorrent::QbitClient::new(config.clone());

    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| ".".to_string());
    let db_path = format!("{}/data.db", data_dir);
    let db = db::DbClient::new(&db_path);

    // Ensure admin password is generated/exists
    let _ = db.get_admin_password().unwrap();

    let state = routes::AppState {
        config: Arc::new(config.clone()),
        qbit: Arc::new(qbit),
        http_client: Client::builder().build().unwrap(),
        db: Arc::new(db),
        admin_session_token: uuid::Uuid::new_v4().to_string(),
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
