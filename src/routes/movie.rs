use super::{AppState, require_valid_api_key};
use crate::cinemeta::get_cinemeta_title;
use crate::jackett::TorznabParams;
use crate::stream_builder::build_stream_response;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json, Response},
};

pub async fn movie_stream(
    State(state): State<AppState>,
    Path((api_key, id_ext)): Path<(String, String)>,
) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    let id = id_ext.replace(".json", "");

    let mut expected_title = id.clone();
    let mut fetch_plans = vec![TorznabParams::MovieImdb {
        imdb_id: id.clone(),
    }];

    if let Some(title) = get_cinemeta_title(&state.http_client, "movie", &id).await {
        expected_title = title.clone();
        if state.config.jackett_search_type == "text" {
            fetch_plans = vec![TorznabParams::MovieText { query: title }];
        }
    }

    let response = build_stream_response(
        &state,
        &api_key,
        &id,
        false,
        1,
        1,
        fetch_plans,
        None,
        &expected_title,
    )
    .await;
    Json(response).into_response()
}
