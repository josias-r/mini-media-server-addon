use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration as StdDuration;

use crate::db::UserRecord;
use crate::routes::AppState;
use crate::routes::api::json_error;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/session", get(admin_session))
        .route("/login", post(login_submit))
        .route("/logout", post(logout))
        .route("/password", post(change_password))
        .route("/overview", get(admin_overview))
        .route("/users", post(create_user))
        .route("/users/{api_key}", delete(delete_user))
        .route("/torrents/{hash}", delete(delete_torrent_handler))
}

fn check_auth(headers: &HeaderMap, state: &AppState) -> bool {
    if let Some(cookie_str) = headers.get(header::COOKIE).and_then(|h| h.to_str().ok()) {
        let expected_cookie = format!("admin_session={}", state.admin_session_token);
        for cookie in cookie_str.split(';') {
            if cookie.trim() == expected_cookie {
                return true;
            }
        }
    }

    false
}

async fn admin_session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !check_auth(&headers, &state) {
        return json_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    Json(serde_json::json!({ "authenticated": true })).into_response()
}

#[derive(Deserialize)]
struct LoginRequest {
    password: String,
}

async fn login_submit(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Response {
    let expected_password = state.db.get_admin_password().unwrap_or_default();

    if payload.password == expected_password {
        let cookie_value = format!(
            "admin_session={}; Path=/; HttpOnly; SameSite=Lax",
            state.admin_session_token
        );
        (
            StatusCode::OK,
            [(header::SET_COOKIE, cookie_value)],
            Json(serde_json::json!({ "ok": true })),
        )
            .into_response()
    } else {
        json_error(StatusCode::UNAUTHORIZED, "Invalid password")
    }
}

async fn logout() -> Response {
    let cookie_value = "admin_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie_value.to_string())],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct ChangePasswordRequest {
    password: String,
}

async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChangePasswordRequest>,
) -> Response {
    if !check_auth(&headers, &state) {
        return json_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }

    if payload.password.len() < 4 {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Password must be at least 4 characters",
        );
    }

    match state.db.set_admin_password(&payload.password) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update password: {}", e),
        ),
    }
}

#[derive(Serialize)]
struct AdminHealth {
    qbittorrent: bool,
    jackett: bool,
}

#[derive(Serialize)]
struct AdminConfigInfo {
    public_url: String,
    retention_days: u64,
    jackett_search_type: String,
}

#[derive(Serialize)]
struct AdminTorrentInfo {
    hash: String,
    name: String,
    size: u64,
    progress: f64,
    state: String,
    dlspeed: u64,
    upspeed: u64,
    eta: i64,
    added_on: u64,
    last_watched_at: Option<String>,
    expires_at: Option<String>,
}

#[derive(Serialize)]
struct AdminOverviewResponse {
    health: AdminHealth,
    config: AdminConfigInfo,
    users: Vec<UserRecord>,
    torrents: Vec<AdminTorrentInfo>,
}

async fn check_jackett_health(state: &AppState) -> bool {
    let base = state.config.jackett_url.trim_end_matches('/');
    let url = format!(
        "{}/api/v2.0/indexers/all/results/torznab/api?t=caps&apikey={}",
        base,
        urlencoding::encode(&state.config.jackett_api_key)
    );

    match state
        .http_client
        .get(&url)
        .timeout(StdDuration::from_secs(3))
        .send()
        .await
    {
        Ok(res) => res.status().is_success(),
        Err(_) => false,
    }
}

fn compute_expires_at(
    retention_days: u64,
    last_watched_at: Option<&str>,
    added_on: u64,
) -> Option<String> {
    if retention_days == 0 {
        return None;
    }
    let retention_secs = retention_days as i64 * 86_400;

    if let Some(last) = last_watched_at
        && let Ok(parsed) = NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S")
    {
        let dt = DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc)
            + Duration::seconds(retention_secs);
        return Some(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    }

    if added_on > 0
        && let Some(dt) = DateTime::<Utc>::from_timestamp(added_on as i64 + retention_secs, 0)
    {
        return Some(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    }

    None
}

async fn admin_overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !check_auth(&headers, &state) {
        return json_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }

    let (qbit_ok, jackett_ok, raw_torrents) = tokio::join!(
        state.qbit.check_health(),
        check_jackett_health(&state),
        state.qbit.get_all_torrents()
    );

    let watch_times = state.db.get_torrent_watch_times().unwrap_or_default();
    let users = state.db.get_users_with_stats().unwrap_or_default();

    let torrents = raw_torrents
        .into_iter()
        .map(|t| {
            let last_watched_at = watch_times.get(&t.hash.to_lowercase()).cloned();
            let expires_at = compute_expires_at(
                state.config.retention_days,
                last_watched_at.as_deref(),
                t.added_on,
            );
            AdminTorrentInfo {
                hash: t.hash,
                name: t.name,
                size: t.size,
                progress: t.progress,
                state: t.state,
                dlspeed: t.dlspeed,
                upspeed: t.upspeed,
                eta: t.eta,
                added_on: t.added_on,
                last_watched_at,
                expires_at,
            }
        })
        .collect();

    Json(AdminOverviewResponse {
        health: AdminHealth {
            qbittorrent: qbit_ok,
            jackett: jackett_ok,
        },
        config: AdminConfigInfo {
            public_url: state.config.public_url.clone(),
            retention_days: state.config.retention_days,
            jackett_search_type: state.config.jackett_search_type.clone(),
        },
        users,
        torrents,
    })
    .into_response()
}

#[derive(Deserialize)]
struct CreateUserRequest {
    username: String,
}

#[derive(Serialize)]
struct CreateUserResponse {
    username: String,
    api_key: String,
    created_at: String,
}

async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateUserRequest>,
) -> Response {
    if !check_auth(&headers, &state) {
        return json_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }

    let username = payload.username.trim();
    if username.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "Username cannot be empty");
    }

    match state.db.create_user(username) {
        Ok(api_key) => {
            let created_at = state
                .db
                .get_user_by_key(&api_key)
                .ok()
                .flatten()
                .map(|(_, _, ts)| ts)
                .unwrap_or_else(|| Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());

            (
                StatusCode::CREATED,
                Json(CreateUserResponse {
                    username: username.to_string(),
                    api_key,
                    created_at,
                }),
            )
                .into_response()
        }
        Err(e) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create user: {}", e),
        ),
    }
}

async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(api_key): Path<String>,
) -> Response {
    if !check_auth(&headers, &state) {
        return json_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }

    match state.db.delete_user(&api_key) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete user: {}", e),
        ),
    }
}

async fn delete_torrent_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(hash): Path<String>,
) -> Response {
    if !check_auth(&headers, &state) {
        return json_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }

    if state.qbit.delete_torrent(&hash, true).await {
        let _ = state.db.delete_watch_history(&hash);
        Json(serde_json::json!({ "ok": true })).into_response()
    } else {
        json_error(
            StatusCode::BAD_GATEWAY,
            "Failed to delete torrent from qBittorrent",
        )
    }
}
