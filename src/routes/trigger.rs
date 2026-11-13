use super::{AppState, require_valid_api_key};
use crate::matcher::find_best_file_match;
use crate::stream_builder::get_series_params;
use axum::{
    extract::{Path, Query, Request, State},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::time::Duration;
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub async fn trigger_download(
    State(state): State<AppState>,
    Path((api_key, stremio_id, info_hash)): Path<(String, String, String)>,
    Query(params): Query<HashMap<String, String>>,
    req: Request,
) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    let _ = state
        .db
        .update_watch_history(&api_key, &info_hash, "torrent");

    let base_id = stremio_id.split(':').next().unwrap_or(&stremio_id);
    if let Some(magnet) = params.get("magnet") {
        state.qbit.add_torrent(magnet, base_id).await;
    }

    let mut files = vec![];
    for _ in 0..30 {
        files = state.qbit.get_torrent_files(&info_hash).await;
        if !files.is_empty() {
            break;
        }
        log::debug!(
            "Torrent files not yet available for {}, waiting...",
            info_hash
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let placeholder = ServeFile::new(std::path::Path::new("placeholder.mp4"));
    if files.is_empty() {
        log::debug!("Fallbacking to placeholder for {}", info_hash);
        return placeholder.oneshot(req).await.unwrap().into_response();
    }

    let (is_series, season, episode, expected_title) = get_series_params(&state, &stremio_id).await;

    let file_infos: Vec<(&str, u64)> = files.iter().map(|f| (f.name.as_str(), f.size)).collect();
    let best_idx = find_best_file_match(&file_infos, &expected_title, is_series, season, episode);

    if let Some(chosen_idx) = best_idx {
        let all_ids: Vec<usize> = (0..files.len()).collect();
        state
            .qbit
            .set_file_priorities(&info_hash, &all_ids, 0)
            .await;
        state
            .qbit
            .set_file_priorities(&info_hash, &[chosen_idx], 1)
            .await;
        state.qbit.resume_torrent(&info_hash).await;
    }

    // Stremio will play this placeholder and the user can check back later when it's ready.
    placeholder.oneshot(req).await.unwrap().into_response()
}
