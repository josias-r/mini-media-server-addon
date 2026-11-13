pub mod admin;
pub mod api;
pub mod manifest;
pub mod movie;
pub mod play;
pub mod series;
pub mod stream;
pub mod trigger;

use crate::config::Config;
use crate::qbittorrent::QbitClient;
use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use reqwest::Client;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub qbit: Arc<QbitClient>,
    pub http_client: Client,
    pub db: Arc<crate::db::DbClient>,
    pub admin_session_token: String,
}

pub fn require_valid_api_key(state: &AppState, api_key: &str) -> Option<Response> {
    if state.db.validate_api_key(api_key) {
        None
    } else {
        Some((StatusCode::UNAUTHORIZED, "Invalid API Key").into_response())
    }
}

pub fn create_router(state: AppState) -> Router {
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "app/dist".to_string());
    let spa_service = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(format!("{}/index.html", static_dir)));

    Router::new()
        .route("/{api_key}/manifest.json", get(manifest::manifest))
        .route("/{api_key}/stream/movie/{id}", get(movie::movie_stream))
        .route("/{api_key}/stream/series/{id}", get(series::series_stream))
        .route(
            "/{api_key}/trigger-download/{stremio_id}/{info_hash}",
            get(trigger::trigger_download),
        )
        .route(
            "/{api_key}/play-file/{stremio_id}/{info_hash}",
            get(play::play_file),
        )
        .route(
            "/{api_key}/stream-file/{info_hash}",
            get(stream::stream_file),
        )
        .nest("/api", api::router())
        .fallback_service(spa_service)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
