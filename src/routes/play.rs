use super::{
    AppState, PlaceholderState, build_live_placeholder_redirect, require_valid_api_key,
    stream::resolve_safe_download_path,
};
use crate::qbittorrent::build_torrent_tags;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use std::collections::HashMap;
use std::time::Duration;

/// Minimum bytes downloaded for standard/large files (20 MB) to ensure container headers
/// (first/last pieces) and initial playback clusters are written to disk.
const MIN_READY_BYTES: u64 = 20 * 1024 * 1024;
/// Upper byte cap (64 MB): once at least 64 MB and >= 1% of a large file (e.g. 4K remux)
/// are downloaded, we do not require waiting for the full 3% percentage threshold.
const CAP_READY_BYTES: u64 = 64 * 1024 * 1024;
/// Default minimum progress fraction (3%) for medium files.
const MIN_READY_PROGRESS: f64 = 0.03;
/// Minimum progress fraction (20%) for very small files (<= 20 MB).
const SMALL_FILE_READY_PROGRESS: f64 = 0.20;

/// Returns whether a torrent file has enough sequential/header data downloaded to begin playback.
pub fn is_file_stream_ready(file_size: u64, progress: f64) -> bool {
    if progress >= 1.0 {
        return true;
    }
    if file_size == 0 {
        return progress >= MIN_READY_PROGRESS;
    }
    if file_size <= MIN_READY_BYTES {
        return progress >= SMALL_FILE_READY_PROGRESS;
    }
    let downloaded_bytes = ((file_size as f64) * progress) as u64;
    downloaded_bytes >= MIN_READY_BYTES
        && (progress >= MIN_READY_PROGRESS
            || (downloaded_bytes >= CAP_READY_BYTES && progress >= 0.01))
}

/// Returns whether a qBittorrent torrent state represents a stalled download with zero speed.
pub fn is_torrent_stalled(torrent_state: &str, dlspeed: u64) -> bool {
    let s = torrent_state.trim();
    dlspeed == 0 && matches!(s, "stalledDL" | "metaDL" | "error" | "missingFiles")
}

/// Determines which placeholder state to serve immediately, or `None` if ready to stream directly.
pub fn decide_play_placeholder(
    exists_on_disk: bool,
    file_priority: Option<u8>,
    file_size: u64,
    file_progress: f64,
    was_unrequested_param: bool,
    already_requested_in_session: bool,
    torrent_stalled: bool,
) -> Option<PlaceholderState> {
    let stream_ready = is_file_stream_ready(file_size, file_progress);
    if exists_on_disk && stream_ready {
        return None;
    }

    let is_unrequested =
        file_priority == Some(0) || (file_priority.is_none() && was_unrequested_param);
    if is_unrequested && !already_requested_in_session {
        return Some(PlaceholderState::EpisodeRequested);
    }

    if !exists_on_disk && (stream_ready || file_progress >= 0.10) {
        return Some(PlaceholderState::FilesMissing);
    }

    if torrent_stalled {
        return Some(PlaceholderState::Stalled);
    }

    Some(PlaceholderState::buffering_for_progress(file_progress))
}

