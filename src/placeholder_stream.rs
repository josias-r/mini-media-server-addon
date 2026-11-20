use crate::matcher::find_best_file_match;
use crate::routes::play::{is_file_stream_ready, is_torrent_stalled};
use crate::routes::stream::resolve_safe_download_path;
use crate::routes::{AppState, require_valid_api_key};
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

const SEGMENTS_PER_LOOP: usize = 7;
const VIDEO_TRACK_ID: u32 = 1;
const VIDEO_TIMESCALE: u32 = 15_360;
const VIDEO_SEG_DURATION: u64 = 32_768; // 64 frames @ 30fps in 15360 timescale
const AUDIO_TRACK_ID: u32 = 2;
const AUDIO_SEG_DURATION: u64 = 102_400; // 100 AAC frames * 1024 samples @ 48kHz
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

impl PlaceholderState {
    /// Returns the asset directory ID matching `placeholders/placeholders.yaml`.
    pub fn id(self) -> &'static str {
        match self {
            Self::DownloadStarted => "download_started",
            Self::FetchingMetadata => "fetching_metadata",
            Self::EpisodeRequested => "episode_requested",
            Self::Stalled => "stalled",
            Self::Buffering(0) => "buffering_0",
            Self::Buffering(1) => "buffering_1",
            Self::Buffering(2) => "buffering_2",
            Self::Buffering(3) => "buffering_3",
            Self::Buffering(4) => "buffering_4",
            Self::Buffering(5) => "buffering_5",
            Self::Buffering(6) => "buffering_6",
            Self::Buffering(7) => "buffering_7",
            Self::Buffering(8) => "buffering_8",
            Self::Buffering(_) => "buffering_9",
            Self::Ready => "ready",
            Self::FilesMissing => "files_missing",
        }
    }

    /// Parses a state ID from query parameters or directory names.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "download_started" => Some(Self::DownloadStarted),
            "fetching_metadata" => Some(Self::FetchingMetadata),
            "episode_requested" => Some(Self::EpisodeRequested),
            "stalled" => Some(Self::Stalled),
            "buffering" | "buffering_0" => Some(Self::Buffering(0)),
            "buffering_1" => Some(Self::Buffering(1)),
            "buffering_2" => Some(Self::Buffering(2)),
            "buffering_3" => Some(Self::Buffering(3)),
            "buffering_4" => Some(Self::Buffering(4)),
            "buffering_5" => Some(Self::Buffering(5)),
            "buffering_6" => Some(Self::Buffering(6)),
            "buffering_7" => Some(Self::Buffering(7)),
            "buffering_8" => Some(Self::Buffering(8)),
            "buffering_9" => Some(Self::Buffering(9)),
            "ready" => Some(Self::Ready),
            "files_missing" | "files-missing" => Some(Self::FilesMissing),
            _ => None,
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
        match v {
            0 => Self::DownloadStarted,
            1 => Self::FetchingMetadata,
            2 => Self::EpisodeRequested,
            3 => Self::Stalled,
            4..=13 => Self::Buffering(v - 4),
            14 => Self::Ready,
            15 => Self::FilesMissing,
            _ => Self::Buffering(0),
        }
    }

    /// Returns all 16 placeholder states required on disk.
    pub fn all() -> [Self; 16] {
        [
            Self::DownloadStarted,
            Self::FetchingMetadata,
            Self::EpisodeRequested,
            Self::Stalled,
            Self::Buffering(0),
            Self::Buffering(1),
            Self::Buffering(2),
            Self::Buffering(3),
            Self::Buffering(4),
            Self::Buffering(5),
            Self::Buffering(6),
            Self::Buffering(7),
            Self::Buffering(8),
            Self::Buffering(9),
            Self::Ready,
            Self::FilesMissing,
        ]
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
    let Some(raw) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some(spec) = raw.trim().strip_prefix("bytes=") else {
        return false;
    };
    if spec.contains(',') {
        return false;
    }
    let Some((start_str, _)) = spec.split_once('-') else {
        return false;
    };
    let start_str = start_str.trim();
    if start_str.is_empty() {
        return true;
    }
    matches!(start_str.parse::<u64>(), Ok(start) if start > 0)
}

