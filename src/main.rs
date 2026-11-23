mod cinemeta;
mod config;
mod db;
mod domain;
mod jackett;
mod models;
mod placeholder;
mod qbittorrent;
mod routes;
mod stream_builder;
mod worker;

use log::info;

#[tokio::main]
async fn main() {
    // Initialize env_logger with a default filter if RUST_LOG is not set
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,mini_media_server_addon=debug"),
    )
    .init();

    let config = config::Config::load();
    let state = config::AppState::from_config(config.clone());

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
