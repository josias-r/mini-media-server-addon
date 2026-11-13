use crate::routes::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;

pub async fn manifest(
    State(state): State<AppState>,
    Path(api_key): Path<String>,
) -> Result<impl IntoResponse, Response> {
    if !state.db.validate_api_key(&api_key) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid API Key").into_response());
    }

    Ok(Json(json!({
        "id": "mini-media-server.addon",
        "version": "1.0.0",
        "name": "Mini Media Server",
        "description": "A tiny Stremio addon acting similar to a self-hostable debrid service.",
        "resources": ["stream"],
        "types": ["movie", "series"],
        "catalogs": []
    })))
}
