use crate::cinemeta::get_or_fetch_cinemeta_title;
use crate::config::AppState;
use crate::domain::matcher::is_extra_or_sample_relative_to;
use crate::domain::parser::{
    is_definitely_not_full_season, is_definitely_wrong_episode, is_video_file,
};
use crate::domain::{base_imdb_id, match_best_qbit_file};
use crate::jackett::{JackettResult, TorznabParams, fetch_jackett_results};
use crate::models::{Stream, StreamResponse};

use futures_util::future::join_all;
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
    let plan_results = join_all(
        fetch_plans
            .iter()
            .map(|plan| fetch_jackett_results(&state.http_client, &state.config, plan)),
    )
    .await;

    let mut results = Vec::new();
    let mut seen = HashSet::new();

    for (plan, items) in fetch_plans.iter().zip(plan_results) {
        for r in items {
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
    let base_id = base_imdb_id(stremio_id);
    let (results, tagged_torrents) = tokio::join!(
        fetch_and_dedup_jackett_results(state, &fetch_plans, &filter_fn),
        state.qbit.get_torrents_by_tag(base_id),
    );
    let downloaded_hashes: HashSet<String> = tagged_torrents
        .iter()
        .map(|t| t.hash.to_lowercase())
        .collect();
    let mut torrents_with_metadata = HashSet::new();

    let torrent_files = join_all(
        tagged_torrents
            .iter()
            .map(|t| state.qbit.get_torrent_files(&t.hash)),
    )
    .await;

    for (torrent, files) in tagged_torrents.iter().zip(torrent_files) {
        if !files.is_empty() {
            torrents_with_metadata.insert(torrent.hash.to_lowercase());
        }

        let best_idx = match_best_qbit_file(&files, expected_title, is_series, season, episode);

        for (idx, file) in files.iter().enumerate() {
            if !is_video_file(&file.name) || filter_fn.as_ref().is_some_and(|f| f(&file.name)) {
                continue;
            }
            if let Some(b_idx) = best_idx
                && idx != b_idx
                && is_extra_or_sample_relative_to(
                    &file.name,
                    file.size,
                    &files[b_idx].name,
                    files[b_idx].size,
                )
            {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::DbClient;
    use crate::placeholder::LiveSessionStore;
    use crate::qbittorrent::QbitClient;
    use axum::{
        Router,
        extract::Query,
        http::{StatusCode, header},
        routing::{get, post},
    };
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn concurrent_jackett_and_qbit_queries_deduplicate_and_order_streams() {
        let qbit_hash = "aaaa1111bbbb2222cccc3333dddd4444eeee5555";
        let dup_hash = "bbbb1111cccc2222dddd3333eeee4444ffff5555";
        let pack_hash = "cccc1111dddd2222eeee3333ffff4444aaaa5555";

        let app = Router::new()
            .route(
                "/api/v2.0/indexers/all/results/torznab/api",
                get(move |Query(params): Query<HashMap<String, String>>| async move {
                    let t = params.get("t").map(String::as_str).unwrap_or("");
                    if params.contains_key("ep") || t == "search" && params.get("q").is_some_and(|q| q.contains("E02")) {
                        format!(
                            r#"<rss><channel>
                                <item>
                                    <title>Show.S01E02.1080p.WEB-DL</title>
                                    <size>1000000000</size>
                                    <jackettindexer>TrackerA</jackettindexer>
                                    <torznab:attr name="seeders" value="25"/>
                                    <torznab:attr name="infohash" value="{dup_hash}"/>
                                    <torznab:attr name="magneturl" value="magnet:?xt=urn:btih:{dup_hash}"/>
                                </item>
                                <item>
                                    <title>Show.S01E02.720p.AlreadyInQbit</title>
                                    <size>500000000</size>
                                    <jackettindexer>TrackerA</jackettindexer>
                                    <torznab:attr name="seeders" value="99"/>
                                    <torznab:attr name="infohash" value="{qbit_hash}"/>
                                    <torznab:attr name="magneturl" value="magnet:?xt=urn:btih:{qbit_hash}"/>
                                </item>
                            </channel></rss>"#
                        )
                    } else {
                        format!(
                            r#"<rss><channel>
                                <item>
                                    <title>Show.S01.COMPLETE.1080p</title>
                                    <size>9000000000</size>
                                    <jackettindexer>TrackerB</jackettindexer>
                                    <torznab:attr name="seeders" value="50"/>
                                    <torznab:attr name="infohash" value="{pack_hash}"/>
                                    <torznab:attr name="magneturl" value="magnet:?xt=urn:btih:{pack_hash}"/>
                                </item>
                                <item>
                                    <title>Show.S01E02.Duplicate.From.SeasonSearch</title>
                                    <size>1000000000</size>
                                    <jackettindexer>TrackerB</jackettindexer>
                                    <torznab:attr name="seeders" value="10"/>
                                    <torznab:attr name="infohash" value="{dup_hash}"/>
                                    <torznab:attr name="magneturl" value="magnet:?xt=urn:btih:{dup_hash}"/>
                                </item>
                            </channel></rss>"#
                        )
                    }
                }),
            )
            .route(
                "/api/v2/auth/login",
                post(|| async {
                    (
                        StatusCode::OK,
                        [(header::SET_COOKIE, "SID=mock-sid; Path=/")],
                        "Ok.",
                    )
                }),
            )
            .route(
                "/api/v2/torrents/info",
                get(move || async move {
                    axum::Json(vec![serde_json::json!({
                        "hash": qbit_hash,
                        "tags": "tt0903747,series",
                        "name": "Show.S01",
                        "size": 2_000_000_000u64,
                        "progress": 0.5,
                        "state": "downloading",
                        "added_on": 1_700_000_000u64,
                        "dlspeed": 0u64,
                        "upspeed": 0u64,
                        "eta": 0i64,
                    })])
                }),
            )
            .route(
                "/api/v2/torrents/files",
                get(|| async {
                    axum::Json(vec![serde_json::json!({
                        "index": 0,
                        "name": "Show.S01E02.720p.mkv",
                        "size": 500_000_000u64,
                        "progress": 1.0,
                        "priority": 1u8,
                    })])
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let base_url = format!("http://{}", addr);

        let config = Config {
            port: 3000,
            public_url: "http://localhost:3000".to_string(),
            jackett_url: base_url.clone(),
            jackett_api_key: "key".to_string(),
            jackett_search_type: "imdb".to_string(),
            qbittorrent_url: base_url,
            qbittorrent_username: "admin".to_string(),
            qbittorrent_password: "admin".to_string(),
            download_path: "/tmp".to_string(),
            retention_days: 0,
        };
        let state = AppState {
            config: Arc::new(config.clone()),
            qbit: Arc::new(QbitClient::new(config)),
            http_client: reqwest::Client::new(),
            db: Arc::new(DbClient::new(":memory:")),
            admin_session_token: "tok".to_string(),
            triggered_torrents: Arc::new(RwLock::new(HashSet::new())),
            placeholder_bank: None,
            live_sessions: Arc::new(LiveSessionStore::default()),
        };

        let plans = vec![
            TorznabParams::SeriesImdb {
                imdb_id: "tt0903747".to_string(),
                season: 1,
                episode: 2,
            },
            TorznabParams::SeriesSeasonImdb {
                imdb_id: "tt0903747".to_string(),
                season: 1,
            },
        ];
        let resp = build_stream_response(
            &state,
            "userkey",
            "tt0903747:1:2",
            true,
            1,
            2,
            plans,
            None,
            "Show S01E02",
        )
        .await;

        assert_eq!(resp.streams.len(), 3);
        assert_eq!(
            resp.streams[0].name.as_deref(),
            Some("Available (Best Match)")
        );
        assert_eq!(resp.streams[1].name.as_deref(), Some("Qbit - TrackerB"));
        assert_eq!(resp.streams[2].name.as_deref(), Some("Qbit - TrackerA"));
    }

    #[tokio::test]
    async fn filters_sample_and_extra_video_files_while_keeping_legitimate_alternates() {
        let hash = "dddd1111eeee2222ffff3333aaaa4444bbbb5555";
        let app = Router::new()
            .route(
                "/api/v2.0/indexers/all/results/torznab/api",
                get(|| async { "<rss><channel></channel></rss>" }),
            )
            .route(
                "/api/v2/auth/login",
                post(|| async {
                    (
                        StatusCode::OK,
                        [(header::SET_COOKIE, "SID=mock-sid; Path=/")],
                        "Ok.",
                    )
                }),
            )
            .route(
                "/api/v2/torrents/info",
                get(move || async move {
                    axum::Json(vec![serde_json::json!({
                        "hash": hash,
                        "tags": "tt0903747,series",
                        "name": "Show.S01",
                        "size": 3_000_000_000u64,
                        "progress": 1.0,
                        "state": "uploading",
                        "added_on": 1_700_000_000u64,
                        "dlspeed": 0u64,
                        "upspeed": 0u64,
                        "eta": 0i64,
                    })])
                }),
            )
            .route(
                "/api/v2/torrents/files",
                get(|| async {
                    axum::Json(vec![
                        serde_json::json!({
                            "index": 0,
                            "name": "Show.S01/Show.S01E02.1080p.mkv",
                            "size": 1_200_000_000u64,
                            "progress": 1.0,
                            "priority": 1u8,
                        }),
                        serde_json::json!({
                            "index": 1,
                            "name": "Show.S01/Sample/sample.mkv",
                            "size": 25_000_000u64,
                            "progress": 0.0,
                            "priority": 0u8,
                        }),
                        serde_json::json!({
                            "index": 2,
                            "name": "Show.S01/Extras/NCOP.mkv",
                            "size": 90_000_000u64,
                            "progress": 0.0,
                            "priority": 0u8,
                        }),
                        serde_json::json!({
                            "index": 3,
                            "name": "Show.S01/Show.S01E02.Trailer.mkv",
                            "size": 40_000_000u64,
                            "progress": 0.0,
                            "priority": 0u8,
                        }),
                        serde_json::json!({
                            "index": 4,
                            "name": "Show.S01/Show.S01E02.720p.mkv",
                            "size": 600_000_000u64,
                            "progress": 0.0,
                            "priority": 0u8,
                        }),
                    ])
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let base_url = format!("http://{}", addr);

        let config = Config {
            port: 3000,
            public_url: "http://localhost:3000".to_string(),
            jackett_url: base_url.clone(),
            jackett_api_key: "key".to_string(),
            jackett_search_type: "imdb".to_string(),
            qbittorrent_url: base_url,
            qbittorrent_username: "admin".to_string(),
            qbittorrent_password: "admin".to_string(),
            download_path: "/tmp".to_string(),
            retention_days: 0,
        };
        let state = AppState {
            config: Arc::new(config.clone()),
            qbit: Arc::new(QbitClient::new(config)),
            http_client: reqwest::Client::new(),
            db: Arc::new(DbClient::new(":memory:")),
            admin_session_token: "tok".to_string(),
            triggered_torrents: Arc::new(RwLock::new(HashSet::new())),
            placeholder_bank: None,
            live_sessions: Arc::new(LiveSessionStore::default()),
        };

        let filter_fn: FilterFn = Arc::new(|t: &str| is_definitely_wrong_episode(t, 1, 2));
        let resp = build_stream_response(
            &state,
            "userkey",
            "tt0903747:1:2",
            true,
            1,
            2,
            vec![],
            Some(filter_fn),
            "Show S01E02",
        )
        .await;

        assert_eq!(resp.streams.len(), 2);
        assert!(
            resp.streams[0]
                .title
                .as_deref()
                .unwrap()
                .contains("Show.S01E02.1080p.mkv")
        );
        assert!(
            resp.streams[1]
                .title
                .as_deref()
                .unwrap()
                .contains("Show.S01E02.720p.mkv")
        );
    }
}