pub async fn play_file(
    State(state): State<AppState>,
    Path((api_key, stremio_id, info_hash)): Path<(String, String, String)>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    let file_idx = params.get("fileIdx").and_then(|s| s.parse::<usize>().ok());
    let file_path = params.get("filePath").cloned().unwrap_or_default();

    let session_key = crate::placeholder_stream::build_session_key(
        &api_key,
        "play",
        &stremio_id,
        &info_hash,
        file_idx,
        &file_path,
    );
    if crate::placeholder_stream::has_nonzero_range_header(&headers)
        && state.live_sessions.has_active_session(&session_key).await
    {
        return build_live_placeholder_redirect(
            &state.config.public_url,
            &api_key,
            "play",
            &stremio_id,
            &info_hash,
            PlaceholderState::Buffering(0),
            file_idx,
            Some(&file_path),
        );
    }

    let _ = state
        .db
        .record_media_watch(&api_key, &stremio_id, &info_hash);

    let was_unrequested = params.get("unrequested").is_some_and(|v| v == "1");

    let exists_on_disk = resolve_safe_download_path(&state.config.download_path, &file_path)
        .is_some_and(|p| {
            std::fs::metadata(p)
                .map(|m| m.is_file() && m.len() > 0)
                .unwrap_or(false)
        });

    let base_id = stremio_id.split(':').next().unwrap_or(&stremio_id);
    let qbit_lookup = tokio::time::timeout(Duration::from_millis(1500), async {
        tokio::join!(
            state.qbit.get_torrent_files(&info_hash),
            state.qbit.get_torrents_by_tag(base_id),
        )
    })
    .await;

    let (files, tagged_torrents) = match qbit_lookup {
        Ok(res) => res,
        Err(_) => {
            log::warn!(
                "Timed out querying qBittorrent status for {} in play_file",
                info_hash
            );
            (Vec::new(), Vec::new())
        }
    };

    let target_file = files
        .iter()
        .enumerate()
        .find(|(i, f)| {
            file_idx.is_some_and(|idx| f.index.unwrap_or(*i) == idx)
                || (!file_path.is_empty() && f.name == file_path)
        })
        .map(|(_, f)| f);

    let parent_torrent = tagged_torrents
        .iter()
        .find(|t| t.hash.eq_ignore_ascii_case(&info_hash));

    let request_key = format!(
        "file:{}:{}",
        info_hash.to_lowercase(),
        file_idx.unwrap_or(0)
    );
    let already_requested_in_session = !state.triggered_torrents.write().await.insert(request_key);

    let qbit = state.qbit.clone();
    let hash = info_hash.clone();
    let tags = build_torrent_tags(&stremio_id);
    tokio::spawn(async move {
        if let Some(idx) = file_idx {
            qbit.set_file_priorities(&hash, &[idx], 1).await;
            qbit.resume_torrent(&hash).await;
        }
        qbit.add_torrent_tags(&hash, &tags).await;
    });

    let file_priority = target_file.map(|f| f.priority);
    let file_size = target_file.map_or(0, |f| f.size);
    let file_progress = target_file.map_or(0.0, |f| f.progress);
    let torrent_stalled = parent_torrent.is_some_and(|t| is_torrent_stalled(&t.state, t.dlspeed));

    if let Some(placeholder) = decide_play_placeholder(
        exists_on_disk,
        file_priority,
        file_size,
        file_progress,
        was_unrequested,
        already_requested_in_session,
        torrent_stalled,
    ) {
        return build_live_placeholder_redirect(
            &state.config.public_url,
            &api_key,
            "play",
            &stremio_id,
            &info_hash,
            placeholder,
            file_idx,
            Some(&file_path),
        );
    }

    let redirect_url = format!(
        "{}/{}/stream-file/{}?filePath={}",
        state.config.public_url,
        api_key,
        info_hash,
        urlencoding::encode(&file_path)
    );
    Redirect::temporary(&redirect_url).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn stream_readiness_thresholds() {
        assert!(!is_file_stream_ready(GB, 0.0));
        assert!(is_file_stream_ready(GB, 1.0));

        // Small file (10 MB): requires >= 20%
        assert!(!is_file_stream_ready(10 * MB, 0.05));
        assert!(is_file_stream_ready(10 * MB, 0.25));

        // Medium file (1 GB): requires >= 20 MB AND >= 3%
        assert!(!is_file_stream_ready(GB, 0.01));
        assert!(!is_file_stream_ready(GB, 0.025));
        assert!(is_file_stream_ready(GB, 0.03));

        // Large file (20 GB): 0.1% = ~20 MB (< 1% so not ready); 1.2% = ~245 MB (>= 64 MB cap & >= 1%)
        assert!(!is_file_stream_ready(20 * GB, 0.001));
        assert!(is_file_stream_ready(20 * GB, 0.012));
    }

    #[test]
    fn stalled_torrent_detection() {
        assert!(is_torrent_stalled("stalledDL", 0));
        assert!(is_torrent_stalled("metaDL", 0));
        assert!(is_torrent_stalled("error", 0));
        assert!(is_torrent_stalled("missingFiles", 0));
        assert!(!is_torrent_stalled("downloading", 0));
        assert!(!is_torrent_stalled("stalledDL", 1024));
        assert!(!is_torrent_stalled("uploading", 0));
    }

    #[test]
    fn placeholder_decisions_cover_all_states() {
        // First click on unrequested episode -> EpisodeRequested
        assert_eq!(
            decide_play_placeholder(false, Some(0), GB, 0.0, true, false, false),
            Some(PlaceholderState::EpisodeRequested)
        );

        // Second click on same episode in session -> Buffering(0) (or Stalled if stalled)
        assert_eq!(
            decide_play_placeholder(false, Some(0), GB, 0.0, true, true, false),
            Some(PlaceholderState::Buffering(0))
        );
        assert_eq!(
            decide_play_placeholder(false, Some(0), GB, 0.0, true, true, true),
            Some(PlaceholderState::Stalled)
        );

        // Actively downloading file below threshold -> Buffering(pct)
        assert_eq!(
            decide_play_placeholder(true, Some(1), GB, 0.01, false, false, false),
            Some(PlaceholderState::Buffering(1))
        );

        // Stalled torrent below threshold -> Stalled
        assert_eq!(
            decide_play_placeholder(true, Some(1), GB, 0.01, false, false, true),
            Some(PlaceholderState::Stalled)
        );

        // Ready file on disk -> None (redirect to direct stream)
        assert_eq!(
            decide_play_placeholder(true, Some(1), GB, 0.05, false, false, false),
            None
        );

        // Ready in qBit (80% or 5% of 1 GB) but file missing on disk -> FilesMissing
        assert_eq!(
            decide_play_placeholder(false, Some(1), GB, 0.80, false, false, false),
            Some(PlaceholderState::FilesMissing)
        );
        assert_eq!(
            decide_play_placeholder(false, Some(1), GB, 0.05, false, false, false),
            Some(PlaceholderState::FilesMissing)
        );
        // Below ready threshold (1% of 1 GB) and missing on disk -> Buffering(1)
        assert_eq!(
            decide_play_placeholder(false, Some(1), GB, 0.01, false, false, false),
            Some(PlaceholderState::Buffering(1))
        );
        // On disk at 12% of 100 MB (below 20 MB ready threshold) -> Buffering(9)
        assert_eq!(
            decide_play_placeholder(true, Some(1), 100 * MB, 0.12, false, false, false),
            Some(PlaceholderState::Buffering(9))
        );
    }

    #[test]
    fn all_placeholder_segments_and_config_exist_on_disk() {
        assert!(
            std::path::Path::new("placeholders/placeholders.yaml").exists(),
            "Missing placeholders/placeholders.yaml"
        );
        assert!(
            std::path::Path::new("placeholders/init.mp4").exists(),
            "Missing placeholders/init.mp4"
        );
        for state in PlaceholderState::all() {
            for idx in 0..7 {
                let seg_path = format!("placeholders/{}/seg_{}.m4s", state.id(), idx);
                assert!(
                    std::path::Path::new(&seg_path).exists(),
                    "Missing generated placeholder segment: {}",
                    seg_path
                );
            }
        }
    }
}
