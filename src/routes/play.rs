use super::{
    AppState, PlaceholderState, build_live_placeholder_redirect, require_valid_api_key,
    stream::resolve_safe_download_path,
};
use crate::qbittorrent::{QbitFile, build_torrent_tags};
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

fn parse_series_episode_id(stremio_id: &str) -> Option<(String, u32, u32)> {
    let parts: Vec<&str> = stremio_id.trim().split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let base = parts[0].trim();
    if base.is_empty() {
        return None;
    }
    let season: u32 = parts[1].trim().parse().ok()?;
    let episode: u32 = parts[2].trim().parse().ok()?;
    Some((base.to_lowercase(), season, episode))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextEpisodePrefetch {
    pub file_id: usize,
    pub file_path: String,
    pub season: u32,
    pub episode: u32,
}

pub fn select_next_episode_to_prefetch(
    stremio_id: &str,
    torrent_name: Option<&str>,
    files: &[QbitFile],
    current_file_vec_idx: usize,
) -> Option<NextEpisodePrefetch> {
    let (_, season, episode) = parse_series_episode_id(stremio_id)?;
    let file_infos: Vec<(&str, u64)> = files.iter().map(|f| (f.name.as_str(), f.size)).collect();
    let next_match = crate::matcher::find_next_episode_file(
        &file_infos,
        torrent_name,
        current_file_vec_idx,
        season,
        episode,
    )?;

    let next_file = &files[next_match.file_vec_idx];
    if next_file.priority != 0 {
        return None;
    }

    Some(NextEpisodePrefetch {
        file_id: next_file.index.unwrap_or(next_match.file_vec_idx),
        file_path: next_file.name.clone(),
        season: next_match.season,
        episode: next_match.episode,
    })
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

    let target_file = files.iter().enumerate().find(|(i, f)| {
        file_idx.is_some_and(|idx| f.index.unwrap_or(*i) == idx)
            || (!file_path.is_empty() && f.name == file_path)
    });

    let parent_torrent = tagged_torrents
        .iter()
        .find(|t| t.hash.eq_ignore_ascii_case(&info_hash));

    let current_file_id = file_idx.or_else(|| target_file.map(|(i, f)| f.index.unwrap_or(i)));

    let request_key = format!(
        "file:{}:{}",
        info_hash.to_lowercase(),
        current_file_id.unwrap_or(0)
    );
    let already_requested_in_session = !state.triggered_torrents.write().await.insert(request_key);

    let file_priority = target_file.map(|(_, f)| f.priority);
    let file_size = target_file.map_or(0, |(_, f)| f.size);
    let file_progress = target_file.map_or(0.0, |(_, f)| f.progress);
    let torrent_stalled = parent_torrent.is_some_and(|t| is_torrent_stalled(&t.state, t.dlspeed));

    let placeholder = decide_play_placeholder(
        exists_on_disk,
        file_priority,
        file_size,
        file_progress,
        was_unrequested,
        already_requested_in_session,
        torrent_stalled,
    );

    let next_episode_prefetch = if placeholder.is_none() {
        target_file.and_then(|(cur_vec_idx, _)| {
            select_next_episode_to_prefetch(
                &stremio_id,
                parent_torrent.map(|t| t.name.as_str()),
                &files,
                cur_vec_idx,
            )
        })
    } else {
        None
    };

    let current_file_priority = if placeholder.is_none()
        && file_progress < 1.0
        && (next_episode_prefetch.is_some() || file_priority.is_some_and(|p| p >= 6))
    {
        6
    } else {
        1
    };

    let qbit = state.qbit.clone();
    let triggered_torrents = state.triggered_torrents.clone();
    let hash = info_hash.clone();
    let tags = build_torrent_tags(&stremio_id);
    tokio::spawn(async move {
        let mut should_resume = false;
        if let Some(cur_id) = current_file_id {
            qbit.set_file_priorities(&hash, &[cur_id], current_file_priority)
                .await;
            should_resume = true;
        }
        if let Some(next_ep) = next_episode_prefetch {
            if qbit.set_file_priorities(&hash, &[next_ep.file_id], 1).await {
                log::info!(
                    "Prefetching next episode S{:02}E{:02} (file_id={}, path={:?}) for torrent {}",
                    next_ep.season,
                    next_ep.episode,
                    next_ep.file_id,
                    next_ep.file_path,
                    hash
                );
                let next_request_key = format!("file:{}:{}", hash.to_lowercase(), next_ep.file_id);
                triggered_torrents.write().await.insert(next_request_key);
                should_resume = true;
            } else {
                log::warn!(
                    "Failed to prefetch next episode S{:02}E{:02} (file_id={}) for torrent {}",
                    next_ep.season,
                    next_ep.episode,
                    next_ep.file_id,
                    hash
                );
            }
        }
        if should_resume {
            qbit.resume_torrent(&hash).await;
        }
        qbit.add_torrent_tags(&hash, &tags).await;
    });

    if let Some(placeholder) = placeholder {
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
    use crate::config::Config;
    use crate::db::{DbClient, MediaMetadata};
    use crate::placeholder_stream::LiveSessionStore;
    use crate::qbittorrent::QbitClient;
    use crate::routes::create_router;
    use axum::{
        Router,
        body::{Body, Bytes},
        extract::{Query as AxumQuery, State as AxumState},
        http::{Request, StatusCode, header},
        routing::{get, post},
    };
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use tokio::sync::{Notify, RwLock};
    use tower::ServiceExt;

    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn create_file(&self, rel: &str, bytes: &[u8]) {
            let full = self.0.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, bytes).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum MockQbitCall {
        FilePrio {
            hash: String,
            ids: String,
            priority: String,
        },
        Start {
            hashes: String,
        },
        AddTags {
            hashes: String,
            tags: String,
        },
        AddTorrent {
            tags: String,
        },
    }

    #[derive(Clone)]
    struct MockQbitState {
        torrents: Arc<Mutex<Vec<serde_json::Value>>>,
        files_by_hash: Arc<Mutex<HashMap<String, Vec<serde_json::Value>>>>,
        calls: Arc<Mutex<Vec<MockQbitCall>>>,
        notify: Arc<Notify>,
    }

    impl MockQbitState {
        fn new(
            torrents: Vec<serde_json::Value>,
            files_by_hash: HashMap<String, Vec<serde_json::Value>>,
        ) -> Self {
            Self {
                torrents: Arc::new(Mutex::new(torrents)),
                files_by_hash: Arc::new(Mutex::new(files_by_hash)),
                calls: Arc::new(Mutex::new(Vec::new())),
                notify: Arc::new(Notify::new()),
            }
        }

        fn record(&self, call: MockQbitCall) {
            self.calls.lock().unwrap().push(call);
            self.notify.notify_waiters();
        }

        fn recorded_calls(&self) -> Vec<MockQbitCall> {
            self.calls.lock().unwrap().clone()
        }

        async fn wait_for<F>(&self, mut predicate: F)
        where
            F: FnMut(&[MockQbitCall]) -> bool,
        {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            loop {
                let notified = self.notify.notified();
                {
                    let calls = self.calls.lock().unwrap();
                    if predicate(&calls) {
                        return;
                    }
                }
                if tokio::time::timeout_at(deadline, notified).await.is_err() {
                    let calls = self.calls.lock().unwrap();
                    assert!(
                        predicate(&calls),
                        "Timed out waiting for mock qBittorrent calls; observed: {:?}",
                        *calls
                    );
                    return;
                }
            }
        }
    }

    fn parse_multipart_fields(headers: &HeaderMap, body: &Bytes) -> HashMap<String, String> {
        let mut fields = HashMap::new();
        let Some(ct) = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
        else {
            return fields;
        };
        let Some(boundary) = ct
            .split(';')
            .find_map(|part| part.trim().strip_prefix("boundary="))
            .map(|b| b.trim_matches('"'))
        else {
            return fields;
        };

        let body_str = String::from_utf8_lossy(body);
        let delimiter = format!("--{}", boundary);
        for part in body_str.split(&delimiter) {
            let trimmed = part.trim_start_matches("\r\n");
            if trimmed.is_empty() || trimmed.starts_with("--") {
                continue;
            }
            let Some((raw_headers, raw_val)) = trimmed.split_once("\r\n\r\n") else {
                continue;
            };
            let Some(name_start) = raw_headers.find("name=\"") else {
                continue;
            };
            let rest = &raw_headers[name_start + 6..];
            let Some(name_end) = rest.find('"') else {
                continue;
            };
            let name = &rest[..name_end];
            let val = raw_val.strip_suffix("\r\n").unwrap_or(raw_val);
            fields.insert(name.to_string(), val.to_string());
        }
        fields
    }

    fn parse_form_urlencoded(body: &Bytes) -> HashMap<String, String> {
        let s = String::from_utf8_lossy(body);
        s.split('&')
            .filter_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                Some((
                    urlencoding::decode(k).ok()?.into_owned(),
                    urlencoding::decode(v).ok()?.into_owned(),
                ))
            })
            .collect()
    }

    async fn mock_qbit_login() -> impl IntoResponse {
        (
            StatusCode::OK,
            [(header::SET_COOKIE, "SID=mock-sid; Path=/")],
            "Ok.",
        )
    }

    async fn mock_qbit_torrents_info(
        AxumState(mock): AxumState<MockQbitState>,
    ) -> impl IntoResponse {
        axum::Json(mock.torrents.lock().unwrap().clone())
    }

    async fn mock_qbit_torrents_files(
        AxumState(mock): AxumState<MockQbitState>,
        AxumQuery(params): AxumQuery<HashMap<String, String>>,
    ) -> impl IntoResponse {
        let hash = params
            .get("hash")
            .map(|h| h.trim().to_lowercase())
            .unwrap_or_default();
        let files = mock
            .files_by_hash
            .lock()
            .unwrap()
            .get(&hash)
            .cloned()
            .unwrap_or_default();
        axum::Json(files)
    }

    async fn mock_qbit_file_prio(
        AxumState(mock): AxumState<MockQbitState>,
        headers: HeaderMap,
        body: Bytes,
    ) -> impl IntoResponse {
        let fields = parse_multipart_fields(&headers, &body);
        mock.record(MockQbitCall::FilePrio {
            hash: fields.get("hash").cloned().unwrap_or_default(),
            ids: fields.get("id").cloned().unwrap_or_default(),
            priority: fields.get("priority").cloned().unwrap_or_default(),
        });
        StatusCode::OK
    }

    async fn mock_qbit_start(AxumState(mock): AxumState<MockQbitState>, body: Bytes) -> StatusCode {
        let fields = parse_form_urlencoded(&body);
        mock.record(MockQbitCall::Start {
            hashes: fields.get("hashes").cloned().unwrap_or_default(),
        });
        StatusCode::OK
    }

    async fn mock_qbit_add_tags(
        AxumState(mock): AxumState<MockQbitState>,
        body: Bytes,
    ) -> StatusCode {
        let fields = parse_form_urlencoded(&body);
        mock.record(MockQbitCall::AddTags {
            hashes: fields.get("hashes").cloned().unwrap_or_default(),
            tags: fields.get("tags").cloned().unwrap_or_default(),
        });
        StatusCode::OK
    }

    async fn mock_qbit_add(
        AxumState(mock): AxumState<MockQbitState>,
        headers: HeaderMap,
        body: Bytes,
    ) -> StatusCode {
        let fields = parse_multipart_fields(&headers, &body);
        mock.record(MockQbitCall::AddTorrent {
            tags: fields.get("tags").cloned().unwrap_or_default(),
        });
        StatusCode::OK
    }

    async fn spawn_mock_qbit(mock: MockQbitState) -> String {
        let app = Router::new()
            .route("/api/v2/auth/login", post(mock_qbit_login))
            .route("/api/v2/torrents/info", get(mock_qbit_torrents_info))
            .route("/api/v2/torrents/files", get(mock_qbit_torrents_files))
            .route("/api/v2/torrents/filePrio", post(mock_qbit_file_prio))
            .route("/api/v2/torrents/start", post(mock_qbit_start))
            .route("/api/v2/torrents/addTags", post(mock_qbit_add_tags))
            .route("/api/v2/torrents/add", post(mock_qbit_add))
            .with_state(mock);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{}", addr)
    }

    fn build_test_app(
        qbit_url: String,
        download_path: &str,
    ) -> (Router, AppState, String, Arc<DbClient>) {
        let config = Config {
            port: 3000,
            public_url: "http://localhost:3000".to_string(),
            jackett_url: "http://localhost:9117".to_string(),
            jackett_api_key: "test".to_string(),
            jackett_search_type: "text".to_string(),
            qbittorrent_url: qbit_url,
            qbittorrent_username: "admin".to_string(),
            qbittorrent_password: "admin".to_string(),
            download_path: download_path.to_string(),
            retention_days: 0,
        };
        let db = Arc::new(DbClient::new(":memory:"));
        let api_key = db.create_user("tester").unwrap();
        let _ = db.upsert_media_metadata(&MediaMetadata {
            imdb_id: "tt0903747".to_string(),
            media_type: "series".to_string(),
            name: "Breaking Bad".to_string(),
            poster: None,
            background: None,
            logo: None,
            description: None,
            release_info: None,
            imdb_rating: None,
        });
        let qbit = Arc::new(QbitClient::new(config.clone()));
        let state = AppState {
            config: Arc::new(config),
            qbit,
            http_client: reqwest::Client::new(),
            db: db.clone(),
            admin_session_token: "test-admin-token".to_string(),
            triggered_torrents: Arc::new(RwLock::new(HashSet::new())),
            placeholder_bank: None,
            live_sessions: Arc::new(LiveSessionStore::default()),
        };
        let router = create_router(state.clone());
        (router, state, api_key, db)
    }

    fn season_pack_files_json(
        ep1_priority: u8,
        ep1_progress: f64,
        ep2_priority: u8,
    ) -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "index": 0,
                "name": "Breaking.Bad.S01.1080p/Breaking.Bad.S01E01.mkv",
                "size": GB,
                "progress": ep1_progress,
                "priority": ep1_priority,
            }),
            serde_json::json!({
                "index": 1,
                "name": "Breaking.Bad.S01.1080p/Breaking.Bad.S01E02.mkv",
                "size": GB,
                "progress": 0.0,
                "priority": ep2_priority,
            }),
            serde_json::json!({
                "index": 2,
                "name": "Breaking.Bad.S01.1080p/Breaking.Bad.S01E03.mkv",
                "size": GB,
                "progress": 0.0,
                "priority": 0,
            }),
        ]
    }

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

    #[test]
    fn select_next_episode_respects_priority_type_and_qbit_file_index() {
        let files = vec![
            QbitFile {
                index: Some(0),
                name: "Show.S01/readme.txt".to_string(),
                size: 100,
                progress: 0.0,
                priority: 0,
            },
            QbitFile {
                index: Some(10),
                name: "Show.S01/Show.S01E01.mkv".to_string(),
                size: GB,
                progress: 1.0,
                priority: 1,
            },
            QbitFile {
                index: Some(15),
                name: "Show.S01/Show.S01E02.mkv".to_string(),
                size: GB,
                progress: 0.0,
                priority: 0,
            },
        ];

        assert_eq!(
            select_next_episode_to_prefetch("tt0903747:1:1", Some("Show.S01"), &files, 1),
            Some(NextEpisodePrefetch {
                file_id: 15,
                file_path: "Show.S01/Show.S01E02.mkv".to_string(),
                season: 1,
                episode: 2,
            })
        );

        let mut already_requested = files.clone();
        already_requested[2].priority = 1;
        assert_eq!(
            select_next_episode_to_prefetch(
                "tt0903747:1:1",
                Some("Show.S01"),
                &already_requested,
                1
            ),
            None
        );

        assert_eq!(
            select_next_episode_to_prefetch("tt1375666", Some("Show.S01"), &files, 1),
            None
        );
    }

    #[tokio::test]
    async fn full_season_play_ready_downloading_prefetches_next_and_prioritizes_current() {
        let hash = "aaaa1111bbbb2222cccc3333dddd4444eeee5555";
        let ep1_path = "Breaking.Bad.S01.1080p/Breaking.Bad.S01E01.mkv";
        let dir = TempDir::new("play_prefetch_dl");
        dir.create_file(ep1_path, b"video-data");

        let mut files_map = HashMap::new();
        files_map.insert(hash.to_string(), season_pack_files_json(1, 0.05, 0));
        let mock = MockQbitState::new(
            vec![serde_json::json!({
                "hash": hash,
                "tags": "tt0903747,series",
                "name": "Breaking.Bad.S01.1080p",
                "size": 3 * GB,
                "progress": 0.02,
                "state": "downloading",
                "added_on": 1_700_000_000u64,
                "dlspeed": 5_000_000u64,
                "upspeed": 0u64,
                "eta": 60i64,
            })],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, state, api_key, db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        let uri = format!(
            "/{}/play-file/tt0903747:1:1/{}?fileIdx=0&filePath={}",
            api_key,
            hash,
            urlencoding::encode(ep1_path)
        );
        let res = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
        let loc = res
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(loc.contains("/stream-file/"), "unexpected location: {loc}");

        mock.wait_for(|calls| {
            calls
                .iter()
                .any(|c| matches!(c, MockQbitCall::AddTags { .. }))
        })
        .await;

        let calls = mock.recorded_calls();
        assert!(calls.contains(&MockQbitCall::FilePrio {
            hash: hash.to_string(),
            ids: "0".to_string(),
            priority: "6".to_string(),
        }));
        assert!(calls.contains(&MockQbitCall::FilePrio {
            hash: hash.to_string(),
            ids: "1".to_string(),
            priority: "1".to_string(),
        }));
        assert!(!calls.iter().any(|c| matches!(
            c,
            MockQbitCall::FilePrio { ids, .. } if ids == "2"
        )));
        assert!(calls.contains(&MockQbitCall::Start {
            hashes: hash.to_string(),
        }));
        assert!(calls.contains(&MockQbitCall::AddTags {
            hashes: hash.to_string(),
            tags: "tt0903747,series,tt0903747:1:1".to_string(),
        }));

        let series_entries = db.get_user_catalog_entries(&api_key, "series").unwrap();
        assert_eq!(series_entries.len(), 1);
        assert_eq!(series_entries[0].latest_stremio_id, "tt0903747:1:1");
        assert!(
            state
                .triggered_torrents
                .read()
                .await
                .contains(&format!("file:{}:1", hash))
        );
    }

    #[tokio::test]
    async fn full_season_play_ready_completed_keeps_normal_priority_and_prefetches_next() {
        let hash = "bbbb1111cccc2222dddd3333eeee4444ffff5555";
        let ep1_path = "Breaking.Bad.S01.1080p/Breaking.Bad.S01E01.mkv";
        let dir = TempDir::new("play_prefetch_done");
        dir.create_file(ep1_path, b"video-data");

        let mut files_map = HashMap::new();
        files_map.insert(hash.to_string(), season_pack_files_json(1, 1.0, 0));
        let mock = MockQbitState::new(
            vec![serde_json::json!({
                "hash": hash,
                "tags": "tt0903747,series",
                "name": "Breaking.Bad.S01.1080p",
                "size": 3 * GB,
                "progress": 0.33,
                "state": "uploading",
                "added_on": 1_700_000_000u64,
                "dlspeed": 0u64,
                "upspeed": 0u64,
                "eta": 0i64,
            })],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, _state, api_key, _db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        let uri = format!(
            "/{}/play-file/tt0903747:1:1/{}?fileIdx=0&filePath={}",
            api_key,
            hash,
            urlencoding::encode(ep1_path)
        );
        let res = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);

        mock.wait_for(|calls| {
            calls
                .iter()
                .any(|c| matches!(c, MockQbitCall::AddTags { .. }))
        })
        .await;

        let calls = mock.recorded_calls();
        assert!(calls.contains(&MockQbitCall::FilePrio {
            hash: hash.to_string(),
            ids: "0".to_string(),
            priority: "1".to_string(),
        }));
        assert!(calls.contains(&MockQbitCall::FilePrio {
            hash: hash.to_string(),
            ids: "1".to_string(),
            priority: "1".to_string(),
        }));
    }

    #[tokio::test]
    async fn full_season_play_unrequested_does_not_prefetch_next_episode() {
        let hash = "cccc1111dddd2222eeee3333ffff4444aaaa5555";
        let ep1_path = "Breaking.Bad.S01.1080p/Breaking.Bad.S01E01.mkv";
        let dir = TempDir::new("play_unrequested");

        let mut files_map = HashMap::new();
        files_map.insert(hash.to_string(), season_pack_files_json(0, 0.0, 0));
        let mock = MockQbitState::new(
            vec![serde_json::json!({
                "hash": hash,
                "tags": "tt0903747,series",
                "name": "Breaking.Bad.S01.1080p",
                "size": 3 * GB,
                "progress": 0.0,
                "state": "stoppedDL",
                "added_on": 1_700_000_000u64,
                "dlspeed": 0u64,
                "upspeed": 0u64,
                "eta": 0i64,
            })],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, _state, api_key, _db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        let uri = format!(
            "/{}/play-file/tt0903747:1:1/{}?fileIdx=0&filePath={}&unrequested=1",
            api_key,
            hash,
            urlencoding::encode(ep1_path)
        );
        let res = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
        let loc = res
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            loc.contains("/placeholder-live/play/") && loc.contains("initial=episode_requested"),
            "unexpected location: {loc}"
        );

        mock.wait_for(|calls| {
            calls
                .iter()
                .any(|c| matches!(c, MockQbitCall::AddTags { .. }))
        })
        .await;

        let calls = mock.recorded_calls();
        assert!(calls.contains(&MockQbitCall::FilePrio {
            hash: hash.to_string(),
            ids: "0".to_string(),
            priority: "1".to_string(),
        }));
        assert!(!calls.iter().any(|c| matches!(
            c,
            MockQbitCall::FilePrio { ids, .. } if ids != "0"
        )));
    }

    #[tokio::test]
    async fn full_season_play_buffering_does_not_prefetch_next_episode() {
        let hash = "dddd1111eeee2222ffff3333aaaa4444bbbb5555";
        let ep1_path = "Breaking.Bad.S01.1080p/Breaking.Bad.S01E01.mkv";
        let dir = TempDir::new("play_buffering");
        dir.create_file(ep1_path, b"partial-data");

        let mut files_map = HashMap::new();
        files_map.insert(hash.to_string(), season_pack_files_json(1, 0.01, 0));
        let mock = MockQbitState::new(
            vec![serde_json::json!({
                "hash": hash,
                "tags": "tt0903747,series",
                "name": "Breaking.Bad.S01.1080p",
                "size": 3 * GB,
                "progress": 0.005,
                "state": "downloading",
                "added_on": 1_700_000_000u64,
                "dlspeed": 1_000_000u64,
                "upspeed": 0u64,
                "eta": 300i64,
            })],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, _state, api_key, _db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        let uri = format!(
            "/{}/play-file/tt0903747:1:1/{}?fileIdx=0&filePath={}",
            api_key,
            hash,
            urlencoding::encode(ep1_path)
        );
        let res = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
        let loc = res
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            loc.contains("/placeholder-live/play/") && loc.contains("initial=buffering_1"),
            "unexpected location: {loc}"
        );

        mock.wait_for(|calls| {
            calls
                .iter()
                .any(|c| matches!(c, MockQbitCall::AddTags { .. }))
        })
        .await;

        let calls = mock.recorded_calls();
        assert!(calls.contains(&MockQbitCall::FilePrio {
            hash: hash.to_string(),
            ids: "0".to_string(),
            priority: "1".to_string(),
        }));
        assert!(!calls.iter().any(|c| matches!(
            c,
            MockQbitCall::FilePrio { ids, .. } if ids != "0"
        )));
    }

    #[tokio::test]
    async fn trigger_download_on_full_season_never_prefetches_next_episode() {
        let hash = "eeee1111ffff2222aaaa3333bbbb4444cccc5555";
        let dir = TempDir::new("trigger_no_prefetch");

        let mut files_map = HashMap::new();
        files_map.insert(hash.to_string(), season_pack_files_json(1, 0.0, 1));
        let mock = MockQbitState::new(vec![], files_map);
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, _state, api_key, _db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        let magnet = format!("magnet:?xt=urn:btih:{}", hash);
        let uri = format!(
            "/{}/trigger-download/tt0903747:1:1/{}?magnet={}",
            api_key,
            hash,
            urlencoding::encode(&magnet)
        );
        let res = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
        let loc = res
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            loc.contains("/placeholder-live/trigger/"),
            "unexpected location: {loc}"
        );

        mock.wait_for(|calls| {
            calls
                .iter()
                .any(|c| matches!(c, MockQbitCall::Start { .. }))
        })
        .await;

        let calls = mock.recorded_calls();
        assert!(calls.contains(&MockQbitCall::FilePrio {
            hash: hash.to_string(),
            ids: "1|2".to_string(),
            priority: "0".to_string(),
        }));
        assert!(calls.contains(&MockQbitCall::FilePrio {
            hash: hash.to_string(),
            ids: "0".to_string(),
            priority: "1".to_string(),
        }));
        assert!(!calls.iter().any(|c| matches!(
            c,
            MockQbitCall::FilePrio { ids, priority, .. } if ids == "1" && priority != "0"
        )));
    }

    #[tokio::test]
    async fn full_season_play_finale_and_already_requested_next_do_not_trigger_extra_files() {
        let hash = "ffff1111aaaa2222bbbb3333cccc4444dddd5555";
        let ep1_path = "Breaking.Bad.S01.1080p/Breaking.Bad.S01E01.mkv";
        let ep3_path = "Breaking.Bad.S01.1080p/Breaking.Bad.S01E03.mkv";
        let dir = TempDir::new("play_finale_and_no_cascade");
        dir.create_file(ep1_path, b"video-data");
        dir.create_file(ep3_path, b"video-data");

        // Case 1: Finale S01E03 -> never triggers any other file
        let mut files_map = HashMap::new();
        files_map.insert(hash.to_string(), season_pack_files_json(1, 1.0, 1));
        if let Some(files) = files_map.get_mut(hash) {
            files[2]["priority"] = serde_json::json!(1);
            files[2]["progress"] = serde_json::json!(1.0);
        }
        let mock_finale = MockQbitState::new(
            vec![serde_json::json!({
                "hash": hash,
                "tags": "tt0903747,series",
                "name": "Breaking.Bad.S01.1080p",
                "size": 3 * GB,
                "progress": 1.0,
                "state": "uploading",
                "added_on": 1_700_000_000u64,
                "dlspeed": 0u64,
                "upspeed": 0u64,
                "eta": 0i64,
            })],
            files_map,
        );
        let qbit_url_1 = spawn_mock_qbit(mock_finale.clone()).await;
        let (app_1, _state_1, api_key_1, _db_1) =
            build_test_app(qbit_url_1, dir.0.to_str().unwrap());

        let uri_finale = format!(
            "/{}/play-file/tt0903747:1:3/{}?fileIdx=2&filePath={}",
            api_key_1,
            hash,
            urlencoding::encode(ep3_path)
        );
        let res_finale = app_1
            .oneshot(
                Request::builder()
                    .uri(&uri_finale)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_finale.status(), StatusCode::TEMPORARY_REDIRECT);
        mock_finale
            .wait_for(|calls| {
                calls
                    .iter()
                    .any(|c| matches!(c, MockQbitCall::AddTags { .. }))
            })
            .await;

        let finale_calls = mock_finale.recorded_calls();
        assert!(!finale_calls.iter().any(|c| matches!(
            c,
            MockQbitCall::FilePrio { ids, .. } if ids != "2"
        )));

        // Case 2: S01E01 watched when S01E02 already has priority=1 -> never triggers S01E02 or S01E03
        let mut files_map_2 = HashMap::new();
        files_map_2.insert(hash.to_string(), season_pack_files_json(1, 1.0, 1));
        let mock_no_cascade = MockQbitState::new(
            vec![serde_json::json!({
                "hash": hash,
                "tags": "tt0903747,series",
                "name": "Breaking.Bad.S01.1080p",
                "size": 3 * GB,
                "progress": 0.5,
                "state": "downloading",
                "added_on": 1_700_000_000u64,
                "dlspeed": 1_000_000u64,
                "upspeed": 0u64,
                "eta": 60i64,
            })],
            files_map_2,
        );
        let qbit_url_2 = spawn_mock_qbit(mock_no_cascade.clone()).await;
        let (app_2, _state_2, api_key_2, _db_2) =
            build_test_app(qbit_url_2, dir.0.to_str().unwrap());

        let uri_ep1 = format!(
            "/{}/play-file/tt0903747:1:1/{}?fileIdx=0&filePath={}",
            api_key_2,
            hash,
            urlencoding::encode(ep1_path)
        );
        let res_ep1 = app_2
            .oneshot(
                Request::builder()
                    .uri(&uri_ep1)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_ep1.status(), StatusCode::TEMPORARY_REDIRECT);
        mock_no_cascade
            .wait_for(|calls| {
                calls
                    .iter()
                    .any(|c| matches!(c, MockQbitCall::AddTags { .. }))
            })
            .await;

        let no_cascade_calls = mock_no_cascade.recorded_calls();
        assert!(!no_cascade_calls.iter().any(|c| matches!(
            c,
            MockQbitCall::FilePrio { ids, .. } if ids != "0"
        )));
    }

    #[tokio::test]
    async fn single_episode_and_movie_torrents_never_prefetch_other_files() {
        let single_hash = "111122223333444455556666777788889999aaaa";
        let single_ep_path = "Breaking.Bad.S01E01.1080p/Breaking.Bad.S01E01.1080p.mkv";
        let movie_hash = "22223333444455556666777788889999aaaabbbb";
        let movie_path = "Inception.2010.1080p/Inception.2010.1080p.mkv";

        let dir = TempDir::new("single_and_movie_no_prefetch");
        dir.create_file(single_ep_path, b"video-data");
        dir.create_file(movie_path, b"video-data");

        let mut files_map = HashMap::new();
        files_map.insert(
            single_hash.to_string(),
            vec![
                serde_json::json!({
                    "index": 0,
                    "name": single_ep_path,
                    "size": GB,
                    "progress": 1.0,
                    "priority": 1,
                }),
                serde_json::json!({
                    "index": 1,
                    "name": "Breaking.Bad.S01E01.1080p/Sample/sample.mkv",
                    "size": 25 * MB,
                    "progress": 0.0,
                    "priority": 0,
                }),
            ],
        );
        files_map.insert(
            movie_hash.to_string(),
            vec![
                serde_json::json!({
                    "index": 0,
                    "name": movie_path,
                    "size": 2 * GB,
                    "progress": 1.0,
                    "priority": 1,
                }),
                serde_json::json!({
                    "index": 1,
                    "name": "Inception.2010.1080p/Sample/sample.mkv",
                    "size": 25 * MB,
                    "progress": 0.0,
                    "priority": 0,
                }),
            ],
        );

        let mock = MockQbitState::new(
            vec![
                serde_json::json!({
                    "hash": single_hash,
                    "tags": "tt0903747,series",
                    "name": "Breaking.Bad.S01E01.1080p",
                    "size": GB,
                    "progress": 1.0,
                    "state": "uploading",
                    "added_on": 1_700_000_000u64,
                    "dlspeed": 0u64,
                    "upspeed": 0u64,
                    "eta": 0i64,
                }),
                serde_json::json!({
                    "hash": movie_hash,
                    "tags": "tt1375666,movie",
                    "name": "Inception.2010.1080p",
                    "size": 2 * GB,
                    "progress": 1.0,
                    "state": "uploading",
                    "added_on": 1_700_000_000u64,
                    "dlspeed": 0u64,
                    "upspeed": 0u64,
                    "eta": 0i64,
                }),
            ],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, _state, api_key, _db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        let single_uri = format!(
            "/{}/play-file/tt0903747:1:1/{}?fileIdx=0&filePath={}",
            api_key,
            single_hash,
            urlencoding::encode(single_ep_path)
        );
        let res_single = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&single_uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_single.status(), StatusCode::TEMPORARY_REDIRECT);

        let movie_uri = format!(
            "/{}/play-file/tt1375666/{}?fileIdx=0&filePath={}",
            api_key,
            movie_hash,
            urlencoding::encode(movie_path)
        );
        let res_movie = app
            .oneshot(
                Request::builder()
                    .uri(&movie_uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_movie.status(), StatusCode::TEMPORARY_REDIRECT);

        mock.wait_for(|calls| {
            calls
                .iter()
                .filter(|c| matches!(c, MockQbitCall::AddTags { .. }))
                .count()
                >= 2
        })
        .await;

        let calls = mock.recorded_calls();
        assert!(!calls.iter().any(|c| matches!(
            c,
            MockQbitCall::FilePrio { ids, .. } if ids != "0"
        )));
    }
}