fn file_exists_nonempty(download_root: &str, file_path: &str) -> bool {
    resolve_safe_download_path(download_root, file_path).is_some_and(|p| {
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
    })
}

/// Maps live qBittorrent file progress, stall status, and disk presence to a placeholder state.
pub fn decide_live_file_state(
    exists_on_disk: bool,
    file_size: u64,
    file_progress: f64,
    torrent_stalled: bool,
) -> PlaceholderState {
    let stream_ready = is_file_stream_ready(file_size, file_progress);
    if stream_ready {
        return if exists_on_disk {
            PlaceholderState::Ready
        } else {
            PlaceholderState::FilesMissing
        };
    }

    if file_progress >= 0.10 && !exists_on_disk {
        return PlaceholderState::FilesMissing;
    }

    if torrent_stalled {
        return PlaceholderState::Stalled;
    }

    PlaceholderState::buffering_for_progress(file_progress)
}

#[derive(Debug, Clone)]
struct SegmentTemplate {
    raw: Vec<u8>,
    mfhd_seq_offset: usize,
    video_tfdt_offset: usize,
    video_tfdt_v1: bool,
    audio_tfdt_offset: usize,
    audio_tfdt_v1: bool,
}

#[derive(Debug)]
pub struct PlaceholderBank {
    pub header: Bytes,
    pub slot_size: u64,
    pub total_fragments: u64,
    pub total_len: u64,
    states: HashMap<PlaceholderState, [SegmentTemplate; SEGMENTS_PER_LOOP]>,
}

fn parse_boxes(
    data: &[u8],
    start: usize,
    end: usize,
) -> Result<Vec<([u8; 4], usize, usize)>, String> {
    if end > data.len() || start > end {
        return Err("MP4 slice bounds out of range".to_string());
    }
    let mut pos = start;
    let mut out = Vec::new();
    while pos + 8 <= end {
        let raw_sz = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let typ: [u8; 4] = data[pos + 4..pos + 8].try_into().unwrap();
        let sz = if raw_sz == 0 { end - pos } else { raw_sz };
        if sz < 8 || pos + sz > end {
            return Err(format!(
                "Invalid MP4 box {:?} size {} at {}",
                String::from_utf8_lossy(&typ),
                sz,
                pos
            ));
        }
        out.push((typ, pos, sz));
        pos += sz;
    }
    if pos != end {
        return Err(format!("Trailing {} unparsed MP4 bytes", end - pos));
    }
    Ok(out)
}

