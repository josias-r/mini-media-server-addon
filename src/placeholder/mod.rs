pub mod fmp4;

pub use fmp4::PlaceholderBank;

use crate::config::AppState;
use crate::domain::{
    base_imdb_id, decide_live_file_state, file_exists_nonempty, find_target_file,
    is_torrent_stalled, is_valid_hex_hash, match_best_qbit_file,
};
use crate::stream_builder::get_series_params;
use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_util::stream;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, watch};
use tokio::time::sleep_until;
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub const SEGMENTS_PER_LOOP: usize = 7;
pub const VIDEO_TRACK_ID: u32 = 1;
pub const VIDEO_TIMESCALE: u32 = 15_360;
pub const VIDEO_SEG_DURATION: u64 = 32_768; // 64 frames @ 30fps in 15360 timescale
pub const AUDIO_TRACK_ID: u32 = 2;
pub const AUDIO_SEG_DURATION: u64 = 102_400; // 100 AAC frames * 1024 samples @ 48kHz
const SEG_DURATION_NS: u64 = 2_133_333_333; // 64 / 30 seconds

pub const DEFAULT_DURATION_HOURS: u64 = 5;
const BURST_FRAGMENTS: u64 = 3;
const INITIAL_HOLD_FRAGMENTS: u64 = 2;
const STREAM_CHUNK_SIZE: usize = 16 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
const POLLER_IDLE_GRACE: Duration = Duration::from_secs(3);
const ACTIVE_SESSION_GRACE: Duration = Duration::from_secs(5);
const SESSION_TTL: Duration = Duration::from_secs(3 * 3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceholderState {
    DownloadStarted,
    FetchingMetadata,
    EpisodeRequested,
    Stalled,
    Buffering(u8),
    Ready,
    FilesMissing,
}

const ALL_STATES: [(PlaceholderState, &str); 16] = [
    (PlaceholderState::DownloadStarted, "download_started"),
    (PlaceholderState::FetchingMetadata, "fetching_metadata"),
    (PlaceholderState::EpisodeRequested, "episode_requested"),
    (PlaceholderState::Stalled, "stalled"),
    (PlaceholderState::Buffering(0), "buffering_0"),
    (PlaceholderState::Buffering(1), "buffering_1"),
    (PlaceholderState::Buffering(2), "buffering_2"),
    (PlaceholderState::Buffering(3), "buffering_3"),
    (PlaceholderState::Buffering(4), "buffering_4"),
    (PlaceholderState::Buffering(5), "buffering_5"),
    (PlaceholderState::Buffering(6), "buffering_6"),
    (PlaceholderState::Buffering(7), "buffering_7"),
    (PlaceholderState::Buffering(8), "buffering_8"),
    (PlaceholderState::Buffering(9), "buffering_9"),
    (PlaceholderState::Ready, "ready"),
    (PlaceholderState::FilesMissing, "files_missing"),
];

impl PlaceholderState {
    /// Returns the asset directory ID matching `placeholders/placeholders.yaml`.
    pub fn id(self) -> &'static str {
        ALL_STATES[self.to_u8() as usize].1
    }

    /// Parses a state ID from query parameters or directory names.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "buffering" => Some(Self::Buffering(0)),
            "files-missing" => Some(Self::FilesMissing),
            _ => ALL_STATES
                .iter()
                .find(|&&(_, s)| s == id)
                .map(|&(st, _)| st),
        }
    }

    /// Encodes the state into a compact `u8` (`0..=15`) for lock-free atomic sharing.
    pub fn to_u8(self) -> u8 {
        match self {
            Self::DownloadStarted => 0,
            Self::FetchingMetadata => 1,
            Self::EpisodeRequested => 2,
            Self::Stalled => 3,
            Self::Buffering(p) => 4 + p.min(9),
            Self::Ready => 14,
            Self::FilesMissing => 15,
        }
    }

    /// Decodes a `u8` back into `PlaceholderState`.
    pub fn from_u8(v: u8) -> Self {
        ALL_STATES
            .get(v as usize)
            .map_or(Self::Buffering(0), |&(s, _)| s)
    }

    /// Returns all 16 placeholder states required on disk.
    pub fn all() -> [Self; 16] {
        ALL_STATES.map(|(s, _)| s)
    }

    /// Maps a `[0.0, 1.0]` download fraction into `Buffering(0..=9)`.
    pub fn buffering_for_progress(progress: f64) -> Self {
        if !progress.is_finite() {
            return Self::Buffering(0);
        }
        let pct = (progress.clamp(0.0, 0.9999) * 100.0).floor() as u8;
        Self::Buffering(pct.min(9))
    }

    /// Whether this initial state should stay visible for the first few seconds before switching.
    pub fn is_transient_initial(self) -> bool {
        matches!(self, Self::DownloadStarted | Self::EpisodeRequested)
    }
}

