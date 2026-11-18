use super::{AppState, require_valid_api_key};
use crate::cinemeta::{
    metahub_background_url, metahub_logo_url, metahub_poster_url, resolve_media_metadata,
};
use crate::db::{DbClient, MediaHistoryEntry, MediaMetadata};
use crate::models::{CatalogResponse, MetaPreview};
use crate::parser::looks_like_series_name;
use crate::qbittorrent::{QbitTorrent, parse_torrent_tags};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json, Response},
};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const CATALOG_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogKind {
    User,
    Community,
}

fn strip_json_suffix(segment: &str) -> &str {
    segment.strip_suffix(".json").unwrap_or(segment)
}

pub fn parse_skip_extra(extra: &str) -> usize {
    let clean = strip_json_suffix(extra.trim());
    for pair in clean.split('&') {
        if let Some((k, v)) = pair.split_once('=')
            && k.trim().eq_ignore_ascii_case("skip")
            && let Ok(val) = v.trim().parse::<usize>()
        {
            return val;
        }
    }
    0
}

fn resolve_catalog_target(
    media_type: &str,
    catalog_id: &str,
) -> Option<(CatalogKind, &'static str)> {
    match (media_type, catalog_id) {
        ("movie", "mms_user_movies") => Some((CatalogKind::User, "movie")),
        ("series", "mms_user_series") => Some((CatalogKind::User, "series")),
        ("movie", "mms_community_movies") => Some((CatalogKind::Community, "movie")),
        ("series", "mms_community_series") => Some((CatalogKind::Community, "series")),
        _ => None,
    }
}

fn format_episode_badge(stremio_id: &str) -> Option<String> {
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

pub fn build_meta_preview(entry: &MediaHistoryEntry, meta: Option<MediaMetadata>) -> MetaPreview {
    let (name, poster, background, logo, mut description, mut release_info, imdb_rating) =
        match meta {
            Some(m) => (
                m.name,
                m.poster
                    .or_else(|| Some(metahub_poster_url(&entry.imdb_id))),
                m.background
                    .or_else(|| Some(metahub_background_url(&entry.imdb_id))),
                m.logo.or_else(|| Some(metahub_logo_url(&entry.imdb_id))),
                m.description,
                m.release_info,
                m.imdb_rating,
            ),
            None => (
                entry.imdb_id.clone(),
                Some(metahub_poster_url(&entry.imdb_id)),
                Some(metahub_background_url(&entry.imdb_id)),
                Some(metahub_logo_url(&entry.imdb_id)),
                None,
                None,
                None,
            ),
        };

    if entry.media_type == "series"
        && let Some(ep_badge) = format_episode_badge(&entry.latest_stremio_id)
    {
        release_info = match release_info
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(ri) => Some(format!("{} • Last: {}", ri, ep_badge)),
            None => Some(format!("Last: {}", ep_badge)),
        };
        description = match description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(desc) => Some(format!("Last streamed: {}\n\n{}", ep_badge, desc)),
            None => Some(format!("Last streamed: {}", ep_badge)),
        };
    }

    MetaPreview {
        id: entry.imdb_id.clone(),
        media_type: entry.media_type.clone(),
        name,
        poster,
        background,
        logo,
        description,
        release_info,
        imdb_rating,
        behavior_hints: None,
    }
}

fn torrent_timestamp(torrent: &QbitTorrent, watch_times: &HashMap<String, String>) -> String {
    let hash_lower = torrent.hash.trim().to_lowercase();
    if let Some(ts) = watch_times.get(&hash_lower) {
        return ts.clone();
    }
    if torrent.added_on > 0
        && let Some(dt) = DateTime::<Utc>::from_timestamp(torrent.added_on as i64, 0)
    {
        return dt.format("%Y-%m-%d %H:%M:%S").to_string();
    }
    "1970-01-01 00:00:00".to_string()
}

