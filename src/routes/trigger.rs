use super::{AppState, PlaceholderState, build_live_placeholder_redirect, require_valid_api_key};
use crate::domain::{
    base_imdb_id, build_torrent_tags, extract_btih_hex, is_torrent_stalled, match_best_qbit_file,
    normalize_valid_hash,
};
use crate::placeholder::{build_session_key, has_nonzero_range_header};
use crate::stream_builder::get_series_params;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

fn build_trigger_key(
    valid_initial_hash: Option<&str>,
    magnet_opt: Option<&str>,
    stremio_id: &str,
    info_hash: &str,
) -> String {
    if let Some(h) = valid_initial_hash {
        return h.to_string();
    }
    if let Some(magnet) = magnet_opt {
        return extract_btih_hex(magnet).unwrap_or_else(|| format!("{}:{}", stremio_id, magnet));
    }
    format!("{}:{}", stremio_id, info_hash)
}

fn spawn_trigger_background_task(
    task_state: AppState,
    task_api_key: String,
    task_stremio_id: String,
    initial_hash_clone: Option<String>,
    magnet_opt: Option<String>,
    trigger_key: String,
    already_existing: bool,
) {
    tokio::spawn(async move {
        let base_id = base_imdb_id(&task_stremio_id).to_string();

        let mut existing_hashes = HashSet::new();
        if initial_hash_clone.is_none() {
            for t in task_state.qbit.get_torrents_by_tag(&base_id).await {
                existing_hashes.insert(t.hash.to_lowercase());
            }
        }

        let tags = build_torrent_tags(&task_stremio_id);
        let mut target_hash = initial_hash_clone.clone();
        if let Some(ref magnet) = magnet_opt {
            let resolved = task_state
                .qbit
                .resolve_and_add_torrent(magnet, &tags, target_hash.as_deref())
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
            task_state
                .triggered_torrents
                .write()
                .await
                .insert(h.clone());
            task_state
                .resolved_trigger_hashes
                .write()
                .await
                .insert(trigger_key, h.clone());

            if initial_hash_clone.is_none() {
                let _ = task_state
                    .db
                    .record_media_watch(&task_api_key, &task_stremio_id, h);
            }
            task_state.qbit.add_torrent_tags(h, &tags).await;

            if !files.is_empty() {
                let (is_series, season, episode, expected_title) =
                    get_series_params(&task_state, &task_stremio_id).await;

                let best_idx =
                    match_best_qbit_file(&files, &expected_title, is_series, season, episode);

                if let Some(chosen_idx) = best_idx {
                    let chosen_id = files[chosen_idx].index.unwrap_or(chosen_idx);
                    if !already_existing {
                        let unwanted_ids: Vec<usize> = files
                            .iter()
                            .enumerate()
                            .filter(|&(i, f)| i != chosen_idx && f.progress == 0.0)
                            .map(|(i, f)| f.index.unwrap_or(i))
                            .collect();

                        if !unwanted_ids.is_empty() {
                            task_state
                                .qbit
                                .set_file_priorities(h, &unwanted_ids, 0)
                                .await;
                        }
                    }
                    task_state
                        .qbit
                        .set_file_priorities(h, &[chosen_id], 1)
                        .await;
                    task_state.qbit.resume_torrent(h).await;
                }
            } else {
                log::debug!("Metadata still unavailable after background poll for {}", h);
            }
        } else {
            log::warn!(
                "Could not resolve torrent hash for stremio_id {}",
                task_stremio_id
            );
        }
    });
}

