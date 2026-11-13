use crate::config::Config;
use log::{debug, error};
use reqwest::{Client, multipart};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct QbitClient {
    client: Client,
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
    pub name: String,
    pub size: u64,
    pub progress: f64,
    pub priority: u8,
}

impl QbitClient {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::builder().build().unwrap(),
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

        match res {
            Ok(r) => {
                let status = r.status();
                if let Some(cookie) = r.headers().get("set-cookie") {
                    let cookie_str = cookie.to_str().unwrap_or("").to_string();
                    let sid = cookie_str.split(';').next().unwrap_or("").to_string();

                    let mut duration_secs = 3600;
                    for part in cookie_str.split(';') {
                        let part = part.trim();
                        if part.to_lowercase().starts_with("max-age=") {
                            if let Ok(secs) = part[8..].parse::<u64>() {
                                duration_secs = secs;
                            }
                        } else if part.to_lowercase().starts_with("expires=") {
                            let date_str = &part[8..];
                            // Try parsing standard HTTP date variant with hyphens or spaces
                            let clean_date = date_str.replace("-", " ");
                            if let Ok(parsed) = chrono::DateTime::parse_from_rfc2822(&clean_date) {
                                let now = chrono::Utc::now();
                                let diff = parsed
                                    .with_timezone(&chrono::Utc)
                                    .signed_duration_since(now)
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

                    let mut cache = self.session_cookie.write().await;
                    *cache = Some((
                        sid.clone(),
                        Instant::now() + Duration::from_secs(duration_secs),
                    ));
                    Some(sid)
                } else {
                    error!(
                        "No cookie returned from qBittorrent login, status: {}",
                        status
                    );
                    None
                }
            }
            Err(e) => {
                error!("Failed to login to qBittorrent: {}", e);
                None
            }
        }
    }

    pub async fn add_torrent(&self, magnet_uri: &str, tag: &str) -> bool {
        let sid = match self.get_session_cookie().await {
            Some(s) => s,
            None => return false,
        };

        debug!("Adding torrent to qBittorrent with tag: {}", tag);
        let form = multipart::Form::new()
            .text("urls", magnet_uri.to_string())
            .text("tags", tag.to_string())
            .text("sequentialDownload", "true")
            .text("firstLastPiecePrio", "true");

        let url = format!("{}/api/v2/torrents/add", self.config.qbittorrent_url);
        match self
            .client
            .post(&url)
            .header("Cookie", sid)
            .multipart(form)
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => {
                debug!("Successfully added torrent");
                true
            }
            Ok(res) => {
                error!("Failed to add torrent, status: {}", res.status());
                false
            }
            Err(e) => {
                error!("Error communicating with qBittorrent: {}", e);
                false
            }
        }
    }

    pub async fn get_torrents_by_tag(&self, tag: &str) -> Vec<QbitTorrent> {
        let sid = match self.get_session_cookie().await {
            Some(s) => s,
            None => return vec![],
        };

        debug!("Fetching torrents by tag: {}", tag);
        let url = format!(
            "{}/api/v2/torrents/info?filter=all",
            self.config.qbittorrent_url
        );
        match self.client.get(&url).header("Cookie", sid).send().await {
            Ok(res) if res.status().is_success() => {
                if let Ok(torrents) = res.json::<Vec<QbitTorrent>>().await {
                    torrents
                        .into_iter()
                        .filter(|t| t.tags.contains(tag))
                        .collect()
                } else {
                    error!("Failed to parse qBittorrent torrents JSON");
                    vec![]
                }
            }
            Ok(res) => {
                error!("Failed to fetch torrents, status: {}", res.status());
                vec![]
            }
            Err(e) => {
                error!("Error fetching torrents: {}", e);
                vec![]
            }
        }
    }

    pub async fn get_all_torrents(&self) -> Vec<QbitTorrent> {
        let sid = match self.get_session_cookie().await {
            Some(s) => s,
            None => return vec![],
        };

        debug!("Fetching all torrents");
        let url = format!(
            "{}/api/v2/torrents/info?filter=all",
            self.config.qbittorrent_url
        );
        match self.client.get(&url).header("Cookie", sid).send().await {
            Ok(res) if res.status().is_success() => {
                if let Ok(torrents) = res.json::<Vec<QbitTorrent>>().await {
                    torrents
                } else {
                    error!("Failed to parse qBittorrent torrents JSON");
                    vec![]
                }
            }
            Ok(res) => {
                error!("Failed to fetch all torrents, status: {}", res.status());
                vec![]
            }
            Err(e) => {
                error!("Error fetching all torrents: {}", e);
                vec![]
            }
        }
    }

    pub async fn delete_torrent(&self, hash: &str, delete_files: bool) -> bool {
        let hash = hash.trim().to_lowercase();
        let sid = match self.get_session_cookie().await {
            Some(s) => s,
            None => return false,
        };

        debug!("Deleting torrent {} (delete_files: {})", hash, delete_files);
        let params = [
            ("hashes", hash.as_str()),
            ("deleteFiles", if delete_files { "true" } else { "false" }),
        ];

        let url = format!("{}/api/v2/torrents/delete", self.config.qbittorrent_url);
        match self
            .client
            .post(&url)
            .header("Cookie", sid)
            .form(&params)
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => {
                debug!("Successfully deleted torrent {}", hash);
                true
            }
            Ok(res) => {
                error!(
                    "Failed to delete torrent {}, status: {}",
                    hash,
                    res.status()
                );
                false
            }
            Err(e) => {
                error!("Error deleting torrent: {}", e);
                false
            }
        }
    }