fn parse_segment_template(raw: Vec<u8>) -> Result<SegmentTemplate, String> {
    let top = parse_boxes(&raw, 0, raw.len())?;
    if top.len() != 2 || &top[0].0 != b"moof" || &top[1].0 != b"mdat" {
        return Err("Segment must consist of [moof, mdat]".to_string());
    }
    let moof_end = top[0].2;
    let mut mfhd_seq_offset = None;
    let mut video_tfdt = None;
    let mut audio_tfdt = None;

    for (ctyp, cpos, csz) in parse_boxes(&raw, 8, moof_end)? {
        if &ctyp == b"mfhd" {
            if csz < 16 {
                return Err("mfhd box too small".to_string());
            }
            mfhd_seq_offset = Some(cpos + 12);
        } else if &ctyp == b"traf" {
            let mut track_id = None;
            let mut tfdt_info = None;
            for (ttyp, tpos, tsz) in parse_boxes(&raw, cpos + 8, cpos + csz)? {
                if &ttyp == b"tfhd" {
                    if tsz < 16 {
                        return Err("tfhd box too small".to_string());
                    }
                    track_id = Some(u32::from_be_bytes(
                        raw[tpos + 12..tpos + 16].try_into().unwrap(),
                    ));
                } else if &ttyp == b"tfdt" {
                    if tsz < 16 {
                        return Err("tfdt box too small".to_string());
                    }
                    let ver = raw[tpos + 8];
                    let is_v1 = match ver {
                        0 => false,
                        1 => {
                            if tsz < 20 {
                                return Err("tfdt v1 box too small".to_string());
                            }
                            true
                        }
                        _ => return Err(format!("Unsupported tfdt version {}", ver)),
                    };
                    tfdt_info = Some((tpos + 12, is_v1));
                }
            }
            match (track_id, tfdt_info) {
                (Some(VIDEO_TRACK_ID), Some(info)) => video_tfdt = Some(info),
                (Some(AUDIO_TRACK_ID), Some(info)) => audio_tfdt = Some(info),
                _ => {}
            }
        }
    }

    let mfhd_seq_offset = mfhd_seq_offset.ok_or_else(|| "Missing mfhd in segment".to_string())?;
    let (video_tfdt_offset, video_tfdt_v1) =
        video_tfdt.ok_or_else(|| "Missing video tfdt in segment".to_string())?;
    let (audio_tfdt_offset, audio_tfdt_v1) =
        audio_tfdt.ok_or_else(|| "Missing audio tfdt in segment".to_string())?;

    Ok(SegmentTemplate {
        raw,
        mfhd_seq_offset,
        video_tfdt_offset,
        video_tfdt_v1,
        audio_tfdt_offset,
        audio_tfdt_v1,
    })
}

fn patch_init_with_mehd(init_bytes: &[u8], total_fragments: u64) -> Result<Vec<u8>, String> {
    let top = parse_boxes(init_bytes, 0, init_bytes.len())?;
    if top.len() < 2 || &top[0].0 != b"ftyp" || &top[1].0 != b"moov" {
        return Err("init.mp4 must begin with ftyp + moov".to_string());
    }

    let ftyp = &init_bytes[top[0].1..top[0].1 + top[0].2];
    let moov_start = top[1].1;
    let moov_end = moov_start + top[1].2;
    let moov_children = parse_boxes(init_bytes, moov_start + 8, moov_end)?;

    let mut mvhd_timescale: Option<u32> = None;
    for (ctyp, cpos, csz) in &moov_children {
        if ctyp == b"mvhd" {
            let ver = init_bytes[cpos + 8];
            let ts_off = if ver == 0 { cpos + 20 } else { cpos + 28 };
            if ts_off + 4 > cpos + csz {
                return Err("mvhd box truncated".to_string());
            }
            mvhd_timescale = Some(u32::from_be_bytes(
                init_bytes[ts_off..ts_off + 4].try_into().unwrap(),
            ));
            break;
        }
    }
    let mvhd_timescale = mvhd_timescale.ok_or_else(|| "Missing mvhd in init.mp4".to_string())?;
    let mehd_duration = ((total_fragments as u128) * 64 * (mvhd_timescale as u128) / 30) as u64;

    let mut moov_inner = Vec::with_capacity(top[1].2 + 32);
    let mut saw_mvex = false;
    for (ctyp, cpos, csz) in moov_children {
        if &ctyp == b"mvex" {
            saw_mvex = true;
            let mut mvex_inner = Vec::with_capacity(csz + 24);
            mvex_inner.extend_from_slice(&20u32.to_be_bytes());
            mvex_inner.extend_from_slice(b"mehd");
            mvex_inner.extend_from_slice(&0x0100_0000u32.to_be_bytes());
            mvex_inner.extend_from_slice(&mehd_duration.to_be_bytes());

            for (mtyp, mpos, msz) in parse_boxes(init_bytes, cpos + 8, cpos + csz)? {
                if &mtyp != b"mehd" {
                    mvex_inner.extend_from_slice(&init_bytes[mpos..mpos + msz]);
                }
            }
            moov_inner.extend_from_slice(&((8 + mvex_inner.len()) as u32).to_be_bytes());
            moov_inner.extend_from_slice(b"mvex");
            moov_inner.extend_from_slice(&mvex_inner);
        } else {
            moov_inner.extend_from_slice(&init_bytes[cpos..cpos + csz]);
        }
    }

    if !saw_mvex {
        return Err("Missing mvex in init.mp4".to_string());
    }

    let mut out = Vec::with_capacity(ftyp.len() + 8 + moov_inner.len());
    out.extend_from_slice(ftyp);
    out.extend_from_slice(&((8 + moov_inner.len()) as u32).to_be_bytes());
    out.extend_from_slice(b"moov");
    out.extend_from_slice(&moov_inner);
    Ok(out)
}