pub async fn trigger_download(
    State(state): State<AppState>,
    Path((api_key, stremio_id, info_hash)): Path<(String, String, String)>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    let session_key = build_session_key(&api_key, "trigger", &stremio_id, &info_hash, None, "");
    if has_nonzero_range_header(&headers)
        && state.live_sessions.has_active_session(&session_key).await
    {
        return build_live_placeholder_redirect(
            &state.config.public_url,
            &api_key,
            "trigger",
            &stremio_id,
            &info_hash,
            PlaceholderState::FetchingMetadata,
            None,
            None,
        );
    }

    let valid_initial_hash = normalize_valid_hash(&info_hash);
    let magnet_opt = params.get("magnet").cloned();
    let trigger_key = build_trigger_key(
        valid_initial_hash.as_deref(),
        magnet_opt.as_deref(),
        &stremio_id,
        &info_hash,
    );
    let cached_hash = state
        .resolved_trigger_hashes
        .read()
        .await
        .get(&trigger_key)
        .cloned();
    let candidate_hash = valid_initial_hash
        .clone()
        .or_else(|| magnet_opt.as_deref().and_then(extract_btih_hex))
        .or(cached_hash);

    if let Some(ref hash) = candidate_hash {
        let qbit_lookup = tokio::time::timeout(Duration::from_millis(1500), async {
            tokio::join!(
                state.qbit.get_all_torrents(),
                state.qbit.get_torrent_files(hash),
            )
        })
        .await;

        let (all_torrents, files) = match qbit_lookup {
            Ok(res) => res,
            Err(_) => {
                log::warn!(
                    "Timed out querying qBittorrent status for {} in trigger_download",
                    hash
                );
                (Vec::new(), Vec::new())
            }
        };

        if let Some(existing_torrent) = all_torrents
            .iter()
            .find(|t| t.hash.eq_ignore_ascii_case(hash))
        {
            if !files.is_empty() {
                let (is_series, season, episode, expected_title) =
                    get_series_params(&state, &stremio_id).await;
                if let Some(chosen_idx) =
                    match_best_qbit_file(&files, &expected_title, is_series, season, episode)
                {
                    let chosen_file = &files[chosen_idx];
                    let chosen_id = chosen_file.index.unwrap_or(chosen_idx);
                    let was_unrequested = chosen_file.priority == 0;
                    return super::play::serve_existing_torrent_file(
                        &state,
                        &api_key,
                        &stremio_id,
                        hash,
                        &files,
                        Some(existing_torrent),
                        Some(chosen_id),
                        &chosen_file.name,
                        was_unrequested,
                    )
                    .await;
                }

                let _ = state.db.record_media_watch(&api_key, &stremio_id, hash);
                let placeholder =
                    if is_torrent_stalled(&existing_torrent.state, existing_torrent.dlspeed) {
                        PlaceholderState::Stalled
                    } else {
                        PlaceholderState::Buffering(0)
                    };
                return build_live_placeholder_redirect(
                    &state.config.public_url,
                    &api_key,
                    "trigger",
                    &stremio_id,
                    hash,
                    placeholder,
                    None,
                    None,
                );
            }

            let _ = state.db.record_media_watch(&api_key, &stremio_id, hash);
            let newly_inserted = state
                .triggered_torrents
                .write()
                .await
                .insert(trigger_key.clone());
            if newly_inserted {
                spawn_trigger_background_task(
                    state.clone(),
                    api_key.clone(),
                    stremio_id.clone(),
                    Some(hash.clone()),
                    None,
                    trigger_key,
                    true,
                );
            }
            let placeholder = if matches!(existing_torrent.state.trim(), "error" | "missingFiles") {
                PlaceholderState::Stalled
            } else {
                PlaceholderState::FetchingMetadata
            };
            return build_live_placeholder_redirect(
                &state.config.public_url,
                &api_key,
                "trigger",
                &stremio_id,
                hash,
                placeholder,
                None,
                None,
            );
        }
    }

    let _ = state.db.record_media_watch(
        &api_key,
        &stremio_id,
        valid_initial_hash.as_deref().unwrap_or("unknown"),
    );

    let already_existing = params.get("existing").is_some_and(|v| v == "1");
    let newly_inserted = state
        .triggered_torrents
        .write()
        .await
        .insert(trigger_key.clone());
    let placeholder = if already_existing || !newly_inserted {
        PlaceholderState::FetchingMetadata
    } else {
        PlaceholderState::DownloadStarted
    };

    spawn_trigger_background_task(
        state.clone(),
        api_key.clone(),
        stremio_id.clone(),
        valid_initial_hash,
        magnet_opt,
        trigger_key,
        already_existing,
    );

    build_live_placeholder_redirect(
        &state.config.public_url,
        &api_key,
        "trigger",
        &stremio_id,
        &info_hash,
        placeholder,
        None,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_key_prefers_valid_hash_then_magnet_btih_then_fallback() {
        assert_eq!(
            build_trigger_key(
                Some("0123456789abcdef0123456789abcdef01234567"),
                Some("magnet:?xt=urn:btih:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                "tt123",
                "0123456789abcdef0123456789abcdef01234567"
            ),
            "0123456789abcdef0123456789abcdef01234567"
        );

        assert_eq!(
            build_trigger_key(
                None,
                Some("magnet:?xt=urn:btih:BA0B39DE2C19CEADCFB1B441D8B6D668E57458EF&dn=Test"),
                "tt123",
                "unknown"
            ),
            "ba0b39de2c19ceadcfb1b441d8b6d668e57458ef"
        );

        assert_eq!(
            build_trigger_key(
                None,
                Some("https://jackett.local/dl/123"),
                "tt123:1:2",
                "unknown"
            ),
            "tt123:1:2:https://jackett.local/dl/123"
        );

        assert_eq!(
            build_trigger_key(None, None, "tt123", "unknown"),
            "tt123:unknown"
        );
    }
}
