use crate::cinemeta::get_or_fetch_cinemeta_title;
use crate::config::AppState;
use crate::domain::parser::{
    is_definitely_not_full_season, is_definitely_wrong_episode, is_video_file,
};
use crate::domain::{base_imdb_id, match_best_qbit_file};
use crate::jackett::{JackettResult, TorznabParams, fetch_jackett_results};
use crate::models::{Stream, StreamResponse};

use std::collections::HashSet;
use std::sync::Arc;

pub type FilterFn = Arc<dyn Fn(&str) -> bool + Send + Sync>;

pub struct StreamItem {
    pub stream: Stream,
    pub category: u8,
    pub seeders: u32,
}

pub async fn fetch_and_dedup_jackett_results(
    state: &AppState,
    fetch_plans: &[TorznabParams],
    filter_fn: &Option<FilterFn>,
) -> Vec<JackettResult> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    for plan in fetch_plans {
        for r in fetch_jackett_results(&state.http_client, &state.config, plan).await {
            let key = r
                .info_hash
                .clone()
                .unwrap_or_else(|| r.magnet_uri.clone().unwrap_or_else(|| r.title.clone()))
                .to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            let include = match (plan, filter_fn) {
                (TorznabParams::SeriesSeasonImdb { season, .. }, _)
                | (TorznabParams::SeriesSeasonText { season, .. }, _) => {
                    !is_definitely_not_full_season(&r.title, *season)
                }
                (_, Some(f)) => !f(&r.title),
                _ => true,
            };
            if include {
                seen.insert(key);
                results.push(r);
            }
        }
    }
    results
}

async fn resolve_media_query(
    state: &AppState,
    stremio_id: &str,
    is_series: bool,
) -> (u32, u32, Option<String>, String) {
    if is_series {
        let mut parts = stremio_id.split(':');
        let series_id = parts.next().unwrap_or(stremio_id);
        let season: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
        let episode: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
        let title =
            get_or_fetch_cinemeta_title(&state.http_client, &state.db, "series", series_id).await;
        let expected_title = title
            .as_ref()
            .map(|t| format!("{} S{:02}E{:02}", t, season, episode))
            .unwrap_or_else(|| stremio_id.to_string());
        (season, episode, title, expected_title)
    } else {
        let title =
            get_or_fetch_cinemeta_title(&state.http_client, &state.db, "movie", stremio_id).await;
        let expected_title = title.clone().unwrap_or_else(|| stremio_id.to_string());
        (1, 1, title, expected_title)
    }
}