    pub async fn get_torrent_files(&self, info_hash: &str) -> Vec<QbitFile> {
        let info_hash = info_hash.trim().to_lowercase();
        let sid = match self.get_session_cookie().await {
            Some(s) => s,
            None => return vec![],
        };

        debug!("Fetching files for torrent: {}", info_hash);
        let url = format!(
            "{}/api/v2/torrents/files?hash={}",
            self.config.qbittorrent_url, info_hash
        );
        match self.client.get(&url).header("Cookie", sid).send().await {
            Ok(res) if res.status().is_success() => {
                if let Ok(files) = res.json::<Vec<QbitFile>>().await {
                    files
                } else {
                    error!("Failed to parse qBittorrent files JSON");
                    vec![]
                }
            }
            Ok(res) => {
                error!("Failed to fetch files, status: {}", res.status());
                vec![]
            }
            Err(e) => {
                error!("Error fetching files: {}", e);
                vec![]
            }
        }
    }

    pub async fn set_file_priorities(&self, hash: &str, file_ids: &[usize], priority: u8) -> bool {
        if file_ids.is_empty() {
            return true;
        }

        let hash = hash.trim().to_lowercase();
        let sid = match self.get_session_cookie().await {
            Some(s) => s,
            None => return false,
        };

        let id_str = file_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join("|");
        debug!(
            "Setting priority {} for files {} in torrent {}",
            priority, id_str, hash
        );

        let form = multipart::Form::new()
            .text("hash", hash)
            .text("id", id_str)
            .text("priority", priority.to_string());

        let url = format!("{}/api/v2/torrents/filePrio", self.config.qbittorrent_url);
        match self
            .client
            .post(&url)
            .header("Cookie", sid)
            .multipart(form)
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => true,
            Ok(res) => {
                error!("Failed to set file priorities, status: {}", res.status());
                false
            }
            Err(e) => {
                error!("Error setting file priorities: {}", e);
                false
            }
        }
    }

    pub async fn resume_torrent(&self, hash: &str) -> bool {
        let hash = hash.trim().to_lowercase();
        let sid = match self.get_session_cookie().await {
            Some(s) => s,
            None => return false,
        };

        debug!("Resuming torrent {}", hash);
        let params = [("hashes", hash.as_str())];
        let url = format!("{}/api/v2/torrents/start", self.config.qbittorrent_url);
        match self
            .client
            .post(&url)
            .header("Cookie", sid)
            .form(&params)
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => true,
            Ok(res) => {
                error!("Failed to resume torrent, status: {}", res.status());
                false
            }
            Err(e) => {
                error!("Error resuming torrent: {}", e);
                false
            }
        }
    }

    pub async fn check_health(&self) -> bool {
        self.get_session_cookie().await.is_some()
    }
}
