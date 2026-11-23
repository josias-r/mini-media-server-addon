pub mod matcher;
pub mod parser;

use crate::placeholder::PlaceholderState;
use crate::qbittorrent::QbitFile;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use matcher::find_next_episode_file;
use std::fmt::Write as _;
use std::path::{Component, Path as StdPath, PathBuf};

// ── Stremio IDs, InfoHashes & Torrent Tags ──────────────────────────────────

/// Parses a Stremio ID (e.g. `tt1375666` or `tt0903747:1:2`) into
/// `(normalized_stremio_id, base_imdb_id, media_type)`.
pub fn parse_stremio_media_id(stremio_id: &str) -> Option<(String, String, String)> {
    let trimmed = stremio_id.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.split(':').collect();
    let base_id = parts[0].trim().to_lowercase();
    if base_id.is_empty() {
        return None;
    }

    if parts.len() >= 3 {
        let season: u32 = parts[1].trim().parse().ok()?;
        let episode: u32 = parts[2].trim().parse().ok()?;
        Some((
            format!("{}:{}:{}", base_id, season, episode),
            base_id,
            "series".to_string(),
        ))
    } else if !trimmed.contains(':') {
        Some((base_id.clone(), base_id, "movie".to_string()))
    } else {
        None
    }
}

/// Parses a series Stremio ID (e.g. `tt0903747:1:2`) into `(base_imdb_id, season, episode)`.
pub fn parse_series_episode_id(stremio_id: &str) -> Option<(String, u32, u32)> {
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

/// Extracts the base IMDb ID segment before any `:season:episode` suffix.
pub fn base_imdb_id(stremio_id: &str) -> &str {
    stremio_id.split(':').next().unwrap_or(stremio_id)
}

/// Formats `SxxEyy` when `stremio_id` encodes a series season and episode.
pub fn format_episode_badge(stremio_id: &str) -> Option<String> {
    let parts: Vec<&str> = stremio_id.split(':').collect();
    if parts.len() >= 3
        && let (Ok(season), Ok(episode)) = (
            parts[1].trim().parse::<u32>(),
            parts[2].trim().parse::<u32>(),
        )
    {
        return Some(format!("S{:02}E{:02}", season, episode));
    }
    None
}

/// Returns true if `hash` is a 40-character ASCII hexadecimal BitTorrent info_hash.
pub fn is_valid_hex_hash(hash: &str) -> bool {
    let trimmed = hash.trim();
    trimmed.len() == 40 && trimmed.chars().all(|c| c.is_ascii_hexdigit())
}

/// Returns a trimmed lowercase 40-character hex info_hash if valid.
pub fn normalize_valid_hash(hash: &str) -> Option<String> {
    let trimmed = hash.trim();
    is_valid_hex_hash(trimmed).then(|| trimmed.to_lowercase())
}

fn decode_base32_btih(encoded: &str) -> Option<String> {
    if encoded.len() != 32 {
        return None;
    }

    let mut bytes = [0u8; 20];
    let (mut buffer, mut bits_left, mut out_idx) = (0u32, 0u8, 0usize);

    for b in encoded.bytes() {
        let upper = b.to_ascii_uppercase();
        let val = match upper {
            b'A'..=b'Z' => upper - b'A',
            b'2'..=b'7' => upper - b'2' + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | u32::from(val);
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            bytes[out_idx] = ((buffer >> bits_left) & 0xff) as u8;
            out_idx += 1;
        }
    }

    let mut hex = String::with_capacity(40);
    for byte in bytes {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Some(hex)
}

/// Extracts a 40-character lowercase hex BitTorrent info_hash from a URI containing `urn:btih:`.
/// Supports both 40-character hex and 32-character RFC 4648 Base32 hashes.
pub fn extract_btih_hex(uri: &str) -> Option<String> {
    let start = uri.to_ascii_lowercase().find("urn:btih:")?;
    let hash_part = &uri[start + 9..];
    let end = hash_part.find('&').unwrap_or(hash_part.len());
    let candidate = hash_part[..end].trim();

    if is_valid_hex_hash(candidate) {
        Some(candidate.to_lowercase())
    } else if candidate.len() == 32 {
        decode_base32_btih(candidate)
    } else {
        None
    }
}

/// Builds qBittorrent comma-separated tags for a Stremio media ID.
pub fn build_torrent_tags(stremio_id: &str) -> String {
    if let Some((norm_stremio_id, imdb_id, media_type)) = parse_stremio_media_id(stremio_id) {
        return if media_type == "series" {
            format!("{},series,{}", imdb_id, norm_stremio_id)
        } else {
            format!("{},movie", imdb_id)
        };
    }
    base_imdb_id(stremio_id).trim().to_lowercase()
}

/// Parses a qBittorrent comma-separated `tags` string into
/// `(base_imdb_id, optional_media_type, latest_episode_id)`.
pub fn parse_torrent_tags(tags: &str) -> Option<(String, Option<String>, Option<String>)> {
    let mut base_imdb_id: Option<String> = None;
    let mut fallback_base: Option<String> = None;
    let mut has_series_marker = false;
    let mut has_movie_marker = false;
    let mut latest_episode_id: Option<String> = None;

    for tag in tags
        .split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
    {
        match tag.as_str() {
            "series" => has_series_marker = true,
            "movie" => has_movie_marker = true,
            _ if tag.starts_with("tt") => {
                if let Some((norm_ep, base, media_type)) = parse_stremio_media_id(&tag) {
                    if media_type == "series" {
                        has_series_marker = true;
                        latest_episode_id = Some(norm_ep);
                        fallback_base.get_or_insert(base);
                    } else {
                        base_imdb_id.get_or_insert(base);
                    }
                } else if !tag.contains(':') {
                    base_imdb_id.get_or_insert(tag);
                } else if let Some(prefix) = tag.split(':').next().filter(|p| p.starts_with("tt")) {
                    fallback_base.get_or_insert_with(|| prefix.to_string());
                }
            }
            _ => {}
        }
    }

    let imdb_id = base_imdb_id.or(fallback_base)?;
    let media_type = if has_series_marker || latest_episode_id.is_some() {
        Some("series".to_string())
    } else if has_movie_marker {
        Some("movie".to_string())
    } else {
        None
    };

    Some((imdb_id, media_type, latest_episode_id))
}

/// Checks whether a comma-separated tag list contains `target` case-insensitively.
pub fn torrent_has_tag(tags: &str, target: &str) -> bool {
    let target = target.trim();
    !target.is_empty()
        && tags
            .split(',')
            .map(str::trim)
            .any(|item| item.eq_ignore_ascii_case(target))
}

// ── Bencode & Torrent InfoHash ──────────────────────────────────────────────

const MAX_BENCODE_DEPTH: usize = 64;

fn parse_bencode_bytes(data: &[u8], pos: usize) -> Option<(&[u8], usize)> {
    let slice = data.get(pos..)?;
    let colon_rel = slice.iter().position(|&b| b == b':')?;
    if colon_rel == 0 {
        return None;
    }
    let len_str = std::str::from_utf8(&slice[..colon_rel]).ok()?;
    if !len_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let len: usize = len_str.parse().ok()?;
    let start = pos.checked_add(colon_rel + 1)?;
    let end = start.checked_add(len)?;
    Some((data.get(start..end)?, end))
}

fn skip_bencode_value(data: &[u8], pos: usize, depth: usize) -> Option<usize> {
    if depth >= MAX_BENCODE_DEPTH {
        return None;
    }
    match *data.get(pos)? {
        b'i' => {
            let mut cur = pos + 1 + usize::from(data.get(pos + 1) == Some(&b'-'));
            let digits_start = cur;
            while let Some(&b) = data.get(cur) {
                if b == b'e' {
                    return (cur > digits_start).then_some(cur + 1);
                }
                if !b.is_ascii_digit() {
                    return None;
                }
                cur += 1;
            }
            None
        }
        b'l' => {
            let mut cur = pos + 1;
            while *data.get(cur)? != b'e' {
                cur = skip_bencode_value(data, cur, depth + 1)?;
            }
            Some(cur + 1)
        }
        b'd' => {
            let mut cur = pos + 1;
            while *data.get(cur)? != b'e' {
                let (_, next) = parse_bencode_bytes(data, cur)?;
                cur = skip_bencode_value(data, next, depth + 1)?;
            }
            Some(cur + 1)
        }
        b'0'..=b'9' => Some(parse_bencode_bytes(data, pos)?.1),
        _ => None,
    }
}

fn sha1_hex(data: &[u8]) -> String {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity((data.len() + 9).div_ceil(64) * 64);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (slot, &chunk) in w[..16].iter_mut().zip(block.as_chunks::<4>().0) {
            *slot = u32::from_be_bytes(chunk);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        for (slot, delta) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(delta);
        }
    }

    let mut out = String::with_capacity(40);
    for word in h {
        let _ = write!(&mut out, "{word:08x}");
    }
    out
}

/// Computes the 40-character lowercase hex BitTorrent v1 `info_hash` (SHA-1 of the raw
/// bencoded `info` dictionary) from `.torrent` file bytes.
pub fn compute_torrent_info_hash(torrent_bytes: &[u8]) -> Option<String> {
    if torrent_bytes.first() != Some(&b'd') {
        return None;
    }

    let mut pos = 1;
    while *torrent_bytes.get(pos)? != b'e' {
        let (key, val_start) = parse_bencode_bytes(torrent_bytes, pos)?;
        let val_end = skip_bencode_value(torrent_bytes, val_start, 0)?;
        if key == b"info" {
            return Some(sha1_hex(&torrent_bytes[val_start..val_end]));
        }
        pos = val_end;
    }

    None
}

// ── Playback Readiness & Safe Paths ─────────────────────────────────────────

const MIN_READY_BYTES: u64 = 20 * 1024 * 1024;
const CAP_READY_BYTES: u64 = 64 * 1024 * 1024;
const MIN_READY_PROGRESS: f64 = 0.03;
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
    dlspeed == 0
        && matches!(
            torrent_state.trim(),
            "stalledDL" | "metaDL" | "error" | "missingFiles"
        )
}

