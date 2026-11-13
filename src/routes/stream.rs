use super::{AppState, require_valid_api_key};
use axum::http::StatusCode;
use axum::http::header::RANGE;
use axum::{
    extract::{Path, Query, Request, State},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::path::{Component, Path as StdPath, PathBuf};
use tower::ServiceExt;
use tower_http::services::ServeFile;

fn resolve_safe_download_path(download_root: &str, relative: &str) -> Option<PathBuf> {
    if relative.is_empty() {
        return None;
    }

    let rel_path = StdPath::new(relative);
    let mut safe_rel = PathBuf::new();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(part) => safe_rel.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    if safe_rel.as_os_str().is_empty() {
        return None;
    }

    Some(StdPath::new(download_root).join(safe_rel))
}

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

    if !full_path.exists() {
        log::warn!("File does not exist (yet): {:?}", full_path);
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