/// Computes a deterministic session key shared across redirects and HTTP Range requests.
pub fn build_session_key(
    api_key: &str,
    mode: &str,
    stremio_id: &str,
    info_hash: &str,
    file_idx: Option<usize>,
    file_path: &str,
) -> String {
    let norm_hash = info_hash.trim().to_lowercase();
    match mode {
        "trigger" => format!("trigger:{}:{}:{}", api_key, stremio_id, norm_hash),
        "stream" => format!("stream:{}:{}:{}", api_key, norm_hash, file_path),
        _ => format!(
            "play:{}:{}:{}:{}:{}",
            api_key,
            stremio_id,
            norm_hash,
            file_idx.map(|i| i.to_string()).unwrap_or_default(),
            file_path
        ),
    }
}

/// Returns `true` when the request carries a `Range: bytes=...` header starting past byte 0.
pub fn has_nonzero_range_header(headers: &HeaderMap) -> bool {
    let Some(spec) = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| r.trim().strip_prefix("bytes="))
        .filter(|s| !s.contains(','))
    else {
        return false;
    };
    let Some((start_str, _)) = spec.split_once('-') else {
        return false;
    };
    let start_str = start_str.trim();
    start_str.is_empty() || matches!(start_str.parse::<u64>(), Ok(start) if start > 0)
}

#[derive(Debug, PartialEq, Eq)]
pub enum RangeParseResult {
    None,
    Satisfiable(u64, u64),
    Unsatisfiable,
}

/// Parses a single HTTP `Range: bytes=...` header against `total_len`.
pub fn parse_http_range(headers: &HeaderMap, total_len: u64) -> RangeParseResult {
    let Some(spec) = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| r.trim().strip_prefix("bytes="))
        .filter(|s| !s.contains(','))
    else {
        return RangeParseResult::None;
    };
    let Some((start_str, end_str)) = spec.split_once('-') else {
        return RangeParseResult::None;
    };
    if total_len == 0 {
        return RangeParseResult::Unsatisfiable;
    }

    let (start_str, end_str) = (start_str.trim(), end_str.trim());
    if start_str.is_empty() {
        let Ok(suffix) = end_str.parse::<u64>() else {
            return RangeParseResult::None;
        };
        return if suffix == 0 {
            RangeParseResult::Unsatisfiable
        } else {
            RangeParseResult::Satisfiable(total_len.saturating_sub(suffix), total_len - 1)
        };
    }

    let Ok(start) = start_str.parse::<u64>() else {
        return RangeParseResult::None;
    };
    if start >= total_len {
        return RangeParseResult::Unsatisfiable;
    }
    if end_str.is_empty() {
        return RangeParseResult::Satisfiable(start, total_len - 1);
    }

    let Ok(end) = end_str.parse::<u64>() else {
        return RangeParseResult::None;
    };
    if end < start {
        RangeParseResult::Unsatisfiable
    } else {
        RangeParseResult::Satisfiable(start, end.min(total_len - 1))
    }
}

