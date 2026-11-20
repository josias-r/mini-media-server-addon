pub mod admin;
pub mod api;
pub mod catalog;
pub mod manifest;
pub mod movie;
pub mod play;
pub mod series;
pub mod stream;
pub mod trigger;

use crate::config::Config;
pub use crate::placeholder_stream::PlaceholderState;
use crate::qbittorrent::QbitClient;
use axum::Router;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use reqwest::Client;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

/// Builds a non-cacheable 307 redirect to the live-updating virtual fMP4 placeholder stream.
#[allow(clippy::too_many_arguments)]
pub fn build_live_placeholder_redirect(
    public_url: &str,
    api_key: &str,
    mode: &str,
    stremio_id: &str,
    info_hash: &str,
    initial: PlaceholderState,
    file_idx: Option<usize>,
    file_path: Option<&str>,
) -> Response {
    let base = public_url.trim_end_matches('/');
    let mut url = format!(
        "{}/{}/placeholder-live/{}/{}/{}.mp4?initial={}",
        base,
        urlencoding::encode(api_key),
        urlencoding::encode(mode),
        urlencoding::encode(stremio_id),
        urlencoding::encode(info_hash),
        initial.id(),
    );
    if let Some(idx) = file_idx {
        url.push_str(&format!("&fileIdx={}", idx));
    }
    if let Some(fp) = file_path.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&filePath={}", urlencoding::encode(fp)));
    }
    let mut res = Redirect::temporary(&url).into_response();
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub qbit: Arc<QbitClient>,
    pub http_client: Client,
    pub db: Arc<crate::db::DbClient>,
    pub admin_session_token: String,
    pub triggered_torrents: Arc<RwLock<HashSet<String>>>,
    pub placeholder_bank: Option<Arc<crate::placeholder_stream::PlaceholderBank>>,
    pub live_sessions: Arc<crate::placeholder_stream::LiveSessionStore>,
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
        .route(
            "/{api_key}/catalog/{media_type}/{catalog_id}",
            get(catalog::catalog_handler),
        )
        .route(
            "/{api_key}/catalog/{media_type}/{catalog_id}/{extra}",
            get(catalog::catalog_with_extra_handler),
        )
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
        .route(
            "/{api_key}/placeholder-live/{mode}/{stremio_id}/{info_hash}",
            get(crate::placeholder_stream::serve_live_placeholder),
        )
        .nest("/api", api::router())
        .fallback_service(spa_service)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
