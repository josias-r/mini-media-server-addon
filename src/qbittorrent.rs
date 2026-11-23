use crate::config::Config;
use crate::domain::{
    compute_torrent_info_hash, extract_btih_hex, is_valid_hex_hash, torrent_has_tag,
};
use log::{debug, error, warn};
use reqwest::{Client, RequestBuilder, Response, StatusCode, multipart};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct QbitClient {
    client: Client,
    no_redirect_client: Client,
    config: Config,
    session_cookie: RwLock<Option<(String, Instant)>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct QbitTorrent {
    pub hash: String,
    pub tags: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub added_on: u64,
    #[serde(default)]
    pub dlspeed: u64,
    #[serde(default)]
    pub upspeed: u64,
    #[serde(default)]
    pub eta: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct QbitFile {
    #[serde(default)]
    pub index: Option<usize>,
    pub name: String,
    pub size: u64,
    pub progress: f64,
    pub priority: u8,
}

impl QbitClient {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::builder().build().unwrap(),
            no_redirect_client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            config,
            session_cookie: RwLock::new(None),
        }
    }

    async fn get_session_cookie(&self) -> Option<String> {
        {
            let cache = self.session_cookie.read().await;
            if let Some((cookie, expiry)) = &*cache
                && Instant::now() < *expiry
            {
                debug!("Using cached qBittorrent session cookie");
                return Some(cookie.clone());
            }
        }

        let params = [
            ("username", &self.config.qbittorrent_username),
            ("password", &self.config.qbittorrent_password),
        ];

        debug!("Logging in to qBittorrent...");
        let url = format!("{}/api/v2/auth/login", self.config.qbittorrent_url);
        let res = self
            .client
            .post(&url)
            .header("Origin", &self.config.qbittorrent_url)
            .header("Referer", format!("{}/", self.config.qbittorrent_url))
            .form(&params)
            .send()
            .await;

        let r = match res {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to login to qBittorrent: {}", e);
                return None;
            }
        };

        let status = r.status();
        let Some(cookie) = r.headers().get("set-cookie") else {
            error!(
                "No cookie returned from qBittorrent login, status: {}",
                status
            );
            return None;
        };

        let cookie_str = cookie.to_str().unwrap_or("");
        let sid = cookie_str.split(';').next().unwrap_or("").to_string();
        let mut duration_secs = 3600u64;

        for part in cookie_str.split(';').map(str::trim) {
            let lower = part.to_ascii_lowercase();
            if let Some(val) = lower.strip_prefix("max-age=") {
                if let Ok(secs) = val.parse::<u64>() {
                    duration_secs = secs;
                }
            } else if lower.starts_with("expires=") {
                let clean_date = part[8..].replace('-', " ");
                if let Ok(parsed) = chrono::DateTime::parse_from_rfc2822(&clean_date) {
                    let diff = parsed
                        .with_timezone(&chrono::Utc)
                        .signed_duration_since(chrono::Utc::now())
                        .num_seconds();
                    if diff > 0 {
                        duration_secs = diff as u64;
                    }
                }
            }
        }

        debug!(
            "Caching new qBittorrent session cookie for {} seconds",
            duration_secs
        );
        *self.session_cookie.write().await = Some((
            sid.clone(),
            Instant::now() + Duration::from_secs(duration_secs),
        ));
        Some(sid)
    }

    async fn invalidate_session_cookie(&self) {
        *self.session_cookie.write().await = None;
    }

    async fn authed_send(
        &self,
        build: impl Fn(&Client, String) -> RequestBuilder,
        context: &str,
    ) -> Option<Response> {
        for attempt in 0..2 {
            let sid = self.get_session_cookie().await?;
            match build(&self.client, sid).send().await {
                Ok(res) if res.status().is_success() => return Some(res),
                Ok(res) if res.status() == StatusCode::FORBIDDEN && attempt == 0 => {
                    warn!(
                        "qBittorrent returned 403 Forbidden while trying to {}; refreshing session cookie",
                        context
                    );
                    self.invalidate_session_cookie().await;
                }
                Ok(res) => {
                    error!("Failed to {}, status: {}", context, res.status());
                    return None;
                }
                Err(e) => {
                    error!("Error {}: {}", context, e);
                    return None;
                }
            }
        }
        None
    }

    async fn authed_get(&self, path: &str, context: &str) -> Option<Response> {
        let url = format!("{}{}", self.config.qbittorrent_url, path);
        self.authed_send(|c, sid| c.get(&url).header("Cookie", sid), context)
            .await
    }

    async fn authed_post_form(&self, path: &str, params: &[(&str, &str)], context: &str) -> bool {
        let url = format!("{}{}", self.config.qbittorrent_url, path);
        self.authed_send(
            |c, sid| c.post(&url).header("Cookie", sid).form(params),
            context,
        )
        .await
        .is_some()
    }

    async fn authed_post_multipart(
        &self,
        path: &str,
        build_form: impl Fn() -> multipart::Form,
        context: &str,
    ) -> bool {
        let url = format!("{}{}", self.config.qbittorrent_url, path);
        self.authed_send(
            |c, sid| c.post(&url).header("Cookie", sid).multipart(build_form()),
            context,
        )
        .await
        .is_some()
    }

    async fn send_add_torrent_form(&self, build_form: impl Fn() -> multipart::Form) -> bool {
        let ok = self
            .authed_post_multipart("/api/v2/torrents/add", build_form, "add torrent")
            .await;
        if ok {
            debug!("Successfully added torrent");
        }
        ok
    }

    pub async fn add_torrent(&self, magnet_uri: &str, tag: &str) -> bool {
        debug!("Adding torrent to qBittorrent with tag: {}", tag);
        self.send_add_torrent_form(|| {
            multipart::Form::new()
                .text("urls", magnet_uri.to_string())
                .text("tags", tag.to_string())
                .text("sequentialDownload", "true")
                .text("firstLastPiecePrio", "true")
        })
        .await
    }

    pub async fn add_torrent_bytes(&self, torrent_bytes: Vec<u8>, tag: &str) -> bool {
        debug!(
            "Adding .torrent bytes ({} bytes) to qBittorrent with tag: {}",
            torrent_bytes.len(),
            tag
        );
        self.send_add_torrent_form(|| {
            let part = multipart::Part::bytes(torrent_bytes.clone())
                .file_name("torrent.torrent")
                .mime_str("application/x-bittorrent")
                .unwrap();
            multipart::Form::new()
                .part("torrents", part)
                .text("tags", tag.to_string())
                .text("sequentialDownload", "true")
                .text("firstLastPiecePrio", "true")
        })
        .await
    }

    /// Resolves a magnet URI or Jackett/HTTP `.torrent` download URL (including HTTP redirects
    /// to `magnet:` links or `.torrent` payloads), extracts or computes the 40-char hex
    /// `info_hash` when possible, and submits the torrent to qBittorrent.
    pub async fn resolve_and_add_torrent(
        &self,
        magnet_or_url: &str,
        tag: &str,
        known_hash: Option<&str>,
    ) -> Option<String> {
        let initial_hash = known_hash
            .map(str::trim)
            .filter(|h| is_valid_hex_hash(h))
            .map(str::to_lowercase);

        if magnet_or_url.starts_with("magnet:") {
            let extracted = extract_btih_hex(magnet_or_url);
            self.add_torrent(magnet_or_url, tag).await;
            return extracted.or(initial_hash);
        }

        if magnet_or_url.starts_with("http://") || magnet_or_url.starts_with("https://") {
            let mut current_url = magnet_or_url.to_string();
            for _ in 0..5 {
                match self.no_redirect_client.get(&current_url).send().await {
                    Ok(res) if res.status().is_redirection() => {
                        let Some(location) = res
                            .headers()
                            .get(reqwest::header::LOCATION)
                            .and_then(|v| v.to_str().ok())
                        else {
                            break;
                        };

                        if location.starts_with("magnet:") {
                            let extracted = extract_btih_hex(location);
                            self.add_torrent(location, tag).await;
                            return extracted.or(initial_hash);
                        }

                        if let Ok(next_url) = res.url().join(location) {
                            current_url = next_url.to_string();
                        } else {
                            break;
                        }
                    }
                    Ok(res) if res.status().is_success() => {
                        if let Ok(bytes) = res.bytes().await {
                            if let Ok(text) = std::str::from_utf8(&bytes) {
                                let trimmed = text.trim();
                                if trimmed.starts_with("magnet:") {
                                    let extracted = extract_btih_hex(trimmed);
                                    self.add_torrent(trimmed, tag).await;
                                    return extracted.or(initial_hash);
                                }
                            }

                            let computed_hash = compute_torrent_info_hash(&bytes);
                            if computed_hash.is_some() || bytes.starts_with(b"d") {
                                self.add_torrent_bytes(bytes.to_vec(), tag).await;
                                return computed_hash.or(initial_hash);
                            }
                            warn!(
                                "Unexpected non-torrent payload ({} bytes) from {}",
                                bytes.len(),
                                current_url
                            );
                        }
                        break;
                    }
                    Ok(res) => {
                        warn!(
                            "Failed to resolve torrent URL {} (status: {})",
                            current_url,
                            res.status()
                        );
                        break;
                    }
                    Err(e) => {
                        warn!("Error resolving torrent URL {}: {}", current_url, e);
                        break;
                    }
                }
            }
        }

        self.add_torrent(magnet_or_url, tag).await;
        initial_hash
    }

    pub async fn get_torrents_by_tag(&self, tag: &str) -> Vec<QbitTorrent> {
        debug!("Fetching torrents by tag: {}", tag);
        self.get_all_torrents()
            .await
            .into_iter()
            .filter(|t| torrent_has_tag(&t.tags, tag))
            .collect()
    }

    pub async fn get_all_torrents(&self) -> Vec<QbitTorrent> {
        debug!("Fetching all torrents");
        let Some(res) = self
            .authed_get("/api/v2/torrents/info?filter=all", "fetch all torrents")
            .await
        else {
            return vec![];
        };
        res.json::<Vec<QbitTorrent>>().await.unwrap_or_else(|_| {
            error!("Failed to parse qBittorrent torrents JSON");
            vec![]
        })
    }

    pub async fn delete_torrent(&self, hash: &str, delete_files: bool) -> bool {
        let hash = hash.trim().to_lowercase();
        debug!("Deleting torrent {} (delete_files: {})", hash, delete_files);
        let params = [
            ("hashes", hash.as_str()),
            ("deleteFiles", if delete_files { "true" } else { "false" }),
        ];
        let ok = self
            .authed_post_form(
                "/api/v2/torrents/delete",
                &params,
                &format!("delete torrent {}", hash),
            )
            .await;
        if ok {
            debug!("Successfully deleted torrent {}", hash);
        }
        ok
    }

    pub async fn get_torrent_files(&self, info_hash: &str) -> Vec<QbitFile> {
        let info_hash = info_hash.trim().to_lowercase();
        debug!("Fetching files for torrent: {}", info_hash);
        let path = format!("/api/v2/torrents/files?hash={}", info_hash);
        let Some(res) = self.authed_get(&path, "fetch files").await else {
            return vec![];
        };
        res.json::<Vec<QbitFile>>().await.unwrap_or_else(|_| {
            error!("Failed to parse qBittorrent files JSON");
            vec![]
        })
    }

    pub async fn set_file_priorities(&self, hash: &str, file_ids: &[usize], priority: u8) -> bool {
        if file_ids.is_empty() {
            return true;
        }

        let hash = hash.trim().to_lowercase();
        let id_str = file_ids
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join("|");
        debug!(
            "Setting priority {} for files {} in torrent {}",
            priority, id_str, hash
        );

        self.authed_post_multipart(
            "/api/v2/torrents/filePrio",
            || {
                multipart::Form::new()
                    .text("hash", hash.clone())
                    .text("id", id_str.clone())
                    .text("priority", priority.to_string())
            },
            "set file priorities",
        )
        .await
    }

    pub async fn resume_torrent(&self, hash: &str) -> bool {
        let hash = hash.trim().to_lowercase();
        debug!("Resuming torrent {}", hash);
        self.authed_post_form(
            "/api/v2/torrents/start",
            &[("hashes", hash.as_str())],
            "resume torrent",
        )
        .await
    }

    pub async fn add_torrent_tags(&self, hash: &str, tags: &str) -> bool {
        let (hash, tags) = (hash.trim().to_lowercase(), tags.trim());
        if hash.is_empty() || tags.is_empty() {
            return true;
        }

        debug!("Adding tags {:?} to torrent {}", tags, hash);
        self.authed_post_form(
            "/api/v2/torrents/addTags",
            &[("hashes", hash.as_str()), ("tags", tags)],
            &format!("add tags to torrent {}", hash),
        )
        .await
    }

    pub async fn check_health(&self) -> bool {
        self.get_session_cookie().await.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        http::{HeaderMap, header},
        routing::{get, post},
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn refreshes_session_cookie_and_retries_on_403_forbidden() {
        let logins = Arc::new(AtomicUsize::new(0));
        let logins_clone = logins.clone();
        let active_sid = Arc::new(std::sync::Mutex::new(String::new()));
        let sid_for_login = active_sid.clone();
        let sid_for_info = active_sid.clone();
        let sid_for_prio = active_sid.clone();
        let prio_attempts = Arc::new(AtomicUsize::new(0));
        let prio_clone = prio_attempts.clone();

        let app = Router::new()
            .route(
                "/api/v2/auth/login",
                post(move || {
                    let logins = logins_clone.clone();
                    let active_sid = sid_for_login.clone();
                    async move {
                        let n = logins.fetch_add(1, Ordering::SeqCst) + 1;
                        let sid = format!("SID=mock-sid-{}", n);
                        *active_sid.lock().unwrap() = sid.clone();
                        (StatusCode::OK, [(header::SET_COOKIE, sid)], "Ok.")
                    }
                }),
            )
            .route(
                "/api/v2/torrents/info",
                get(move |headers: HeaderMap| {
                    let active_sid = sid_for_info.clone();
                    async move {
                        let cookie = headers
                            .get(header::COOKIE)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        if cookie == "SID=mock-sid-1" {
                            // Simulate server restart invalidating sid-1 right before info call
                            *active_sid.lock().unwrap() = "invalidated".to_string();
                            return (StatusCode::FORBIDDEN, axum::Json(Vec::<QbitTorrent>::new()));
                        }
                        if cookie == *active_sid.lock().unwrap() {
                            return (
                                StatusCode::OK,
                                axum::Json(vec![QbitTorrent {
                                    hash: "abcd".to_string(),
                                    tags: "tt123".to_string(),
                                    name: "Movie".to_string(),
                                    size: 100,
                                    progress: 1.0,
                                    state: "uploading".to_string(),
                                    added_on: 1,
                                    dlspeed: 0,
                                    upspeed: 0,
                                    eta: 0,
                                }]),
                            );
                        }
                        (StatusCode::FORBIDDEN, axum::Json(Vec::<QbitTorrent>::new()))
                    }
                }),
            )
            .route(
                "/api/v2/torrents/filePrio",
                post(move |headers: HeaderMap| {
                    let active_sid = sid_for_prio.clone();
                    let attempts = prio_clone.clone();
                    async move {
                        let n = attempts.fetch_add(1, Ordering::SeqCst);
                        let cookie = headers
                            .get(header::COOKIE)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        if n == 0 {
                            // Force a 403 on first multipart attempt too
                            *active_sid.lock().unwrap() = "rotated".to_string();
                            return StatusCode::FORBIDDEN;
                        }
                        if cookie == *active_sid.lock().unwrap() {
                            StatusCode::OK
                        } else {
                            StatusCode::FORBIDDEN
                        }
                    }
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = QbitClient::new(Config {
            port: 3000,
            public_url: "http://localhost:3000".to_string(),
            jackett_url: "http://localhost:9117".to_string(),
            jackett_api_key: "test".to_string(),
            jackett_search_type: "text".to_string(),
            qbittorrent_url: format!("http://{}", addr),
            qbittorrent_username: "admin".to_string(),
            qbittorrent_password: "admin".to_string(),
            download_path: "/tmp".to_string(),
            retention_days: 0,
        });

        let torrents = client.get_all_torrents().await;
        assert_eq!(torrents.len(), 1);
        assert_eq!(torrents[0].hash, "abcd");
        assert_eq!(logins.load(Ordering::SeqCst), 2);

        assert!(client.set_file_priorities("abcd", &[0], 6).await);
        assert_eq!(logins.load(Ordering::SeqCst), 3);
        assert_eq!(prio_attempts.load(Ordering::SeqCst), 2);
    }
}
