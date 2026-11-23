use super::{AppState, PlaceholderState, build_live_placeholder_redirect, require_valid_api_key};
use crate::domain::{file_exists_nonempty, resolve_safe_download_path};
use axum::http::StatusCode;
use axum::http::header::RANGE;
use axum::{
    extract::{Path, Query, Request, State},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub async fn stream_file(
    State(state): State<AppState>,
    Path((api_key, info_hash)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    req: Request,
) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    let file_path = params.get("filePath").map(String::as_str).unwrap_or("");
    let Some(full_path) = resolve_safe_download_path(&state.config.download_path, file_path) else {
        log::warn!(
            "Security violation attempt or invalid path for {}: {:?}",
            info_hash,
            file_path
        );
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    };

    log::info!(
        "Stream request for info_hash: {}, filePath: {}",
        info_hash,
        file_path
    );

    if !file_exists_nonempty(&state.config.download_path, file_path) {
        log::warn!(
            "File does not exist or is empty on disk (yet): {:?}; serving missing-files placeholder",
            full_path
        );
        return build_live_placeholder_redirect(
            &state.config.public_url,
            &api_key,
            "stream",
            "unknown",
            &info_hash,
            PlaceholderState::FilesMissing,
            None,
            Some(file_path),
        );
    }

    let range_header = req
        .headers()
        .get(RANGE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("none");
    log::info!("Serving {:?} directly (Range: {})", full_path, range_header);

    ServeFile::new(full_path)
        .oneshot(req)
        .await
        .unwrap()
        .into_response()
}
