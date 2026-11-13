use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::routes::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/info", get(public_info))
        .route("/verify", get(verify_key))
        .nest("/admin", super::admin::router())
}

pub fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

#[derive(Serialize)]
struct PublicInfoResponse {
    public_url: String,
    retention_days: u64,
    jackett_search_type: String,
    version: &'static str,
}

async fn public_info(State(state): State<AppState>) -> impl IntoResponse {
    Json(PublicInfoResponse {
        public_url: state.config.public_url.clone(),
        retention_days: state.config.retention_days,
        jackett_search_type: state.config.jackett_search_type.clone(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Deserialize)]
struct VerifyQuery {
    api_key: Option<String>,
}

#[derive(Serialize)]
struct UserTorrentStatus {
    hash: String,
    name: String,
    size: u64,
    progress: f64,
    state: String,
    dlspeed: u64,
    last_watched_at: String,
}

#[derive(Serialize)]
struct VerifyKeyResponse {
    valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    active_torrents: Vec<UserTorrentStatus>,
}

async fn verify_key(
    State(state): State<AppState>,
    Query(query): Query<VerifyQuery>,
) -> impl IntoResponse {
    let key = query.api_key.unwrap_or_default().trim().to_string();
    if key.is_empty() {
        return Json(VerifyKeyResponse {
            valid: false,
            username: None,
            created_at: None,
            active_torrents: vec![],
        });
    }

    match state.db.get_user_by_key(&key) {
        Ok(Some((username, _, created_at))) => {
            let watch_history = state.db.get_user_watch_history(&key).unwrap_or_default();
            let active_torrents = if watch_history.is_empty() {
                vec![]
            } else {
                let torrents = state.qbit.get_all_torrents().await;
                let torrent_map: HashMap<String, _> = torrents
                    .into_iter()
                    .map(|t| (t.hash.to_lowercase(), t))
                    .collect();

                watch_history
                    .into_iter()
                    .filter_map(|(hash, last_watched_at)| {
                        torrent_map.get(&hash).map(|t| UserTorrentStatus {
                            hash: t.hash.clone(),
                            name: t.name.clone(),
                            size: t.size,
                            progress: t.progress,
                            state: t.state.clone(),
                            dlspeed: t.dlspeed,
                            last_watched_at,
                        })
                    })
                    .collect()
            };

            Json(VerifyKeyResponse {
                valid: true,
                username: Some(username),
                created_at: Some(created_at),
                active_torrents,
            })
        }
        _ => Json(VerifyKeyResponse {
            valid: false,
            username: None,
            created_at: None,
            active_torrents: vec![],
        }),
    }
}
