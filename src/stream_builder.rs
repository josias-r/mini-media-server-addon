use crate::cinemeta::get_cinemeta_title;
use crate::jackett::{JackettResult, TorznabParams, fetch_jackett_results};
use crate::matcher::find_best_file_match;
use crate::models::{Stream, StreamResponse};
use crate::parser::{is_definitely_not_full_season, is_video_file};
use crate::routes::AppState;

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
        let plan_results = fetch_jackett_results(&state.http_client, &state.config, plan).await;
        for r in plan_results {
            let key = r
                .info_hash
                .clone()
                .unwrap_or_else(|| r.magnet_uri.clone().unwrap_or_else(|| r.title.clone()))
                .to_lowercase();
            if !seen.contains(&key) {
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
    }
    results
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

    let base_id = stremio_id.split(':').next().unwrap_or(stremio_id);
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

        let file_infos: Vec<(&str, u64)> =
            files.iter().map(|f| (f.name.as_str(), f.size)).collect();
        let best_idx =
            find_best_file_match(&file_infos, expected_title, is_series, season, episode);

        for (idx, file) in files.iter().enumerate() {
            if is_video_file(&file.name) {
                if let Some(f) = &filter_fn
                    && f(&file.name)
                {
                    continue;
                }

                let is_best = Some(idx) == best_idx;
                let category = if is_best { 1 } else { 2 };
                let size_gb = file.size as f64 / 1024.0 / 1024.0 / 1024.0;
                let progress_pct = (file.progress * 100.0).round();

                let name_str = match (is_best, file.priority == 0) {
                    (_, true) => "Available\n⏸️ Not yet requested",
                    (true, false) => "Available (Best Match)",
                    (false, false) => "Available",
                };

                let title_str = if file.priority == 0 {
                    format!("{}\n💾 {:.2} GB", file.name, size_gb)
                } else {
                    format!("{}\n📥 {}% | 💾 {:.2} GB", file.name, progress_pct, size_gb)
                };

                stream_items.push(StreamItem {
                    stream: Stream {
                        name: Some(name_str.to_string()),
                        title: Some(title_str),
                        url: Some(format!(
                            "{}/{}/play-file/{}/{}?fileIdx={}&filePath={}",
                            state.config.public_url,
                            api_key,
                            urlencoding::encode(stremio_id),
                            torrent.hash,
                            file.index.unwrap_or(idx),
                            urlencoding::encode(&file.name)
                        )),
                        description: None,
                        yt_id: None,
                        info_hash: None,
                        file_idx: None,
                        external_url: None,
                    },
                    category,
                    seeders: 0,
                });
            }
        }
    }

    for r in results {
        let magnet_or_url = r.magnet_uri.clone().or_else(|| r.link.clone());
        if magnet_or_url.is_none() {
            continue;
        }

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
        let size_gb = r.size as f64 / 1024.0 / 1024.0 / 1024.0;

        stream_items.push(StreamItem {
            stream: Stream {
                name: Some(format!("Qbit - {}", r.tracker)),
                title: Some(format!(
                    "{}{}\n👤 {} | 💾 {:.2} GB",
                    prefix, r.title, r.seeders, size_gb
                )),
                url: Some(format!(
                    "{}/{}/trigger-download/{}/{}?magnet={}",
                    state.config.public_url,
                    api_key,
                    urlencoding::encode(stremio_id),
                    info_hash,
                    urlencoding::encode(&magnet_or_url.unwrap())
                )),
                description: None,
                yt_id: None,
                info_hash: None,
                file_idx: None,
                external_url: None,
            },
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
    let mut expected_title = stremio_id.to_string();
    let is_series = stremio_id.contains(':');
    let mut season = 1;
    let mut episode = 1;

    if is_series {
        let parts: Vec<&str> = stremio_id.split(':').collect();
        let series_id = parts[0].to_string();
        season = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
        episode = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);

        if let Some(t) = get_cinemeta_title(&state.http_client, "series", &series_id).await {
            expected_title = format!("{} S{:02}E{:02}", t, season, episode);
        }
    } else if let Some(t) = get_cinemeta_title(&state.http_client, "movie", stremio_id).await {
        expected_title = t;
    }

    (is_series, season, episode, expected_title)
}
