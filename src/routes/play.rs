use super::{AppState, PlaceholderState, build_live_placeholder_redirect, require_valid_api_key};
use crate::domain::{
    base_imdb_id, build_torrent_tags, decide_play_placeholder, file_exists_nonempty,
    find_target_file, is_torrent_stalled, select_next_episode_to_prefetch,
};
use crate::placeholder::{build_session_key, has_nonzero_range_header};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use std::collections::HashMap;
use std::time::Duration;

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

    let session_key = build_session_key(
        &api_key,
        "play",
        &stremio_id,
        &info_hash,
        file_idx,
        &file_path,
    );
    if has_nonzero_range_header(&headers)
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

    let exists_on_disk = file_exists_nonempty(&state.config.download_path, &file_path);

    let base_id = base_imdb_id(&stremio_id);
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

    let target_file = find_target_file(&files, file_idx, &file_path);

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
    use crate::placeholder::LiveSessionStore;
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
                if predicate(&self.calls.lock().unwrap()) {
                    return;
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
        let Some(boundary) = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|ct| {
                ct.split(';')
                    .find_map(|p| p.trim().strip_prefix("boundary="))
                    .map(|b| b.trim_matches('"'))
            })
        else {
            return HashMap::new();
        };

        String::from_utf8_lossy(body)
            .split(&format!("--{}", boundary))
            .filter_map(|part| {
                let trimmed = part.trim_start_matches("\r\n");
                if trimmed.is_empty() || trimmed.starts_with("--") {
                    return None;
                }
                let (raw_headers, raw_val) = trimmed.split_once("\r\n\r\n")?;
                let rest = &raw_headers[raw_headers.find("name=\"")? + 6..];
                let name = &rest[..rest.find('"')?];
                let val = raw_val.strip_suffix("\r\n").unwrap_or(raw_val);
                Some((name.to_string(), val.to_string()))
            })
            .collect()
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

    fn mock_file_json(
        index: usize,
        name: &str,
        size: u64,
        progress: f64,
        priority: u8,
    ) -> serde_json::Value {
        serde_json::json!({
            "index": index,
            "name": name,
            "size": size,
            "progress": progress,
            "priority": priority,
        })
    }

    fn mock_torrent_json(
        hash: &str,
        tags: &str,
        name: &str,
        size: u64,
        progress: f64,
        state: &str,
        dlspeed: u64,
    ) -> serde_json::Value {
        serde_json::json!({
            "hash": hash,
            "tags": tags,
            "name": name,
            "size": size,
            "progress": progress,
            "state": state,
            "added_on": 1_700_000_000u64,
            "dlspeed": dlspeed,
            "upspeed": 0u64,
            "eta": 60i64,
        })
    }

    fn season_pack_files_json(
        ep1_priority: u8,
        ep1_progress: f64,
        ep2_priority: u8,
    ) -> Vec<serde_json::Value> {
        vec![
            mock_file_json(
                0,
                "Breaking.Bad.S01.1080p/Breaking.Bad.S01E01.mkv",
                GB,
                ep1_progress,
                ep1_priority,
            ),
            mock_file_json(
                1,
                "Breaking.Bad.S01.1080p/Breaking.Bad.S01E02.mkv",
                GB,
                0.0,
                ep2_priority,
            ),
            mock_file_json(
                2,
                "Breaking.Bad.S01.1080p/Breaking.Bad.S01E03.mkv",
                GB,
                0.0,
                0,
            ),
        ]
    }

    fn play_uri(
        api_key: &str,
        stremio_id: &str,
        hash: &str,
        file_idx: usize,
        file_path: &str,
        extra: &str,
    ) -> String {
        format!(
            "/{}/play-file/{}/{}?fileIdx={}&filePath={}{}",
            api_key,
            stremio_id,
            hash,
            file_idx,
            urlencoding::encode(file_path),
            extra
        )
    }

    async fn get_redirect_location(app: Router, uri: &str) -> String {
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
        res.headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    }

    async fn wait_for_add_tags(mock: &MockQbitState) {
        mock.wait_for(|calls| {
            calls
                .iter()
                .any(|c| matches!(c, MockQbitCall::AddTags { .. }))
        })
        .await;
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

    #[tokio::test]
    async fn full_season_play_ready_downloading_prefetches_next_and_prioritizes_current() {
        let hash = "aaaa1111bbbb2222cccc3333dddd4444eeee5555";
        let ep1_path = "Breaking.Bad.S01.1080p/Breaking.Bad.S01E01.mkv";
        let dir = TempDir::new("play_prefetch_dl");
        dir.create_file(ep1_path, b"video-data");

        let mut files_map = HashMap::new();
        files_map.insert(hash.to_string(), season_pack_files_json(1, 0.05, 0));
        let mock = MockQbitState::new(
            vec![mock_torrent_json(
                hash,
                "tt0903747,series",
                "Breaking.Bad.S01.1080p",
                3 * GB,
                0.02,
                "downloading",
                5_000_000,
            )],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, state, api_key, db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        let loc = get_redirect_location(
            app,
            &play_uri(&api_key, "tt0903747:1:1", hash, 0, ep1_path, ""),
        )
        .await;
        assert!(loc.contains("/stream-file/"), "unexpected location: {loc}");

        wait_for_add_tags(&mock).await;

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
            vec![mock_torrent_json(
                hash,
                "tt0903747,series",
                "Breaking.Bad.S01.1080p",
                3 * GB,
                0.33,
                "uploading",
                0,
            )],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, _state, api_key, _db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        get_redirect_location(
            app,
            &play_uri(&api_key, "tt0903747:1:1", hash, 0, ep1_path, ""),
        )
        .await;
        wait_for_add_tags(&mock).await;

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
            vec![mock_torrent_json(
                hash,
                "tt0903747,series",
                "Breaking.Bad.S01.1080p",
                3 * GB,
                0.0,
                "stoppedDL",
                0,
            )],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, _state, api_key, _db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        let loc = get_redirect_location(
            app,
            &play_uri(
                &api_key,
                "tt0903747:1:1",
                hash,
                0,
                ep1_path,
                "&unrequested=1",
            ),
        )
        .await;
        assert!(
            loc.contains("/placeholder-live/play/") && loc.contains("initial=episode_requested"),
            "unexpected location: {loc}"
        );

        wait_for_add_tags(&mock).await;

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
            vec![mock_torrent_json(
                hash,
                "tt0903747,series",
                "Breaking.Bad.S01.1080p",
                3 * GB,
                0.005,
                "downloading",
                1_000_000,
            )],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, _state, api_key, _db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        let loc = get_redirect_location(
            app,
            &play_uri(&api_key, "tt0903747:1:1", hash, 0, ep1_path, ""),
        )
        .await;
        assert!(
            loc.contains("/placeholder-live/play/") && loc.contains("initial=buffering_1"),
            "unexpected location: {loc}"
        );

        wait_for_add_tags(&mock).await;

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
        let loc = get_redirect_location(app, &uri).await;
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
            vec![mock_torrent_json(
                hash,
                "tt0903747,series",
                "Breaking.Bad.S01.1080p",
                3 * GB,
                1.0,
                "uploading",
                0,
            )],
            files_map,
        );
        let qbit_url_1 = spawn_mock_qbit(mock_finale.clone()).await;
        let (app_1, _state_1, api_key_1, _db_1) =
            build_test_app(qbit_url_1, dir.0.to_str().unwrap());

        get_redirect_location(
            app_1,
            &play_uri(&api_key_1, "tt0903747:1:3", hash, 2, ep3_path, ""),
        )
        .await;
        wait_for_add_tags(&mock_finale).await;

        assert!(!mock_finale.recorded_calls().iter().any(|c| matches!(
            c,
            MockQbitCall::FilePrio { ids, .. } if ids != "2"
        )));

        // Case 2: S01E01 watched when S01E02 already has priority=1 -> never triggers S01E02 or S01E03
        let mut files_map_2 = HashMap::new();
        files_map_2.insert(hash.to_string(), season_pack_files_json(1, 1.0, 1));
        let mock_no_cascade = MockQbitState::new(
            vec![mock_torrent_json(
                hash,
                "tt0903747,series",
                "Breaking.Bad.S01.1080p",
                3 * GB,
                0.5,
                "downloading",
                1_000_000,
            )],
            files_map_2,
        );
        let qbit_url_2 = spawn_mock_qbit(mock_no_cascade.clone()).await;
        let (app_2, _state_2, api_key_2, _db_2) =
            build_test_app(qbit_url_2, dir.0.to_str().unwrap());

        get_redirect_location(
            app_2,
            &play_uri(&api_key_2, "tt0903747:1:1", hash, 0, ep1_path, ""),
        )
        .await;
        wait_for_add_tags(&mock_no_cascade).await;

        assert!(!mock_no_cascade.recorded_calls().iter().any(|c| matches!(
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
                mock_file_json(0, single_ep_path, GB, 1.0, 1),
                mock_file_json(
                    1,
                    "Breaking.Bad.S01E01.1080p/Sample/sample.mkv",
                    25 * MB,
                    0.0,
                    0,
                ),
            ],
        );
        files_map.insert(
            movie_hash.to_string(),
            vec![
                mock_file_json(0, movie_path, 2 * GB, 1.0, 1),
                mock_file_json(1, "Inception.2010.1080p/Sample/sample.mkv", 25 * MB, 0.0, 0),
            ],
        );

        let mock = MockQbitState::new(
            vec![
                mock_torrent_json(
                    single_hash,
                    "tt0903747,series",
                    "Breaking.Bad.S01E01.1080p",
                    GB,
                    1.0,
                    "uploading",
                    0,
                ),
                mock_torrent_json(
                    movie_hash,
                    "tt1375666,movie",
                    "Inception.2010.1080p",
                    2 * GB,
                    1.0,
                    "uploading",
                    0,
                ),
            ],
            files_map,
        );
        let qbit_url = spawn_mock_qbit(mock.clone()).await;
        let (app, _state, api_key, _db) = build_test_app(qbit_url, dir.0.to_str().unwrap());

        get_redirect_location(
            app.clone(),
            &play_uri(
                &api_key,
                "tt0903747:1:1",
                single_hash,
                0,
                single_ep_path,
                "",
            ),
        )
        .await;
        get_redirect_location(
            app,
            &play_uri(&api_key, "tt1375666", movie_hash, 0, movie_path, ""),
        )
        .await;

        mock.wait_for(|calls| {
            calls
                .iter()
                .filter(|c| matches!(c, MockQbitCall::AddTags { .. }))
                .count()
                >= 2
        })
        .await;

        assert!(!mock.recorded_calls().iter().any(|c| matches!(
            c,
            MockQbitCall::FilePrio { ids, .. } if ids != "0"
        )));
    }
}
