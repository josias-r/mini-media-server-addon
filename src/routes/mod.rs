pub mod api;
pub mod catalog;
pub mod play;
pub mod stream;
pub mod trigger;

pub use crate::config::AppState;
use crate::config::Config;
pub use crate::placeholder::PlaceholderState;
use crate::placeholder::serve_live_placeholder;
use crate::stream_builder::build_media_streams;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Json, Redirect, Response};
use axum::routing::get;
use serde_json::json;
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

pub fn require_valid_api_key(state: &AppState, api_key: &str) -> Option<Response> {
    if state.db.validate_api_key(api_key) {
        None
    } else {
        Some((StatusCode::UNAUTHORIZED, "Invalid API Key").into_response())
    }
}

async fn manifest(State(state): State<AppState>, Path(api_key): Path<String>) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    Json(json!({
        "id": "mini-media-server.addon",
        "version": env!("CARGO_PKG_VERSION"),
        "name": "Mini Media Server",
        "description": "A tiny Stremio addon acting similar to a self-hostable debrid service.",
        "resources": ["catalog", "stream"],
        "types": ["movie", "series"],
        "catalogs": [
            { "type": "movie", "id": "mms_user_movies", "name": "My History" },
            { "type": "series", "id": "mms_user_series", "name": "My History" },
            { "type": "movie", "id": "mms_community_movies", "name": "Community" },
            { "type": "series", "id": "mms_community_series", "name": "Community" }
        ]
    }))
    .into_response()
}

async fn stream_handler(
    State(state): State<AppState>,
    Path((api_key, id_ext)): Path<(String, String)>,
    is_series: bool,
) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    let id = id_ext.replace(".json", "");
    Json(build_media_streams(&state, &api_key, &id, is_series).await).into_response()
}

pub fn create_router(state: AppState) -> Router {
    let static_dir = Config::static_dir();
    let spa_service = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(format!("{}/index.html", static_dir)));

    Router::new()
        .route("/{api_key}/manifest.json", get(manifest))
        .route(
            "/{api_key}/catalog/{media_type}/{catalog_id}",
            get(catalog::catalog_handler),
        )
        .route(
            "/{api_key}/catalog/{media_type}/{catalog_id}/{extra}",
            get(catalog::catalog_with_extra_handler),
        )
        .route(
            "/{api_key}/stream/movie/{id}",
            get(|s, p| stream_handler(s, p, false)),
        )
        .route(
            "/{api_key}/stream/series/{id}",
            get(|s, p| stream_handler(s, p, true)),
        )
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
            get(serve_live_placeholder),
        )
        .nest("/api", api::router())
        .fallback_service(spa_service)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
