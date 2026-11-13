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
        "version": "1.0.0",
        "name": "Mini Media Server",
        "description": "A tiny Stremio addon acting similar to a self-hostable debrid service.",
        "resources": ["stream"],
        "types": ["movie", "series"],
        "catalogs": []
    }))
    .into_response()
}