pub struct LiveSession {
    latest_state: Arc<AtomicU8>,
    initial_state: Arc<AtomicU8>,
    active_streams: Arc<AtomicUsize>,
    stream_count_tx: watch::Sender<usize>,
    frag_states: Arc<std::sync::RwLock<HashMap<u64, PlaceholderState>>>,
    poller_running: Arc<AtomicBool>,
    last_touched: Arc<std::sync::Mutex<Instant>>,
}

impl LiveSession {
    fn new(initial: PlaceholderState) -> Self {
        let (stream_count_tx, _) = watch::channel(0usize);
        Self {
            latest_state: Arc::new(AtomicU8::new(initial.to_u8())),
            initial_state: Arc::new(AtomicU8::new(initial.to_u8())),
            active_streams: Arc::new(AtomicUsize::new(0)),
            stream_count_tx,
            frag_states: Arc::new(std::sync::RwLock::new(HashMap::new())),
            poller_running: Arc::new(AtomicBool::new(false)),
            last_touched: Arc::new(std::sync::Mutex::new(Instant::now())),
        }
    }

    pub fn latest(&self) -> PlaceholderState {
        PlaceholderState::from_u8(self.latest_state.load(Ordering::Acquire))
    }

    pub fn set_latest(&self, state: PlaceholderState) {
        self.latest_state.store(state.to_u8(), Ordering::Release);
    }

    fn touch(&self) {
        if let Ok(mut t) = self.last_touched.lock() {
            *t = Instant::now();
        }
    }

    fn elapsed_since_touch(&self, now: Instant) -> Duration {
        self.last_touched
            .lock()
            .map(|t| now.saturating_duration_since(*t))
            .unwrap_or(Duration::ZERO)
    }

    /// Resolves and locks the state for `frag_idx` synchronously (zero awaits).
    pub fn resolve_fragment_state(&self, frag_idx: u64) -> PlaceholderState {
        if let Some(&cached) = self.frag_states.read().unwrap().get(&frag_idx) {
            return cached;
        }
        let init = PlaceholderState::from_u8(self.initial_state.load(Ordering::Acquire));
        let chosen = if frag_idx < INITIAL_HOLD_FRAGMENTS && init.is_transient_initial() {
            init
        } else {
            self.latest()
        };
        *self
            .frag_states
            .write()
            .unwrap()
            .entry(frag_idx)
            .or_insert(chosen)
    }
}

pub struct StreamGuard {
    session: Arc<LiveSession>,
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        let prev = self.session.active_streams.fetch_sub(1, Ordering::AcqRel);
        self.session.touch();
        let _ = self.session.stream_count_tx.send(prev.saturating_sub(1));
    }
}

#[derive(Default)]
pub struct LiveSessionStore {
    sessions: RwLock<HashMap<String, Arc<LiveSession>>>,
}

impl LiveSessionStore {
    /// Returns `true` when `session_key` has an open stream or closed very recently (Range seek).
    pub async fn has_active_session(&self, session_key: &str) -> bool {
        let map = self.sessions.read().await;
        map.get(session_key).is_some_and(|session| {
            session.active_streams.load(Ordering::Acquire) > 0
                || session.elapsed_since_touch(Instant::now()) < ACTIVE_SESSION_GRACE
        })
    }

    /// Acquires a session handle and RAII active-stream guard; resets fragments on a fresh open.
    pub async fn acquire_session(
        &self,
        session_key: &str,
        initial: PlaceholderState,
        is_fresh_start: bool,
    ) -> (Arc<LiveSession>, StreamGuard) {
        let now = Instant::now();
        let session = {
            let mut map = self.sessions.write().await;
            if !map.contains_key(session_key) {
                map.retain(|_, s| {
                    s.active_streams.load(Ordering::Acquire) > 0
                        || s.poller_running.load(Ordering::Acquire)
                        || s.elapsed_since_touch(now) < SESSION_TTL
                });
            }
            map.entry(session_key.to_string())
                .or_insert_with(|| Arc::new(LiveSession::new(initial)))
                .clone()
        };

        let prev = session.active_streams.fetch_add(1, Ordering::AcqRel);
        if is_fresh_start && prev == 0 {
            session.frag_states.write().unwrap().clear();
            session
                .initial_state
                .store(initial.to_u8(), Ordering::Release);
            session
                .latest_state
                .store(initial.to_u8(), Ordering::Release);
        }
        session.touch();
        let _ = session.stream_count_tx.send(prev + 1);

        (session.clone(), StreamGuard { session })
    }
}