/// Resolves a torrent-relative file path inside `download_root` while rejecting path traversal.
pub fn resolve_safe_download_path(download_root: &str, relative: &str) -> Option<PathBuf> {
    if relative.is_empty() {
        return None;
    }

    let mut safe_rel = PathBuf::new();
    for comp in StdPath::new(relative).components() {
        match comp {
            Component::Normal(part) => safe_rel.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    (!safe_rel.as_os_str().is_empty()).then(|| StdPath::new(download_root).join(safe_rel))
}

/// Returns true if the resolved download file exists on disk and is non-empty.
pub fn file_exists_nonempty(download_root: &str, file_path: &str) -> bool {
    resolve_safe_download_path(download_root, file_path).is_some_and(|p| {
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
    })
}

/// Finds a target torrent file matching either `file_idx` or non-empty `file_path`.
pub fn find_target_file<'a>(
    files: &'a [QbitFile],
    file_idx: Option<usize>,
    file_path: &str,
) -> Option<(usize, &'a QbitFile)> {
    files.iter().enumerate().find(|(i, f)| {
        file_idx.is_some_and(|idx| f.index.unwrap_or(*i) == idx)
            || (!file_path.is_empty() && f.name == file_path)
    })
}

/// Finds the best matching video file index inside a qBittorrent file list.
pub fn match_best_qbit_file(
    files: &[QbitFile],
    expected_title: &str,
    is_series: bool,
    season: u32,
    episode: u32,
) -> Option<usize> {
    let file_infos: Vec<(&str, u64)> = files.iter().map(|f| (f.name.as_str(), f.size)).collect();
    matcher::find_best_file_match(&file_infos, expected_title, is_series, season, episode)
}

/// Maps live qBittorrent file progress, stall status, and disk presence to a placeholder state.
pub fn decide_live_file_state(
    exists_on_disk: bool,
    file_size: u64,
    file_progress: f64,
    torrent_stalled: bool,
) -> PlaceholderState {
    if is_file_stream_ready(file_size, file_progress) {
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
    let live_state =
        decide_live_file_state(exists_on_disk, file_size, file_progress, torrent_stalled);
    if live_state == PlaceholderState::Ready {
        return None;
    }

    let is_unrequested =
        file_priority == Some(0) || (file_priority.is_none() && was_unrequested_param);
    if is_unrequested && !already_requested_in_session {
        return Some(PlaceholderState::EpisodeRequested);
    }

    Some(live_state)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextEpisodePrefetch {
    pub file_id: usize,
    pub file_path: String,
    pub season: u32,
    pub episode: u32,
}

/// Selects the next unrequested episode file in a season-pack torrent to prefetch.
pub fn select_next_episode_to_prefetch(
    stremio_id: &str,
    torrent_name: Option<&str>,
    files: &[QbitFile],
    current_file_vec_idx: usize,
) -> Option<NextEpisodePrefetch> {
    let (_, season, episode) = parse_series_episode_id(stremio_id)?;
    let file_infos: Vec<(&str, u64)> = files.iter().map(|f| (f.name.as_str(), f.size)).collect();
    let next_match = find_next_episode_file(
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

// ── Retention Policy ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionSource {
    LastWatched(DateTime<Utc>),
    AddedOn(DateTime<Utc>),
}

/// Resolves the expiration timestamp and its baseline source (`last_watched_at` or `added_on`).
pub fn resolve_retention_expiry(
    retention_days: u64,
    last_watched_at: Option<&str>,
    added_on: u64,
) -> Option<(DateTime<Utc>, RetentionSource)> {
    if retention_days == 0 {
        return None;
    }
    let retention_secs = retention_days as i64 * 86_400;

    if let Some(last) = last_watched_at
        && let Ok(parsed) = NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S")
    {
        let base_dt = DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc);
        return Some((
            base_dt + Duration::seconds(retention_secs),
            RetentionSource::LastWatched(base_dt),
        ));
    }

    if added_on > 0
        && let Some(base_dt) = DateTime::<Utc>::from_timestamp(added_on as i64, 0)
        && let Some(expires_dt) =
            DateTime::<Utc>::from_timestamp(added_on as i64 + retention_secs, 0)
    {
        return Some((expires_dt, RetentionSource::AddedOn(base_dt)));
    }

    None
}

/// Computes the ISO-8601 UTC expiration timestamp string for admin API responses.
pub fn compute_expires_at(
    retention_days: u64,
    last_watched_at: Option<&str>,
    added_on: u64,
) -> Option<String> {
    resolve_retention_expiry(retention_days, last_watched_at, added_on)
        .map(|(dt, _)| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// Returns a human-readable deletion reason if the torrent has exceeded `retention_days`.
pub fn should_delete_torrent(
    hash: &str,
    added_on: u64,
    last_watched: Option<&str>,
    retention_days: u64,
    now_utc: DateTime<Utc>,
) -> Option<String> {
    let (_, source) = resolve_retention_expiry(retention_days, last_watched, added_on)?;
    let retention_secs = retention_days as i64 * 86_400;

    match source {
        RetentionSource::LastWatched(base_dt) => {
            let duration = now_utc.signed_duration_since(base_dt);
            (duration.num_seconds() >= retention_secs).then(|| {
                format!(
                    "Torrent {} is stale (last watched {} days ago). Scheduling deletion.",
                    hash,
                    duration.num_days()
                )
            })
        }
        RetentionSource::AddedOn(base_dt) => {
            let duration_seconds = now_utc.timestamp() - base_dt.timestamp();
            (duration_seconds >= retention_secs).then(|| {
                format!(
                    "Torrent {} has no watch history and was added {} days ago. Scheduling deletion.",
                    hash,
                    duration_seconds / 86_400
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn parses_stremio_media_ids() {
        assert_eq!(
            parse_stremio_media_id(" TT1375666 "),
            Some((
                "tt1375666".to_string(),
                "tt1375666".to_string(),
                "movie".to_string()
            ))
        );
        assert_eq!(
            parse_stremio_media_id("tt0903747:1:2"),
            Some((
                "tt0903747:1:2".to_string(),
                "tt0903747".to_string(),
                "series".to_string()
            ))
        );
        assert_eq!(parse_stremio_media_id(""), None);
        assert_eq!(parse_stremio_media_id("tt0903747:1"), None);
        assert_eq!(parse_stremio_media_id("tt0903747:one:two"), None);
    }

    #[test]
    fn extracts_btih_hex_and_base32_and_rejects_invalid() {
        let hex_magnet = "magnet:?xt=URN:BTIH:BA0B39DE2C19CEADCFB1B441D8B6D668E57458EF&dn=Avatar";
        assert_eq!(
            extract_btih_hex(hex_magnet).as_deref(),
            Some("ba0b39de2c19ceadcfb1b441d8b6d668e57458ef")
        );

        let b32_magnet = "magnet:?xt=urn:btih:CWWVGHCJO23WHPWLZ7ZHDLR7KD4NXPQV&dn=Test";
        let hex = extract_btih_hex(b32_magnet).unwrap();
        assert_eq!(hex.len(), 40);
        assert_eq!(hex, "15ad531c4976b763becbcff271ae3f50f8dbbe15");

        assert_eq!(extract_btih_hex("https://example.com/file.torrent"), None);
        assert_eq!(extract_btih_hex("magnet:?xt=urn:btih:tooshort"), None);
        assert_eq!(
            extract_btih_hex("magnet:?xt=urn:btih:11111111111111111111111111111111"),
            None
        );
    }

    #[test]
    fn builds_parses_and_matches_torrent_tags() {
        assert_eq!(build_torrent_tags("TT1375666"), "tt1375666,movie");
        assert_eq!(
            build_torrent_tags("tt0903747:1:2"),
            "tt0903747,series,tt0903747:1:2"
        );

        assert_eq!(
            parse_torrent_tags("tt1375666, movie"),
            Some(("tt1375666".to_string(), Some("movie".to_string()), None))
        );
        assert_eq!(
            parse_torrent_tags("tt0903747, series, tt0903747:1:1, tt0903747:1:2"),
            Some((
                "tt0903747".to_string(),
                Some("series".to_string()),
                Some("tt0903747:1:2".to_string())
            ))
        );
        assert_eq!(
            parse_torrent_tags("tt0903747"),
            Some(("tt0903747".to_string(), None, None))
        );
        assert_eq!(
            parse_torrent_tags("tt0903747:2:5"),
            Some((
                "tt0903747".to_string(),
                Some("series".to_string()),
                Some("tt0903747:2:5".to_string())
            ))
        );
        assert_eq!(parse_torrent_tags("manual, 1080p"), None);

        assert!(torrent_has_tag("tt1234, movie", "TT1234"));
        assert!(torrent_has_tag(
            "tt0903747, series, tt0903747:1:2",
            "tt0903747"
        ));
        assert!(!torrent_has_tag("tt12345, movie", "tt1234"));
        assert!(!torrent_has_tag("", "tt1234"));
    }

    #[test]
    fn sha1_and_bencode_info_hash() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");

        let info_dict = b"d6:lengthi1234e4:name8:test.mkv12:piece lengthi16384e6:pieces20:01234567890123456789e";
        let mut torrent = b"d8:announce23:http://tracker/announce4:info".to_vec();
        torrent.extend_from_slice(info_dict);
        torrent.push(b'e');
        assert_eq!(
            compute_torrent_info_hash(&torrent),
            Some(sha1_hex(info_dict))
        );

        let multi_info = b"d5:filesld6:lengthi100e4:pathl4:ep01eed6:lengthi200e4:pathl4:ep02eee4:name4:Show12:piece lengthi16384e6:pieces20:abcdefghijklmnopqrste";
        let mut multi_torrent = b"d7:comment5:hello4:info".to_vec();
        multi_torrent.extend_from_slice(multi_info);
        multi_torrent.extend_from_slice(b"7:privatei0ee");
        assert_eq!(
            compute_torrent_info_hash(&multi_torrent),
            Some(sha1_hex(multi_info))
        );
        assert_eq!(compute_torrent_info_hash(b"not-bencode"), None);
        assert_eq!(compute_torrent_info_hash(b"d4:name3:abce"), None);
    }

    #[test]
    fn stream_readiness_and_stalled_detection() {
        assert!(!is_file_stream_ready(GB, 0.0));
        assert!(is_file_stream_ready(GB, 1.0));
        assert!(!is_file_stream_ready(10 * MB, 0.05));
        assert!(is_file_stream_ready(10 * MB, 0.25));
        assert!(!is_file_stream_ready(GB, 0.01));
        assert!(!is_file_stream_ready(GB, 0.025));
        assert!(is_file_stream_ready(GB, 0.03));
        assert!(!is_file_stream_ready(20 * GB, 0.001));
        assert!(is_file_stream_ready(20 * GB, 0.012));

        assert!(is_torrent_stalled("stalledDL", 0));
        assert!(is_torrent_stalled("metaDL", 0));
        assert!(is_torrent_stalled("error", 0));
        assert!(is_torrent_stalled("missingFiles", 0));
        assert!(!is_torrent_stalled("downloading", 0));
        assert!(!is_torrent_stalled("stalledDL", 1024));
        assert!(!is_torrent_stalled("uploading", 0));
    }

    #[test]
    fn placeholder_and_live_file_decisions_cover_all_states() {
        assert_eq!(
            decide_live_file_state(true, GB, 0.035, false),
            PlaceholderState::Ready
        );
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
        assert_eq!(
            decide_live_file_state(true, 100 * MB, 0.12, false),
            PlaceholderState::Buffering(9)
        );
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
        assert_eq!(
            decide_live_file_state(true, 20 * GB, 0.012, false),
            PlaceholderState::Ready
        );

        assert_eq!(
            decide_play_placeholder(false, Some(0), GB, 0.0, true, false, false),
            Some(PlaceholderState::EpisodeRequested)
        );
        assert_eq!(
            decide_play_placeholder(false, Some(0), GB, 0.0, true, true, false),
            Some(PlaceholderState::Buffering(0))
        );
        assert_eq!(
            decide_play_placeholder(false, Some(0), GB, 0.0, true, true, true),
            Some(PlaceholderState::Stalled)
        );
        assert_eq!(
            decide_play_placeholder(true, Some(1), GB, 0.01, false, false, false),
            Some(PlaceholderState::Buffering(1))
        );
        assert_eq!(
            decide_play_placeholder(true, Some(1), GB, 0.01, false, false, true),
            Some(PlaceholderState::Stalled)
        );
        assert_eq!(
            decide_play_placeholder(true, Some(1), GB, 0.05, false, false, false),
            None
        );
        assert_eq!(
            decide_play_placeholder(false, Some(1), GB, 0.80, false, false, false),
            Some(PlaceholderState::FilesMissing)
        );
        assert_eq!(
            decide_play_placeholder(false, Some(1), GB, 0.05, false, false, false),
            Some(PlaceholderState::FilesMissing)
        );
        assert_eq!(
            decide_play_placeholder(false, Some(1), GB, 0.01, false, false, false),
            Some(PlaceholderState::Buffering(1))
        );
        assert_eq!(
            decide_play_placeholder(true, Some(1), 100 * MB, 0.12, false, false, false),
            Some(PlaceholderState::Buffering(9))
        );
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

    #[test]
    fn retention_policy_handles_watch_history_added_on_and_malformed_dates() {
        let utc = |y, m, d, hh, mm, ss| Utc.with_ymd_and_hms(y, m, d, hh, mm, ss).unwrap();
        let now = utc(2026, 11, 13, 23, 0, 0);
        let jan_added = utc(2026, 1, 1, 0, 0, 0).timestamp() as u64;
        let nov1_added = utc(2026, 11, 1, 0, 0, 0).timestamp() as u64;
        let nov10_added = utc(2026, 11, 10, 0, 0, 0).timestamp() as u64;
        let nov12_added = utc(2026, 11, 12, 0, 0, 0).timestamp() as u64;

        assert!(
            should_delete_torrent("abc123", jan_added, Some("2026-11-10 12:00:00"), 7, now)
                .is_none()
        );
        assert!(
            should_delete_torrent("abc123", jan_added, Some("2026-11-01 12:00:00"), 7, now)
                .is_some()
        );
        assert!(should_delete_torrent("abc123", nov1_added, None, 7, now).is_some());
        assert!(should_delete_torrent("abc123", nov10_added, None, 7, now).is_none());
        assert!(
            should_delete_torrent("abc123", nov1_added, Some("not-a-timestamp"), 7, now).is_some()
        );
        assert!(
            should_delete_torrent("abc123", nov12_added, Some("not-a-timestamp"), 7, now).is_none()
        );
    }
}
