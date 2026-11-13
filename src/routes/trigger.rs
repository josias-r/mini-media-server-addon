use super::{AppState, require_valid_api_key};
use crate::matcher::find_best_file_match;
use crate::stream_builder::get_series_params;
use axum::{
    extract::{Path, Query, Request, State},
    response::{IntoResponse, Response},
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tower::ServiceExt;
use tower_http::services::ServeFile;

fn normalize_valid_hash(hash: &str) -> Option<String> {
    let trimmed = hash.trim();
    if trimmed.len() == 40 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(trimmed.to_lowercase())
    } else {
        None
    }
}

pub async fn trigger_download(
    State(state): State<AppState>,
    Path((api_key, stremio_id, info_hash)): Path<(String, String, String)>,
    Query(params): Query<HashMap<String, String>>,
    req: Request,
) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    let valid_initial_hash = normalize_valid_hash(&info_hash);
    if let Some(ref h) = valid_initial_hash {
        let _ = state.db.update_watch_history(&api_key, h, "torrent");
    }

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let task_state = state.clone();
    let task_api_key = api_key.clone();
    let task_stremio_id = stremio_id.clone();
    let magnet_opt = params.get("magnet").cloned();
    let initial_hash_clone = valid_initial_hash.clone();

    tokio::spawn(async move {
        let base_id = task_stremio_id
            .split(':')
            .next()
            .unwrap_or(&task_stremio_id)
            .to_string();

        let mut existing_hashes = HashSet::new();
        if initial_hash_clone.is_none() {
            for t in task_state.qbit.get_torrents_by_tag(&base_id).await {
                existing_hashes.insert(t.hash.to_lowercase());
            }
        }

        let mut target_hash = initial_hash_clone.clone();
        if let Some(ref magnet) = magnet_opt {
            let resolved = task_state
                .qbit
                .resolve_and_add_torrent(magnet, &base_id, target_hash.as_deref())
                .await;
            if target_hash.is_none() {
                target_hash = resolved;
            }
        }

        let mut files = vec![];
        for _ in 0..120 {
            if target_hash.is_none() {
                let tagged = task_state.qbit.get_torrents_by_tag(&base_id).await;
                let new_torrents: Vec<_> = tagged
                    .iter()
                    .filter(|t| !existing_hashes.contains(&t.hash.to_lowercase()))
                    .collect();
                if new_torrents.len() == 1 {
                    target_hash = Some(new_torrents[0].hash.to_lowercase());
                } else if tagged.len() == 1 {
                    target_hash = Some(tagged[0].hash.to_lowercase());
                }
            }

            if let Some(ref h) = target_hash {
                files = task_state.qbit.get_torrent_files(h).await;
                if !files.is_empty() {
                    break;
                }
            }

            log::debug!(
                "Torrent files not yet available for {:?}, waiting...",
                target_hash
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        if let Some(ref h) = target_hash {
            if initial_hash_clone.is_none() {
                let _ = task_state
                    .db
                    .update_watch_history(&task_api_key, h, "torrent");
            }

            if !files.is_empty() {
                let (is_series, season, episode, expected_title) =
                    get_series_params(&task_state, &task_stremio_id).await;

                let file_infos: Vec<(&str, u64)> =
                    files.iter().map(|f| (f.name.as_str(), f.size)).collect();
                let best_idx =
                    find_best_file_match(&file_infos, &expected_title, is_series, season, episode);

                if let Some(chosen_idx) = best_idx {
                    let chosen_id = files[chosen_idx].index.unwrap_or(chosen_idx);
                    let unwanted_ids: Vec<usize> = files
                        .iter()
                        .enumerate()
                        .filter(|&(i, _)| i != chosen_idx)
                        .map(|(i, f)| f.index.unwrap_or(i))
                        .collect();

                    if !unwanted_ids.is_empty() {
                        task_state
                            .qbit
                            .set_file_priorities(h, &unwanted_ids, 0)
                            .await;
                    }
                    task_state
                        .qbit
                        .set_file_priorities(h, &[chosen_id], 1)
                        .await;
                    task_state.qbit.resume_torrent(h).await;
                }
            } else {
                log::debug!("Fallbacking to placeholder for {}", h);
            }
        } else {
            log::warn!(
                "Could not resolve torrent hash for stremio_id {}",
                task_stremio_id
            );
        }

        let _ = tx.send(());
    });

    // Wait briefly so fast .torrent / cached metadata torrents have priorities applied before
    // returning placeholder.mp4, while slower magnet metadata fetches continue in the background.
    let _ = tokio::time::timeout(Duration::from_secs(12), rx).await;

    let placeholder = ServeFile::new(std::path::Path::new("placeholder.mp4"));
    // Stremio will play this placeholder and the user can check back later when it's ready.
    placeholder.oneshot(req).await.unwrap().into_response()
}