struct PollerRunningGuard(Arc<AtomicBool>);

impl Drop for PollerRunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn spawn_session_poller<F, Fut>(
    session: Arc<LiveSession>,
    poll_interval: Duration,
    idle_grace: Duration,
    mut poll_fn: F,
) where
    F: FnMut() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Option<PlaceholderState>> + Send,
{
    if session
        .poller_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    let mut count_rx = session.stream_count_tx.subscribe();

    tokio::spawn(async move {
        let _guard = PollerRunningGuard(session.poller_running.clone());
        let mut reached_ready = session.latest() == PlaceholderState::Ready;

        loop {
            if session.active_streams.load(Ordering::Acquire) == 0 {
                let _ = count_rx.borrow_and_update();
                if session.active_streams.load(Ordering::Acquire) == 0 {
                    let sleep = tokio::time::sleep(idle_grace);
                    tokio::pin!(sleep);
                    loop {
                        tokio::select! {
                            _ = &mut sleep => break,
                            res = count_rx.changed() => {
                                if res.is_err() || *count_rx.borrow_and_update() > 0 {
                                    break;
                                }
                            }
                        }
                    }
                    if session.active_streams.load(Ordering::Acquire) == 0 {
                        return;
                    }
                }
            }

            if !reached_ready && let Some(next_state) = poll_fn().await {
                session.set_latest(next_state);
                if next_state == PlaceholderState::Ready {
                    reached_ready = true;
                }
            }

            if reached_ready {
                let _ = count_rx.borrow_and_update();
                if session.active_streams.load(Ordering::Acquire) > 0
                    && count_rx.changed().await.is_err()
                {
                    return;
                }
            } else {
                let sleep = tokio::time::sleep(poll_interval);
                tokio::pin!(sleep);
                loop {
                    tokio::select! {
                        _ = &mut sleep => break,
                        res = count_rx.changed() => {
                            if res.is_err() {
                                return;
                            }
                            if *count_rx.borrow_and_update() == 0 {
                                break;
                            }
                        }
                    }
                }
            }
        }
    });
}

#[derive(Default)]
struct PollerCache {
    resolved_hash: Option<String>,
    resolved_file_idx: Option<usize>,
    resolved_file_path: Option<String>,
    cached_series_params: Option<(bool, u32, u32, String)>,
}

