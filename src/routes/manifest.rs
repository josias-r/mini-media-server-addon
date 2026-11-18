use crate::routes::{AppState, require_valid_api_key};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json, Response},
};
use serde_json::json;

pub async fn manifest(State(state): State<AppState>, Path(api_key): Path<String>) -> Response {
    if let Some(err) = require_valid_api_key(&state, &api_key) {
        return err;
    }

    Json(json!({
        "id": "mini-media-server.addon",
        "version": env!("CARGO_PKG_VERSION"),
        "name": "Mini Media Server",
        "description": "A tiny Stremio addon acting similar to a self-hostable debrid service.",
        "resources": ["catalog", "stream"],
        "types": ["movie", "series"],
        "catalogs": [
            {
                "type": "movie",
                "id": "mms_user_movies",
                "name": "My History"
            },
            {
                "type": "series",
                "id": "mms_user_series",
                "name": "My History"
            },
            {
                "type": "movie",
                "id": "mms_community_movies",
                "name": "Community"
            },
            {
                "type": "series",
                "id": "mms_community_series",
                "name": "Community"
            }
        ]
    }))
    .into_response()
}