/// Builds the Stremio `StreamResponse` for either a movie or a series episode.
pub async fn build_media_streams(
    state: &AppState,
    api_key: &str,
    stremio_id: &str,
    is_series: bool,
) -> StreamResponse {
    let (season, episode, title, expected_title) =
        resolve_media_query(state, stremio_id, is_series).await;
    let base_id = base_imdb_id(stremio_id).to_string();
    let use_text = state.config.jackett_search_type == "text";

    let fetch_plans = match (is_series, use_text, title) {
        (true, true, Some(query)) => vec![
            TorznabParams::SeriesText {
                query: query.clone(),
                season,
                episode,
            },
            TorznabParams::SeriesSeasonText { query, season },
        ],
        (true, _, _) => vec![
            TorznabParams::SeriesImdb {
                imdb_id: base_id.clone(),
                season,
                episode,
            },
            TorznabParams::SeriesSeasonImdb {
                imdb_id: base_id,
                season,
            },
        ],
        (false, true, Some(query)) => vec![TorznabParams::MovieText { query }],
        (false, _, _) => vec![TorznabParams::MovieImdb { imdb_id: base_id }],
    };

    let filter_fn: Option<FilterFn> = is_series
        .then(|| Arc::new(move |t: &str| is_definitely_wrong_episode(t, season, episode)) as _);

    build_stream_response(
        state,
        api_key,
        stremio_id,
        is_series,
        season,
        episode,
        fetch_plans,
        filter_fn,
        &expected_title,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn build_stream_response(
    state: &AppState,
    api_key: &str,
    stremio_id: &str,
    is_series: bool,
    season: u32,
    episode: u32,
    fetch_plans: Vec<TorznabParams>,
    filter_fn: Option<FilterFn>,
    expected_title: &str,
) -> StreamResponse {
    let mut stream_items = Vec::new();
    let results = fetch_and_dedup_jackett_results(state, &fetch_plans, &filter_fn).await;

    let base_id = base_imdb_id(stremio_id);
    let tagged_torrents = state.qbit.get_torrents_by_tag(base_id).await;
    let downloaded_hashes: HashSet<String> = tagged_torrents
        .iter()
        .map(|t| t.hash.to_lowercase())
        .collect();
    let mut torrents_with_metadata = HashSet::new();

    for torrent in &tagged_torrents {
        let files = state.qbit.get_torrent_files(&torrent.hash).await;
        if !files.is_empty() {
            torrents_with_metadata.insert(torrent.hash.to_lowercase());
        }

        let best_idx = match_best_qbit_file(&files, expected_title, is_series, season, episode);

        for (idx, file) in files.iter().enumerate() {
            if !is_video_file(&file.name) || filter_fn.as_ref().is_some_and(|f| f(&file.name)) {
                continue;
            }

            let is_best = Some(idx) == best_idx;
            let size_gb = file.size as f64 / 1024.0 / 1024.0 / 1024.0;
            let name_str = match (is_best, file.priority == 0) {
                (_, true) => "Available\n⏸️ Not yet requested",
                (true, false) => "Available (Best Match)",
                (false, false) => "Available",
            };
            let title_str = if file.priority == 0 {
                format!("{}\n💾 {:.2} GB", file.name, size_gb)
            } else {
                format!(
                    "{}\n📥 {}% | 💾 {:.2} GB",
                    file.name,
                    (file.progress * 100.0).round(),
                    size_gb
                )
            };
            let unrequested_param = if file.priority == 0 {
                "&unrequested=1"
            } else {
                ""
            };
            let url = format!(
                "{}/{}/play-file/{}/{}?fileIdx={}&filePath={}{}",
                state.config.public_url,
                api_key,
                urlencoding::encode(stremio_id),
                torrent.hash,
                file.index.unwrap_or(idx),
                urlencoding::encode(&file.name),
                unrequested_param
            );

            stream_items.push(StreamItem {
                stream: Stream::from_url(name_str.to_string(), title_str, url),
                category: if is_best { 1 } else { 2 },
                seeders: 0,
            });
        }
    }

    for r in results {
        let Some(magnet_or_url) = r.magnet_uri.clone().or_else(|| r.link.clone()) else {
            continue;
        };
        let info_hash = r.info_hash.clone().unwrap_or_else(|| "unknown".to_string());
        if torrents_with_metadata.contains(&info_hash.to_lowercase()) {
            continue;
        }

        let is_downloading = downloaded_hashes.contains(&info_hash.to_lowercase());
        let prefix = if is_downloading {
            "[Downloading/Downloaded]\n"
        } else {
            ""
        };
        let existing_param = if is_downloading { "&existing=1" } else { "" };
        let size_gb = r.size as f64 / 1024.0 / 1024.0 / 1024.0;
        let url = format!(
            "{}/{}/trigger-download/{}/{}?magnet={}{}",
            state.config.public_url,
            api_key,
            urlencoding::encode(stremio_id),
            info_hash,
            urlencoding::encode(&magnet_or_url),
            existing_param
        );

        stream_items.push(StreamItem {
            stream: Stream::from_url(
                format!("Qbit - {}", r.tracker),
                format!(
                    "{}{}\n👤 {} | 💾 {:.2} GB",
                    prefix, r.title, r.seeders, size_gb
                ),
                url,
            ),
            category: 3,
            seeders: r.seeders,
        });
    }

    stream_items.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| b.seeders.cmp(&a.seeders))
    });

    StreamResponse {
        streams: stream_items.into_iter().map(|item| item.stream).collect(),
    }
}

pub async fn get_series_params(state: &AppState, stremio_id: &str) -> (bool, u32, u32, String) {
    let is_series = stremio_id.contains(':');
    let (season, episode, _, expected_title) =
        resolve_media_query(state, stremio_id, is_series).await;
    (is_series, season, episode, expected_title)
}