async fn poll_live_state_once(
    app_state: &AppState,
    mode: &str,
    stremio_id: &str,
    info_hash: &str,
    file_idx: Option<usize>,
    file_path: &str,
    cache: &mut PollerCache,
) -> Option<PlaceholderState> {
    let base_id = base_imdb_id(stremio_id);
    if cache.resolved_hash.is_none() && is_valid_hex_hash(info_hash) {
        cache.resolved_hash = Some(info_hash.to_lowercase());
    }

    match mode {
        "trigger" => {
            let tagged_torrents = tokio::time::timeout(
                Duration::from_millis(1500),
                app_state.qbit.get_torrents_by_tag(base_id),
            )
            .await
            .ok()?;

            if cache.resolved_hash.is_none() {
                let chosen = if tagged_torrents.len() == 1 {
                    tagged_torrents.first()
                } else {
                    tagged_torrents.iter().max_by_key(|t| t.added_on)
                };
                cache.resolved_hash = chosen.map(|t| t.hash.to_lowercase());
            }

            let Some(ref hash) = cache.resolved_hash else {
                return Some(PlaceholderState::FetchingMetadata);
            };

            let files = tokio::time::timeout(
                Duration::from_millis(1500),
                app_state.qbit.get_torrent_files(hash),
            )
            .await
            .ok()?;

            let parent = tagged_torrents
                .iter()
                .find(|t| t.hash.eq_ignore_ascii_case(hash));
            let stalled = parent.is_some_and(|t| is_torrent_stalled(&t.state, t.dlspeed));

            if files.is_empty() {
                return Some(
                    if parent.is_some_and(|t| matches!(t.state.trim(), "error" | "missingFiles")) {
                        PlaceholderState::Stalled
                    } else {
                        PlaceholderState::FetchingMetadata
                    },
                );
            }

            if cache.resolved_file_path.is_none() {
                let (is_series, season, episode, expected_title) = match &cache.cached_series_params
                {
                    Some(params) => params.clone(),
                    None => {
                        let params = get_series_params(app_state, stremio_id).await;
                        cache.cached_series_params = Some(params.clone());
                        params
                    }
                };
                if let Some(best_idx) =
                    match_best_qbit_file(&files, &expected_title, is_series, season, episode)
                {
                    cache.resolved_file_idx = Some(files[best_idx].index.unwrap_or(best_idx));
                    cache.resolved_file_path = Some(files[best_idx].name.clone());
                }
            }

            let target_path = cache.resolved_file_path.as_deref().unwrap_or("");
            let target_file = find_target_file(&files, cache.resolved_file_idx, target_path);
            let exists_on_disk = file_exists_nonempty(&app_state.config.download_path, target_path);

            Some(match target_file {
                Some((_, f)) => decide_live_file_state(exists_on_disk, f.size, f.progress, stalled),
                None if stalled => PlaceholderState::Stalled,
                None => PlaceholderState::Buffering(0),
            })
        }
        "stream" => {
            let exists_on_disk = file_exists_nonempty(&app_state.config.download_path, file_path);
            let files = tokio::time::timeout(
                Duration::from_millis(1500),
                app_state.qbit.get_torrent_files(info_hash),
            )
            .await
            .ok()?;

            Some(match find_target_file(&files, None, file_path) {
                Some((_, f)) => decide_live_file_state(exists_on_disk, f.size, f.progress, false),
                None if exists_on_disk => PlaceholderState::Ready,
                None => PlaceholderState::FilesMissing,
            })
        }
        _ => {
            let exists_on_disk = file_exists_nonempty(&app_state.config.download_path, file_path);
            let (files, tagged_torrents) =
                tokio::time::timeout(Duration::from_millis(1500), async {
                    tokio::join!(
                        app_state.qbit.get_torrent_files(info_hash),
                        app_state.qbit.get_torrents_by_tag(base_id),
                    )
                })
                .await
                .ok()?;

            let stalled = tagged_torrents
                .iter()
                .find(|t| t.hash.eq_ignore_ascii_case(info_hash))
                .is_some_and(|t| is_torrent_stalled(&t.state, t.dlspeed));

            Some(match find_target_file(&files, file_idx, file_path) {
                Some((_, f)) => decide_live_file_state(exists_on_disk, f.size, f.progress, stalled),
                None if stalled => PlaceholderState::Stalled,
                None => PlaceholderState::Buffering(0),
            })
        }
    }
}

fn start_qbit_poller(
    app_state: AppState,
    session: Arc<LiveSession>,
    mode: String,
    stremio_id: String,
    info_hash: String,
    file_idx: Option<usize>,
    file_path: String,
) {
    let cache = Arc::new(tokio::sync::Mutex::new(PollerCache::default()));
    spawn_session_poller(session, POLL_INTERVAL, POLLER_IDLE_GRACE, move || {
        let (app_state, mode, stremio_id, info_hash, file_path, cache) = (
            app_state.clone(),
            mode.clone(),
            stremio_id.clone(),
            info_hash.clone(),
            file_path.clone(),
            cache.clone(),
        );
        async move {
            let mut guard = cache.lock().await;
            poll_live_state_once(
                &app_state,
                &mode,
                &stremio_id,
                &info_hash,
                file_idx,
                &file_path,
                &mut guard,
            )
            .await
        }
    });
}