/// Merges database community history with active qBittorrent torrents reverse-looked-up via tags.
pub fn merge_community_entries(
    db: &DbClient,
    media_type: &str,
    db_entries: Vec<MediaHistoryEntry>,
    torrents: &[QbitTorrent],
    watch_times: &HashMap<String, String>,
) -> Vec<MediaHistoryEntry> {
    let mut seen_imdb: HashSet<String> = db_entries
        .iter()
        .map(|e| e.imdb_id.to_lowercase())
        .collect();

    let mut tagged_extras: HashMap<String, MediaHistoryEntry> = HashMap::new();

    for torrent in torrents {
        let Some((imdb_id, tag_type, latest_episode)) = parse_torrent_tags(&torrent.tags) else {
            continue;
        };

        if seen_imdb.contains(&imdb_id) {
            continue;
        }

        let resolved_type = tag_type
            .or_else(|| {
                db.get_media_metadata(&imdb_id)
                    .ok()
                    .flatten()
                    .map(|m| m.media_type)
            })
            .unwrap_or_else(|| {
                if looks_like_series_name(&torrent.name) {
                    "series".to_string()
                } else {
                    "movie".to_string()
                }
            });

        if resolved_type != media_type {
            continue;
        }

        let timestamp = torrent_timestamp(torrent, watch_times);
        let latest_stremio_id = latest_episode.unwrap_or_else(|| imdb_id.clone());
        let candidate = MediaHistoryEntry {
            imdb_id: imdb_id.clone(),
            media_type: resolved_type,
            latest_stremio_id,
            info_hash: torrent.hash.trim().to_lowercase(),
            last_watched_at: timestamp,
        };

        match tagged_extras.get(&imdb_id) {
            Some(existing) if existing.last_watched_at >= candidate.last_watched_at => {}
            _ => {
                tagged_extras.insert(imdb_id, candidate);
            }
        }
    }

    let mut merged: Vec<(usize, MediaHistoryEntry)> = db_entries.into_iter().enumerate().collect();
    for entry in tagged_extras.into_values() {
        seen_imdb.insert(entry.imdb_id.clone());
        merged.push((usize::MAX, entry));
    }

    merged.sort_by(|(idx_a, a), (idx_b, b)| {
        b.last_watched_at
            .cmp(&a.last_watched_at)
            .then_with(|| idx_a.cmp(idx_b))
    });

    merged.into_iter().map(|(_, entry)| entry).collect()
}

pub async fn catalog_handler(
    State(state): State<AppState>,
    Path((api_key, media_type, catalog_id_ext)): Path<(String, String, String)>,
) -> Response {
    serve_catalog(state, api_key, media_type, catalog_id_ext, 0).await
}

pub async fn catalog_with_extra_handler(
    State(state): State<AppState>,
    Path((api_key, media_type, catalog_id, extra_ext)): Path<(String, String, String, String)>,
) -> Response {
    let skip = parse_skip_extra(&extra_ext);
    serve_catalog(state, api_key, media_type, catalog_id, skip).await
}