fn build_sidx(total_fragments: u64, slot_size: u64) -> Result<Vec<u8>, String> {
    let ref_count = u16::try_from(total_fragments)
        .map_err(|_| "total_fragments exceeds sidx u16 reference_count limit".to_string())?;
    if slot_size > 0x7FFF_FFFF {
        return Err("slot_size exceeds 31-bit sidx referenced_size limit".to_string());
    }
    let box_len = 40 + 12 * (ref_count as usize);
    let mut sidx = Vec::with_capacity(box_len);
    sidx.extend_from_slice(&(box_len as u32).to_be_bytes());
    sidx.extend_from_slice(b"sidx");
    sidx.extend_from_slice(&0x0100_0000u32.to_be_bytes()); // version=1, flags=0
    sidx.extend_from_slice(&VIDEO_TRACK_ID.to_be_bytes());
    sidx.extend_from_slice(&VIDEO_TIMESCALE.to_be_bytes());
    sidx.extend_from_slice(&0u64.to_be_bytes()); // earliest_presentation_time
    sidx.extend_from_slice(&0u64.to_be_bytes()); // first_offset
    sidx.extend_from_slice(&0u16.to_be_bytes()); // reserved
    sidx.extend_from_slice(&ref_count.to_be_bytes());

    let ref_word = (slot_size as u32) & 0x7FFF_FFFF;
    let dur_word = VIDEO_SEG_DURATION as u32;
    let sap_word = 0x9000_0000u32; // starts_with_SAP=1, SAP_type=1

    for _ in 0..ref_count {
        sidx.extend_from_slice(&ref_word.to_be_bytes());
        sidx.extend_from_slice(&dur_word.to_be_bytes());
        sidx.extend_from_slice(&sap_word.to_be_bytes());
    }
    Ok(sidx)
}

