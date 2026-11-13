use super::AppState;
use crate::cinemeta::get_cinemeta_title;
use crate::jackett::TorznabParams;
use crate::parser::is_definitely_wrong_episode;
use crate::stream_builder::build_stream_response;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;

pub async fn series_stream(
    State(state): State<AppState>,
    Path((api_key, id_ext)): Path<(String, String)>,
) -> Result<impl IntoResponse, Response> {
    if !state.db.validate_api_key(&api_key) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid API Key").into_response());
    }

    let id_str = id_ext.replace(".json", "");

    let parts: Vec<&str> = id_str.split(':').collect();
    let series_id = parts[0].to_string();
    let season: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let episode: u32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);

    let mut fetch_plans = vec![
        TorznabParams::SeriesImdb {
            imdb_id: series_id.clone(),
            season,
            episode,
        },
        TorznabParams::SeriesSeasonImdb {
            imdb_id: series_id.clone(),
            season,
        },
    ];

    let mut expected_title = id_str.clone();
    if let Some(title) = get_cinemeta_title(&state.http_client, "series", &series_id).await {
        expected_title = format!("{} S{:02}E{:02}", title, season, episode);
        if state.config.jackett_search_type == "text" {
            fetch_plans = vec![
                TorznabParams::SeriesText {
                    query: title.clone(),
                    season,
                    episode,
                },
                TorznabParams::SeriesSeasonText {
                    query: title,
                    season,
                },
            ];
        }
    }

    let filter_fn =
        Arc::new(move |title: &str| is_definitely_wrong_episode(title, season, episode));

    let response = build_stream_response(
        &state,
        &api_key,
        &id_str,
        true,
        season,
        episode,
        fetch_plans,
        Some(filter_fn),
        &expected_title,
    )
    .await;
    Ok(Json(response))
}
