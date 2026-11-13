use super::AppState;
use axum::body::Body;
use axum::http::header::RANGE;
use axum::{
    extract::{Path, Query, Request, State},
    response::{IntoResponse, Response},
};
use serde_json::Value;
use std::collections::HashMap;
use tokio::process::Command;
use tower::ServiceExt;
use tower_http::services::ServeFile;

async fn is_simple_format(path: &std::path::Path) -> bool {
    log::info!("Probing media file: {:?}", path);
    let mut cmd = Command::new("ffprobe");
    cmd.arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("stream=codec_name,codec_type")
        .arg("-of")
        .arg("json")
        .arg(path);

    match cmd.output().await {
        Ok(output) => {
            if !output.status.success() {
                log::warn!("ffprobe returned error status: {}", output.status);
                let err_str = String::from_utf8_lossy(&output.stderr);
                log::warn!("ffprobe stderr: {}", err_str);
                return false;
            }
            if let Ok(json) = serde_json::from_slice::<Value>(&output.stdout) {
                if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
                    let mut has_video = false;
                    let mut is_compatible_video = false;

                    for stream in streams {
                        if let Some(codec_type) = stream.get("codec_type").and_then(|s| s.as_str())
                            && codec_type == "video"
                        {
                            has_video = true;
                            if let Some(codec_name) =
                                stream.get("codec_name").and_then(|s| s.as_str())
                            {
                                log::info!("Found video codec: {}", codec_name);
                                if codec_name == "h264" || codec_name == "hevc" {
                                    is_compatible_video = true;
                                }
                            }
                        }
                    }
                    log::info!(
                        "Probe result: has_video={}, is_compatible_video={}",
                        has_video,
                        is_compatible_video
                    );
                    return has_video && is_compatible_video;
                } else {
                    log::warn!("ffprobe JSON output missing 'streams' array");
                }
            } else {
                log::warn!("Failed to parse ffprobe JSON output");
            }
        }
        Err(e) => {
            log::error!("Failed to execute ffprobe: {}", e);
        }
    }
    false
}

pub async fn stream_file(
    State(state): State<AppState>,
    Path((api_key, info_hash)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    req: Request,
) -> Response {
    if !state.db.validate_api_key(&api_key) {
        return axum::response::Response::builder()
            .status(401)
            .body(Body::from("Unauthorized"))
            .unwrap();
    }

    let file_path = params.get("filePath").unwrap_or(&String::new()).clone();
    let full_path = std::path::Path::new(&state.config.download_path).join(&file_path);

    log::info!(
        "Stream request for info_hash: {}, filePath: {}",
        info_hash,
        file_path
    );

    if !full_path.starts_with(&state.config.download_path) {
        log::warn!(
            "Security violation attempt or invalid path: {:?}",
            full_path
        );
        return axum::response::Response::builder()
            .status(404)
            .body(Body::from("Not Found"))
            .unwrap();
    }

    if !full_path.exists() {
        log::warn!("File does not exist (yet): {:?}", full_path);
        // We should still proceed, ServeFile might handle it or we wait. But ffprobe will definitely fail.
    }

    let ext = full_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let range_header = req
        .headers()
        .get(RANGE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("none")
        .to_string();
    log::info!("File extension: '{}', Range header: {}", ext, range_header);

    // Check if it contains a simple format (for logging purposes)
    if ext != "mp4" && ext != "webm" && full_path.exists() {
        if is_simple_format(&full_path).await {
            log::info!("File contains a compatible video format (H.264/HEVC).");
            log::warn!(
                "Remuxing via pipe is disabled because it breaks HTTP Range requests required by Stremio."
            );
        } else {
            log::info!("File does not contain a compatible video format.");
        }
    }

    log::info!("Serving file directly via ServeFile: {:?}", full_path);
    ServeFile::new(full_path)
        .oneshot(req)
        .await
        .unwrap()
        .into_response()
}