async fn serve_catalog(
    state: AppState,
    api_key: String,
    media_type: String,
    catalog_id_ext: String,
    skip: usize,
) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    let catalog_id = strip_json_suffix(&catalog_id_ext);
    let Some((kind, target_type)) = resolve_catalog_target(&media_type, catalog_id) else {
        return Json(CatalogResponse { metas: vec![] }).into_response();
    };

    let entries = match kind {
        CatalogKind::User => state
            .db
            .get_user_catalog_entries(&api_key, target_type)
            .unwrap_or_default(),
        CatalogKind::Community => {
            let db_entries = state
                .db
                .get_community_catalog_entries(target_type)
                .unwrap_or_default();
            let qbit_torrents =
                tokio::time::timeout(Duration::from_millis(1500), state.qbit.get_all_torrents())
                    .await
                    .unwrap_or_default();
            let watch_times = state.db.get_torrent_watch_times().unwrap_or_default();
            merge_community_entries(
                &state.db,
                target_type,
                db_entries,
                &qbit_torrents,
                &watch_times,
            )
        }
    };

    let page_entries: Vec<MediaHistoryEntry> = entries
        .into_iter()
        .skip(skip)
        .take(CATALOG_PAGE_SIZE)
        .collect();

    let mut handles = Vec::with_capacity(page_entries.len());
    for entry in page_entries {
        let http = state.http_client.clone();
        let db = state.db.clone();
        handles.push(tokio::spawn(async move {
            let meta = resolve_media_metadata(&http, &db, &entry.media_type, &entry.imdb_id).await;
            build_meta_preview(&entry, meta)
        }));
    }

    let mut metas = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(preview) = handle.await {
            metas.push(preview);
        }
    }

    Json(CatalogResponse { metas }).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(
        imdb_id: &str,
        media_type: &str,
        latest_stremio_id: &str,
        ts: &str,
    ) -> MediaHistoryEntry {
        MediaHistoryEntry {
            imdb_id: imdb_id.to_string(),
            media_type: media_type.to_string(),
            latest_stremio_id: latest_stremio_id.to_string(),
            info_hash: "0123456789abcdef0123456789abcdef01234567".to_string(),
            last_watched_at: ts.to_string(),
        }
    }

    #[test]
    fn parses_skip_extra_parameter_variants() {
        assert_eq!(parse_skip_extra("skip=20.json"), 20);
        assert_eq!(parse_skip_extra("genre=Action&skip=40.json"), 40);
        assert_eq!(parse_skip_extra("SKIP=15"), 15);
        assert_eq!(parse_skip_extra("genre=Action.json"), 0);
        assert_eq!(parse_skip_extra("skip=not-a-number.json"), 0);
        assert_eq!(parse_skip_extra(""), 0);
    }

    #[test]
    fn builds_meta_preview_with_series_episode_badge_and_fallback() {
        let series_entry = sample_entry(
            "tt0903747",
            "series",
            "tt0903747:1:2",
            "2026-11-18 20:00:00",
        );
        let cached = MediaMetadata {
            imdb_id: "tt0903747".to_string(),
            media_type: "series".to_string(),
            name: "Breaking Bad".to_string(),
            poster: Some("https://example.com/bb-poster.jpg".to_string()),
            background: None,
            logo: None,
            description: Some("A chemistry teacher turns to crime.".to_string()),
            release_info: Some("2008-2013".to_string()),
            imdb_rating: Some("9.5".to_string()),
        };

        let preview = build_meta_preview(&series_entry, Some(cached));
        assert_eq!(preview.id, "tt0903747");
        assert_eq!(preview.media_type, "series");
        assert_eq!(preview.name, "Breaking Bad");
        assert_eq!(
            preview.release_info.as_deref(),
            Some("2008-2013 • Last: S01E02")
        );
        assert_eq!(
            preview.description.as_deref(),
            Some("Last streamed: S01E02\n\nA chemistry teacher turns to crime.")
        );
        assert_eq!(
            preview.background.as_deref(),
            Some("https://images.metahub.space/background/medium/tt0903747/img")
        );

        let movie_entry = sample_entry("tt1375666", "movie", "tt1375666", "2026-11-18 20:00:00");
        let fallback = build_meta_preview(&movie_entry, None);
        assert_eq!(fallback.id, "tt1375666");
        assert_eq!(fallback.name, "tt1375666");
        assert_eq!(
            fallback.poster.as_deref(),
            Some("https://images.metahub.space/poster/medium/tt1375666/img")
        );
        assert_eq!(fallback.release_info, None);
    }

    #[test]
    fn merges_community_entries_with_reverse_looked_up_qbit_torrents() {
        let db = DbClient::new(":memory:");
        let db_entries = vec![sample_entry(
            "tt1375666",
            "movie",
            "tt1375666",
            "2026-11-18 18:00:00",
        )];

        let torrents = vec![
            QbitTorrent {
                hash: "AAAA0000BBBB1111CCCC2222DDDD3333EEEE4444".to_string(),
                tags: "tt0816692, movie".to_string(),
                name: "Interstellar.2014.1080p.mkv".to_string(),
                size: 0,
                progress: 1.0,
                state: "uploading".to_string(),
                added_on: 1_795_028_400, // 2026-11-18T19:00:00Z
                dlspeed: 0,
                upspeed: 0,
                eta: 0,
            },
            QbitTorrent {
                hash: "BBBB0000CCCC1111DDDD2222EEEE3333FFFF4444".to_string(),
                tags: "tt0903747, series, tt0903747:1:3".to_string(),
                name: "Breaking.Bad.S01.1080p".to_string(),
                size: 0,
                progress: 1.0,
                state: "uploading".to_string(),
                added_on: 1_795_032_000, // 2026-11-18T20:00:00Z
                dlspeed: 0,
                upspeed: 0,
                eta: 0,
            },
            // Legacy torrent with bare IMDb tag and series folder name -> detected as series
            QbitTorrent {
                hash: "CCCC0000DDDD1111EEEE2222FFFF3333AAAA4444".to_string(),
                tags: "tt0944947".to_string(),
                name: "Game.of.Thrones.S02.COMPLETE.1080p".to_string(),
                size: 0,
                progress: 1.0,
                state: "uploading".to_string(),
                added_on: 1_795_024_800,
                dlspeed: 0,
                upspeed: 0,
                eta: 0,
            },
        ];

        let mut watch_times = HashMap::new();
        watch_times.insert(
            "aaaa0000bbbb1111cccc2222dddd3333eeee4444".to_string(),
            "2026-11-18 21:00:00".to_string(),
        );

        let movies = merge_community_entries(&db, "movie", db_entries, &torrents, &watch_times);
        assert_eq!(movies.len(), 2);
        assert_eq!(movies[0].imdb_id, "tt0816692");
        assert_eq!(movies[1].imdb_id, "tt1375666");

        let series = merge_community_entries(&db, "series", vec![], &torrents, &watch_times);
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].imdb_id, "tt0903747");
        assert_eq!(series[0].latest_stremio_id, "tt0903747:1:3");
        assert_eq!(series[1].imdb_id, "tt0944947");
    }
}