fn write_tfdt(buf: &mut [u8], offset: usize, is_v1: bool, value: u64) {
    if is_v1 {
        buf[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
    } else {
        buf[offset..offset + 4].copy_from_slice(&(value as u32).to_be_bytes());
    }
}

impl PlaceholderBank {
    /// Loads `init.mp4` and all 16 × 7 `.m4s` placeholder segments and precomputes the virtual fMP4 layout.
    pub fn load(dir: &Path, duration_hours: u64) -> Result<Self, String> {
        let init_path = dir.join("init.mp4");
        let init_bytes = std::fs::read(&init_path)
            .map_err(|e| format!("Failed to read {:?}: {}", init_path, e))?;

        let mut states = HashMap::with_capacity(16);
        let mut max_seg_len = 0usize;

        for state in PlaceholderState::all() {
            let state_dir = dir.join(state.id());
            let mut templates: Vec<SegmentTemplate> = Vec::with_capacity(SEGMENTS_PER_LOOP);
            for idx in 0..SEGMENTS_PER_LOOP {
                let seg_path = state_dir.join(format!("seg_{}.m4s", idx));
                let raw = std::fs::read(&seg_path)
                    .map_err(|e| format!("Failed to read {:?}: {}", seg_path, e))?;
                max_seg_len = max_seg_len.max(raw.len());
                templates.push(parse_segment_template(raw)?);
            }
            let arr: [SegmentTemplate; SEGMENTS_PER_LOOP] = templates
                .try_into()
                .map_err(|_| "Expected exactly 7 segments per state".to_string())?;
            states.insert(state, arr);
        }

        let hours = duration_hours.clamp(1, 12);
        let total_fragments = ((hours * 3600 * 30).div_ceil(64)).min(u16::MAX as u64);
        // Reserve >= 8 bytes so any padding fits a valid ISO BMFF `free` box.
        let slot_size = ((max_seg_len as u64 + 8) + 4095) & !4095;

        let mut header_vec = patch_init_with_mehd(&init_bytes, total_fragments)?;
        let sidx = build_sidx(total_fragments, slot_size)?;
        header_vec.extend_from_slice(&sidx);
        let header = Bytes::from(header_vec);
        let total_len = header.len() as u64 + total_fragments * slot_size;

        Ok(Self {
            header,
            slot_size,
            total_fragments,
            total_len,
            states,
        })
    }

    /// Materializes a single fixed-slot `[moof][mdat][free?]` fragment for `frag_idx`.
    pub fn build_fragment(&self, state: PlaceholderState, frag_idx: u64) -> Vec<u8> {
        let seg_idx = (frag_idx % SEGMENTS_PER_LOOP as u64) as usize;
        let seg = &self
            .states
            .get(&state)
            .or_else(|| self.states.get(&PlaceholderState::Buffering(0)))
            .expect("PlaceholderBank always contains Buffering(0)")[seg_idx];

        let slot_usize = self.slot_size as usize;
        let mut buf = Vec::with_capacity(slot_usize);
        buf.extend_from_slice(&seg.raw);

        let seq_num = ((frag_idx + 1) as u32).to_be_bytes();
        buf[seg.mfhd_seq_offset..seg.mfhd_seq_offset + 4].copy_from_slice(&seq_num);

        write_tfdt(
            &mut buf,
            seg.video_tfdt_offset,
            seg.video_tfdt_v1,
            frag_idx * VIDEO_SEG_DURATION,
        );
        write_tfdt(
            &mut buf,
            seg.audio_tfdt_offset,
            seg.audio_tfdt_v1,
            frag_idx * AUDIO_SEG_DURATION,
        );

        let rem = slot_usize - buf.len();
        if rem > 0 {
            debug_assert!(rem >= 8);
            buf.extend_from_slice(&(rem as u32).to_be_bytes());
            buf.extend_from_slice(b"free");
            buf.resize(slot_usize, 0);
        }

        buf
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

    fn initial(&self) -> PlaceholderState {
        PlaceholderState::from_u8(self.initial_state.load(Ordering::Acquire))
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
        let init = self.initial();
        let chosen = if frag_idx < INITIAL_HOLD_FRAGMENTS && init.is_transient_initial() {
            init
        } else {
            self.latest()
        };
        let mut map = self.frag_states.write().unwrap();
        *map.entry(frag_idx).or_insert(chosen)
    }
}

pub struct StreamGuard {
    session: Arc<LiveSession>,
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        let prev = self.session.active_streams.fetch_sub(1, Ordering::AcqRel);
        let new_count = prev.saturating_sub(1);
        self.session.touch();
        let _ = self.session.stream_count_tx.send(new_count);
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
        let Some(session) = map.get(session_key) else {
            return false;
        };
        session.active_streams.load(Ordering::Acquire) > 0
            || session.elapsed_since_touch(Instant::now()) < ACTIVE_SESSION_GRACE
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

#[derive(Debug, PartialEq, Eq)]
pub enum RangeParseResult {
    None,
    Satisfiable(u64, u64),
    Unsatisfiable,
}

/// Parses a single HTTP `Range: bytes=...` header against `total_len`.
pub fn parse_http_range(headers: &HeaderMap, total_len: u64) -> RangeParseResult {
    let Some(raw) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) else {
        return RangeParseResult::None;
    };
    let trimmed = raw.trim();
    let Some(spec) = trimmed.strip_prefix("bytes=") else {
        return RangeParseResult::None;
    };
    if spec.contains(',') {
        return RangeParseResult::None;
    }
    let Some((start_str, end_str)) = spec.split_once('-') else {
        return RangeParseResult::None;
    };

    if total_len == 0 {
        return RangeParseResult::Unsatisfiable;
    }

    let start_str = start_str.trim();
    let end_str = end_str.trim();

    if start_str.is_empty() {
        let Ok(suffix) = end_str.parse::<u64>() else {
            return RangeParseResult::None;
        };
        if suffix == 0 {
            return RangeParseResult::Unsatisfiable;
        }
        let start = total_len.saturating_sub(suffix);
        return RangeParseResult::Satisfiable(start, total_len - 1);
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
        return RangeParseResult::Unsatisfiable;
    }

    RangeParseResult::Satisfiable(start, end.min(total_len - 1))
}

fn is_valid_hex_hash(hash: &str) -> bool {
    let trimmed = hash.trim();
    trimmed.len() == 40 && trimmed.chars().all(|c| c.is_ascii_hexdigit())
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
    let base_id = stremio_id.split(':').next().unwrap_or(stremio_id);
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
                if tagged_torrents.len() == 1 {
                    cache.resolved_hash = Some(tagged_torrents[0].hash.to_lowercase());
                } else if let Some(latest) = tagged_torrents.iter().max_by_key(|t| t.added_on) {
                    cache.resolved_hash = Some(latest.hash.to_lowercase());
                }
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
                let (is_series, season, episode, expected_title) =
                    if let Some(params) = cache.cached_series_params.clone() {
                        params
                    } else {
                        let params = get_series_params(app_state, stremio_id).await;
                        cache.cached_series_params = Some(params.clone());
                        params
                    };
                let file_infos: Vec<(&str, u64)> =
                    files.iter().map(|f| (f.name.as_str(), f.size)).collect();
                if let Some(best_idx) =
                    find_best_file_match(&file_infos, &expected_title, is_series, season, episode)
                {
                    cache.resolved_file_idx = Some(files[best_idx].index.unwrap_or(best_idx));
                    cache.resolved_file_path = Some(files[best_idx].name.clone());
                }
            }

            let target_file = files.iter().enumerate().find(|(i, f)| {
                cache
                    .resolved_file_idx
                    .is_some_and(|idx| f.index.unwrap_or(*i) == idx)
                    || cache
                        .resolved_file_path
                        .as_deref()
                        .is_some_and(|p| !p.is_empty() && f.name == p)
            });

            let target_path = cache.resolved_file_path.as_deref().unwrap_or("");
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

            let target_file = files
                .iter()
                .find(|f| !file_path.is_empty() && f.name == file_path);

            Some(match target_file {
                Some(f) => decide_live_file_state(exists_on_disk, f.size, f.progress, false),
                None if exists_on_disk => PlaceholderState::Ready,
                None => PlaceholderState::FilesMissing,
            })
        }
        _ => {
            // "play" mode
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

            let target_file = files
                .iter()
                .enumerate()
                .find(|(i, f)| {
                    file_idx.is_some_and(|idx| f.index.unwrap_or(*i) == idx)
                        || (!file_path.is_empty() && f.name == file_path)
                })
                .map(|(_, f)| f);

            let parent = tagged_torrents
                .iter()
                .find(|t| t.hash.eq_ignore_ascii_case(info_hash));
            let stalled = parent.is_some_and(|t| is_torrent_stalled(&t.state, t.dlspeed));

            Some(match target_file {
                Some(f) => decide_live_file_state(exists_on_disk, f.size, f.progress, stalled),
                None if stalled => PlaceholderState::Stalled,
                None => PlaceholderState::Buffering(0),
            })
        }
    }
}