struct StreamCursor {
    bank: Arc<PlaceholderBank>,
    session: Arc<LiveSession>,
    _guard: StreamGuard,
    pos: u64,
    end: u64,
    pace_stream: bool,
    burst_bytes: u64,
    bytes_emitted: u64,
    stream_start: Instant,
    current_frag: Option<(u64, Vec<u8>)>,
}

/// Axum handler serving a virtual multi-hour fMP4 that reads live-polled state with zero awaits.
pub async fn serve_live_placeholder(
    State(state): State<AppState>,
    AxumPath((api_key, mode, stremio_id, raw_info_hash)): AxumPath<(
        String,
        String,
        String,
        String,
    )>,
    Query(params): Query<HashMap<String, String>>,
    req: Request,
) -> Response {
    if !state.db.validate_api_key(&api_key) {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }

    let Some(bank) = state.placeholder_bank.clone() else {
        return ServeFile::new(Path::new("placeholder.mp4"))
            .oneshot(req)
            .await
            .unwrap()
            .into_response();
    };

    let info_hash = raw_info_hash
        .strip_suffix(".mp4")
        .unwrap_or(&raw_info_hash)
        .to_string();
    let initial_state = params
        .get("initial")
        .and_then(|id| PlaceholderState::from_id(id))
        .unwrap_or(PlaceholderState::Buffering(0));
    let file_idx = params.get("fileIdx").and_then(|s| s.parse::<usize>().ok());
    let file_path = params.get("filePath").cloned().unwrap_or_default();

    let (status, start, end, is_partial, is_bounded_range) =
        match parse_http_range(req.headers(), bank.total_len) {
            RangeParseResult::None => (StatusCode::OK, 0, bank.total_len - 1, false, false),
            RangeParseResult::Satisfiable(s, e) => (
                StatusCode::PARTIAL_CONTENT,
                s,
                e,
                true,
                e < bank.total_len - 1,
            ),
            RangeParseResult::Unsatisfiable => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{}", bank.total_len))
                    .body(Body::empty())
                    .unwrap();
            }
        };

    let content_len = end - start + 1;
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, content_len.to_string())
        .header(header::CACHE_CONTROL, "no-store, no-cache, must-revalidate");

    if is_partial {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, bank.total_len),
        );
    }

    if req.method() == Method::HEAD {
        return builder.body(Body::empty()).unwrap();
    }

    let burst_bytes = BURST_FRAGMENTS * bank.slot_size;
    let pace_stream = !is_bounded_range && content_len > burst_bytes;
    let session_key = build_session_key(
        &api_key,
        &mode,
        &stremio_id,
        &info_hash,
        file_idx,
        &file_path,
    );
    let (session, guard) = state
        .live_sessions
        .acquire_session(&session_key, initial_state, start == 0)
        .await;

    if end >= bank.header.len() as u64 {
        start_qbit_poller(
            state,
            session.clone(),
            mode,
            stremio_id,
            info_hash,
            file_idx,
            file_path,
        );
    }

    let cursor = StreamCursor {
        bank,
        session,
        _guard: guard,
        pos: start,
        end,
        pace_stream,
        burst_bytes,
        bytes_emitted: 0,
        stream_start: Instant::now(),
        current_frag: None,
    };

    let body_stream = stream::unfold(cursor, |mut ctx| async move {
        if ctx.pos > ctx.end {
            return None;
        }

        let header_len = ctx.bank.header.len() as u64;
        if ctx.pos < header_len {
            let hdr_start = ctx.pos as usize;
            let hdr_end =
                ((ctx.end + 1).min(header_len) as usize).min(hdr_start + STREAM_CHUNK_SIZE);
            let chunk = ctx.bank.header.slice(hdr_start..hdr_end);
            ctx.pos += chunk.len() as u64;
            ctx.bytes_emitted += chunk.len() as u64;
            return Some((Ok::<Bytes, Infallible>(chunk), ctx));
        }

        if ctx.pace_stream && ctx.bytes_emitted >= ctx.burst_bytes {
            let excess_bytes = ctx.bytes_emitted - ctx.burst_bytes;
            let target_ns = ((excess_bytes as u128) * (SEG_DURATION_NS as u128) * 100
                / ((ctx.bank.slot_size as u128) * 105)) as u64;
            let target_tokio =
                tokio::time::Instant::from_std(ctx.stream_start + Duration::from_nanos(target_ns));
            if tokio::time::Instant::now() < target_tokio {
                sleep_until(target_tokio).await;
            }
        }

        let rel = ctx.pos - header_len;
        let frag_idx = rel / ctx.bank.slot_size;
        let in_frag = (rel % ctx.bank.slot_size) as usize;

        if ctx
            .current_frag
            .as_ref()
            .is_none_or(|(idx, _)| *idx != frag_idx)
        {
            let resolved = ctx.session.resolve_fragment_state(frag_idx);
            ctx.current_frag = Some((frag_idx, ctx.bank.build_fragment(resolved, frag_idx)));
        }

        let frag_bytes = &ctx.current_frag.as_ref().unwrap().1;
        let take = ((ctx.end - ctx.pos + 1) as usize)
            .min(frag_bytes.len() - in_frag)
            .min(STREAM_CHUNK_SIZE);

        let chunk = Bytes::copy_from_slice(&frag_bytes[in_frag..in_frag + take]);
        ctx.pos += take as u64;
        ctx.bytes_emitted += take as u64;
        Some((Ok::<Bytes, Infallible>(chunk), ctx))
    });

    builder.body(Body::from_stream(body_stream)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_range_headers() {
        let make_headers = |val: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::RANGE, val.parse().unwrap());
            h
        };

        assert_eq!(
            parse_http_range(&HeaderMap::new(), 1000),
            RangeParseResult::None
        );
        assert_eq!(
            parse_http_range(&make_headers("bytes=0-1"), 1000),
            RangeParseResult::Satisfiable(0, 1)
        );
        assert_eq!(
            parse_http_range(&make_headers("bytes=0-"), 1000),
            RangeParseResult::Satisfiable(0, 999)
        );
        assert_eq!(
            parse_http_range(&make_headers("bytes=100-200"), 1000),
            RangeParseResult::Satisfiable(100, 200)
        );
        assert_eq!(
            parse_http_range(&make_headers("bytes=500-9999"), 1000),
            RangeParseResult::Satisfiable(500, 999)
        );
        assert_eq!(
            parse_http_range(&make_headers("bytes=-250"), 1000),
            RangeParseResult::Satisfiable(750, 999)
        );
        assert_eq!(
            parse_http_range(&make_headers("bytes=1000-"), 1000),
            RangeParseResult::Unsatisfiable
        );
        assert_eq!(
            parse_http_range(&make_headers("bytes=200-100"), 1000),
            RangeParseResult::Unsatisfiable
        );
        assert_eq!(
            parse_http_range(&make_headers("bytes=-0"), 1000),
            RangeParseResult::Unsatisfiable
        );
        assert_eq!(
            parse_http_range(&make_headers("items=0-5"), 1000),
            RangeParseResult::None
        );
    }

    #[test]
    fn buffering_progress_maps_to_0_through_9() {
        assert_eq!(
            PlaceholderState::buffering_for_progress(0.0),
            PlaceholderState::Buffering(0)
        );
        assert_eq!(
            PlaceholderState::buffering_for_progress(0.009),
            PlaceholderState::Buffering(0)
        );
        assert_eq!(
            PlaceholderState::buffering_for_progress(0.01),
            PlaceholderState::Buffering(1)
        );
        assert_eq!(
            PlaceholderState::buffering_for_progress(0.055),
            PlaceholderState::Buffering(5)
        );
        assert_eq!(
            PlaceholderState::buffering_for_progress(0.095),
            PlaceholderState::Buffering(9)
        );
        assert_eq!(
            PlaceholderState::buffering_for_progress(0.15),
            PlaceholderState::Buffering(9)
        );
        assert_eq!(
            PlaceholderState::buffering_for_progress(-0.5),
            PlaceholderState::Buffering(0)
        );
        assert_eq!(
            PlaceholderState::buffering_for_progress(f64::NAN),
            PlaceholderState::Buffering(0)
        );
    }

    #[test]
    fn has_nonzero_range_header_detects_mid_stream_seeks() {
        let make_headers = |val: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::RANGE, val.parse().unwrap());
            h
        };

        assert!(!has_nonzero_range_header(&HeaderMap::new()));
        assert!(!has_nonzero_range_header(&make_headers("bytes=0-")));
        assert!(!has_nonzero_range_header(&make_headers("bytes=0-1")));
        assert!(has_nonzero_range_header(&make_headers("bytes=161378-")));
        assert!(has_nonzero_range_header(&make_headers("bytes=1-100")));
        assert!(has_nonzero_range_header(&make_headers("bytes=-500")));
    }

    #[test]
    fn placeholder_state_u8_round_trips_all_variants() {
        for st in PlaceholderState::all() {
            assert_eq!(PlaceholderState::from_u8(st.to_u8()), st);
        }
        assert_eq!(
            PlaceholderState::from_id("files_missing"),
            Some(PlaceholderState::FilesMissing)
        );
        assert_eq!(
            PlaceholderState::from_id("files-missing"),
            Some(PlaceholderState::FilesMissing)
        );
    }

    #[tokio::test]
    async fn deterministic_session_preserves_frag_bytes_across_state_switch_and_resets_on_fresh_start()
     {
        let store = LiveSessionStore::default();
        let key = build_session_key("key", "trigger", "tt123", "abcd", None, "");

        let (s1, g1) = store
            .acquire_session(&key, PlaceholderState::DownloadStarted, true)
            .await;
        assert_eq!(
            s1.resolve_fragment_state(0),
            PlaceholderState::DownloadStarted
        );

        s1.set_latest(PlaceholderState::FetchingMetadata);

        let (s2, g2) = store
            .acquire_session(&key, PlaceholderState::FetchingMetadata, false)
            .await;
        assert!(Arc::ptr_eq(&s1, &s2));
        assert_eq!(
            s2.resolve_fragment_state(0),
            PlaceholderState::DownloadStarted
        );
        assert_eq!(
            s2.resolve_fragment_state(2),
            PlaceholderState::FetchingMetadata
        );

        drop(g2);
        drop(g1);

        let (s3, _g3) = store
            .acquire_session(&key, PlaceholderState::Buffering(4), true)
            .await;
        assert_eq!(s3.resolve_fragment_state(0), PlaceholderState::Buffering(4));
    }

    #[tokio::test]
    async fn poller_shuts_down_when_stream_closes() {
        let store = LiveSessionStore::default();
        let key = build_session_key("key", "play", "tt999", "deadbeef", Some(0), "a.mkv");
        let (session, guard) = store
            .acquire_session(&key, PlaceholderState::Buffering(0), true)
            .await;

        let polls = Arc::new(AtomicUsize::new(0));
        let polls_clone = polls.clone();
        spawn_session_poller(
            session.clone(),
            Duration::from_millis(20),
            Duration::from_millis(40),
            move || {
                let p = polls_clone.clone();
                async move {
                    p.fetch_add(1, Ordering::Relaxed);
                    Some(PlaceholderState::Buffering(2))
                }
            },
        );

        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(polls.load(Ordering::Relaxed) >= 1);
        assert!(session.poller_running.load(Ordering::Acquire));
        assert_eq!(session.latest(), PlaceholderState::Buffering(2));

        drop(guard);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!session.poller_running.load(Ordering::Acquire));
    }
}