struct PollerRunningGuard {
    flag: Arc<AtomicBool>,
}

impl Drop for PollerRunningGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
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
        let _guard = PollerRunningGuard {
            flag: session.poller_running.clone(),
        };
        let mut reached_ready = session.latest() == PlaceholderState::Ready;

        loop {
            if session.active_streams.load(Ordering::Acquire) == 0 {
                // Drain stale watch notifications, then wait up to idle_grace for a Range reopen.
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
        let app_state = app_state.clone();
        let mode = mode.clone();
        let stremio_id = stremio_id.clone();
        let info_hash = info_hash.clone();
        let file_path = file_path.clone();
        let cache = cache.clone();
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
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    let Some(bank) = state.placeholder_bank.clone() else {
        let fallback = Path::new("placeholder.mp4");
        return ServeFile::new(fallback)
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

    let range = parse_http_range(req.headers(), bank.total_len);
    let (status, start, end, is_partial, is_bounded_range) = match range {
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
    let is_fresh_start = start == 0;

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
        .acquire_session(&session_key, initial_state, is_fresh_start)
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
            let target_time = ctx.stream_start + Duration::from_nanos(target_ns);
            let target_tokio = tokio::time::Instant::from_std(target_time);
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
            let frag_bytes = ctx.bank.build_fragment(resolved, frag_idx);
            ctx.current_frag = Some((frag_idx, frag_bytes));
        }

        let frag_bytes = &ctx.current_frag.as_ref().unwrap().1;
        let remaining_in_req = (ctx.end - ctx.pos + 1) as usize;
        let remaining_in_frag = frag_bytes.len() - in_frag;
        let take = remaining_in_req
            .min(remaining_in_frag)
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
    fn loads_all_generated_placeholder_states_and_segments() {
        let bank = PlaceholderBank::load(Path::new("placeholders"), DEFAULT_DURATION_HOURS)
            .expect("should load generated placeholders");
        let expected_fragments =
            ((DEFAULT_DURATION_HOURS.clamp(1, 12) * 3600 * 30).div_ceil(64)).min(u16::MAX as u64);
        assert_eq!(bank.states.len(), 16);
        assert_eq!(bank.total_fragments, expected_fragments);
        assert_eq!(bank.slot_size % 4096, 0);
        assert_eq!(
            bank.total_len,
            bank.header.len() as u64 + expected_fragments * bank.slot_size
        );
    }

    #[test]
    fn spliced_multi_state_virtual_mp4_has_valid_boxes() {
        let bank = PlaceholderBank::load(Path::new("placeholders"), DEFAULT_DURATION_HOURS)
            .expect("should load generated placeholders");

        let sequence = [
            PlaceholderState::DownloadStarted,
            PlaceholderState::DownloadStarted,
            PlaceholderState::FetchingMetadata,
            PlaceholderState::Buffering(0),
            PlaceholderState::Buffering(1),
            PlaceholderState::Buffering(2),
            PlaceholderState::Buffering(3),
            PlaceholderState::Buffering(4),
            PlaceholderState::Buffering(5),
            PlaceholderState::Buffering(6),
            PlaceholderState::Buffering(7),
            PlaceholderState::Buffering(8),
            PlaceholderState::Buffering(9),
            PlaceholderState::Stalled,
            PlaceholderState::FilesMissing,
            PlaceholderState::Ready,
            PlaceholderState::Ready,
        ];

        let mut blob = Vec::new();
        blob.extend_from_slice(&bank.header);
        for (idx, state) in sequence.iter().enumerate() {
            let frag = bank.build_fragment(*state, idx as u64);
            assert_eq!(frag.len(), bank.slot_size as usize);
            blob.extend_from_slice(&frag);
        }

        let boxes = parse_boxes(&blob, 0, blob.len()).expect("valid top-level boxes");
        assert_eq!(&boxes[0].0, b"ftyp");
        assert_eq!(&boxes[1].0, b"moov");
        assert_eq!(&boxes[2].0, b"sidx");

        let mut pos = 3;
        for frag_idx in 0..sequence.len() {
            assert_eq!(&boxes[pos].0, b"moof");
            let moof_pos = boxes[pos].1;
            let moof_sz = boxes[pos].2;
            let tpl = parse_segment_template(
                blob[moof_pos..moof_pos + boxes[pos].2 + boxes[pos + 1].2].to_vec(),
            )
            .expect("valid moof+mdat");
            let seq = u32::from_be_bytes(
                tpl.raw[tpl.mfhd_seq_offset..tpl.mfhd_seq_offset + 4]
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(seq, (frag_idx + 1) as u32);
            let v_tfdt = if tpl.video_tfdt_v1 {
                u64::from_be_bytes(
                    tpl.raw[tpl.video_tfdt_offset..tpl.video_tfdt_offset + 8]
                        .try_into()
                        .unwrap(),
                )
            } else {
                u32::from_be_bytes(
                    tpl.raw[tpl.video_tfdt_offset..tpl.video_tfdt_offset + 4]
                        .try_into()
                        .unwrap(),
                ) as u64
            };
            assert_eq!(v_tfdt, (frag_idx as u64) * VIDEO_SEG_DURATION);

            assert_eq!(&boxes[pos + 1].0, b"mdat");
            let slot_consumed = moof_sz + boxes[pos + 1].2;
            if slot_consumed < bank.slot_size as usize {
                assert_eq!(&boxes[pos + 2].0, b"free");
                assert_eq!(slot_consumed + boxes[pos + 2].2, bank.slot_size as usize);
                pos += 3;
            } else {
                pos += 2;
            }
        }
        assert_eq!(pos, boxes.len());
    }

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
    fn decide_live_file_state_handles_ready_and_files_missing() {
        const MB: u64 = 1024 * 1024;
        const GB: u64 = 1024 * 1024 * 1024;

        // Jumps straight to Ready once stream-ready on disk (even at ~3.5%)
        assert_eq!(
            decide_live_file_state(true, GB, 0.035, false),
            PlaceholderState::Ready
        );
        // Ready in qBit (> 10% or stream-ready) but missing on disk -> FilesMissing
        assert_eq!(
            decide_live_file_state(false, GB, 0.80, false),
            PlaceholderState::FilesMissing
        );
        assert_eq!(
            decide_live_file_state(false, GB, 0.05, false),
            PlaceholderState::FilesMissing
        );
        assert_eq!(
            decide_live_file_state(false, 100 * MB, 0.10, false),
            PlaceholderState::FilesMissing
        );
        assert_eq!(
            decide_live_file_state(false, 100 * MB, 0.12, true),
            PlaceholderState::FilesMissing
        );
        // On disk at 12% of 100 MB (below 20 MB ready threshold) -> Buffering(9)
        assert_eq!(
            decide_live_file_state(true, 100 * MB, 0.12, false),
            PlaceholderState::Buffering(9)
        );
        // Still downloading below ready threshold -> Buffering(pct) or Stalled
        assert_eq!(
            decide_live_file_state(false, GB, 0.0, false),
            PlaceholderState::Buffering(0)
        );
        assert_eq!(
            decide_live_file_state(false, GB, 0.01, false),
            PlaceholderState::Buffering(1)
        );
        assert_eq!(
            decide_live_file_state(false, GB, 0.01, true),
            PlaceholderState::Stalled
        );
        // Also Ready if stream-ready threshold is met before 10% (e.g. > 64 MB and >= 1% on 20 GB)
        assert_eq!(
            decide_live_file_state(true, 20 * GB, 0.012, false),
            PlaceholderState::Ready
        );
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

        // Background poller updates latest state before a second Range connection seeks into frag 0
        s1.set_latest(PlaceholderState::FetchingMetadata);

        let (s2, g2) = store
            .acquire_session(&key, PlaceholderState::FetchingMetadata, false)
            .await;
        assert!(Arc::ptr_eq(&s1, &s2));
        // Existing fragment stays locked to DownloadStarted so mid-fragment bytes never corrupt
        assert_eq!(
            s2.resolve_fragment_state(0),
            PlaceholderState::DownloadStarted
        );
        // Fragments past INITIAL_HOLD_FRAGMENTS pick up the live FetchingMetadata state
        assert_eq!(
            s2.resolve_fragment_state(2),
            PlaceholderState::FetchingMetadata
        );

        drop(g2);
        drop(g1);

        // Re-opening from byte 0 after disconnect resets frag cache cleanly
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
