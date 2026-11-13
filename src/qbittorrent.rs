use crate::config::Config;
use crate::jackett::extract_btih_hex;
use log::{debug, error, warn};
use reqwest::{Client, multipart};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
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

    async fn send_add_torrent_form(&self, form: multipart::Form) -> bool {
        let sid = match self.get_session_cookie().await {
            Some(s) => s,
            None => return false,
        };

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

    pub async fn add_torrent(&self, magnet_uri: &str, tag: &str) -> bool {
        debug!("Adding torrent to qBittorrent with tag: {}", tag);
        let form = multipart::Form::new()
            .text("urls", magnet_uri.to_string())
            .text("tags", tag.to_string())
            .text("sequentialDownload", "true")
            .text("firstLastPiecePrio", "true");

        self.send_add_torrent_form(form).await
    }

    pub async fn add_torrent_bytes(&self, torrent_bytes: Vec<u8>, tag: &str) -> bool {
        debug!(
            "Adding .torrent bytes ({} bytes) to qBittorrent with tag: {}",
            torrent_bytes.len(),
            tag
        );
        let part = multipart::Part::bytes(torrent_bytes)
            .file_name("torrent.torrent")
            .mime_str("application/x-bittorrent")
            .unwrap();
        let form = multipart::Form::new()
            .part("torrents", part)
            .text("tags", tag.to_string())
            .text("sequentialDownload", "true")
            .text("firstLastPiecePrio", "true");

        self.send_add_torrent_form(form).await
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
            .filter(|h| h.len() == 40 && h.chars().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_lowercase);

        if magnet_or_url.starts_with("magnet:") {
            let extracted = extract_btih_hex(magnet_or_url);
            self.add_torrent(magnet_or_url, tag).await;
            return extracted.or(initial_hash);
        }

        if magnet_or_url.starts_with("http://") || magnet_or_url.starts_with("https://") {
            let http_client = Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()
                .ok();

            if let Some(http) = http_client {
                let mut current_url = magnet_or_url.to_string();
                for _ in 0..5 {
                    match http.get(&current_url).send().await {
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
        }

        self.add_torrent(magnet_or_url, tag).await;
        initial_hash
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

const MAX_BENCODE_DEPTH: usize = 64;

fn parse_bencode_bytes(data: &[u8], pos: usize) -> Option<(&[u8], usize)> {
    let slice = data.get(pos..)?;
    let colon_rel = slice.iter().position(|&b| b == b':')?;
    if colon_rel == 0 {
        return None;
    }
    let len_str = std::str::from_utf8(&slice[..colon_rel]).ok()?;
    if !len_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let len: usize = len_str.parse().ok()?;
    let start = pos.checked_add(colon_rel + 1)?;
    let end = start.checked_add(len)?;
    let bytes = data.get(start..end)?;
    Some((bytes, end))
}

fn skip_bencode_value(data: &[u8], pos: usize, depth: usize) -> Option<usize> {
    if depth >= MAX_BENCODE_DEPTH {
        return None;
    }
    let tag = *data.get(pos)?;
    match tag {
        b'i' => {
            let mut cur = pos + 1;
            if data.get(cur) == Some(&b'-') {
                cur += 1;
            }
            let digits_start = cur;
            while let Some(&b) = data.get(cur) {
                if b == b'e' {
                    return (cur > digits_start).then_some(cur + 1);
                }
                if !b.is_ascii_digit() {
                    return None;
                }
                cur += 1;
            }
            None
        }
        b'l' => {
            let mut cur = pos + 1;
            while *data.get(cur)? != b'e' {
                cur = skip_bencode_value(data, cur, depth + 1)?;
            }
            Some(cur + 1)
        }
        b'd' => {
            let mut cur = pos + 1;
            while *data.get(cur)? != b'e' {
                let (_, next) = parse_bencode_bytes(data, cur)?;
                cur = skip_bencode_value(data, next, depth + 1)?;
            }
            Some(cur + 1)
        }
        b'0'..=b'9' => {
            let (_, next) = parse_bencode_bytes(data, pos)?;
            Some(next)
        }
        _ => None,
    }
}

fn sha1_hex(data: &[u8]) -> String {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity((data.len() + 9).div_ceil(64) * 64);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (slot, chunk) in w[..16].iter_mut().zip(block.chunks_exact(4)) {
            *slot = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = String::with_capacity(40);
    for word in h {
        let _ = write!(&mut out, "{word:08x}");
    }
    out
}

/// Computes the 40-character lowercase hex BitTorrent v1 `info_hash` (SHA-1 of the raw
/// bencoded `info` dictionary) from `.torrent` file bytes.
pub fn compute_torrent_info_hash(torrent_bytes: &[u8]) -> Option<String> {
    if torrent_bytes.first() != Some(&b'd') {
        return None;
    }

    let mut pos = 1;
    while *torrent_bytes.get(pos)? != b'e' {
        let (key, val_start) = parse_bencode_bytes(torrent_bytes, pos)?;
        let val_end = skip_bencode_value(torrent_bytes, val_start, 0)?;
        if key == b"info" {
            return Some(sha1_hex(&torrent_bytes[val_start..val_end]));
        }
        pos = val_end;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_matches_standard_vectors() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn computes_info_hash_from_bencoded_torrent() {
        let info_dict = b"d6:lengthi1234e4:name8:test.mkv12:piece lengthi16384e6:pieces20:01234567890123456789e";
        let mut torrent = Vec::new();
        torrent.extend_from_slice(b"d8:announce23:http://tracker/announce4:info");
        torrent.extend_from_slice(info_dict);
        torrent.push(b'e');

        let expected = sha1_hex(info_dict);
        assert_eq!(compute_torrent_info_hash(&torrent), Some(expected));
    }

    #[test]
    fn computes_info_hash_for_multi_file_torrent_and_rejects_invalid() {
        let info_dict = b"d5:filesld6:lengthi100e4:pathl4:ep01eed6:lengthi200e4:pathl4:ep02eee4:name4:Show12:piece lengthi16384e6:pieces20:abcdefghijklmnopqrste";
        let mut torrent = Vec::new();
        torrent.extend_from_slice(b"d7:comment5:hello4:info");
        torrent.extend_from_slice(info_dict);
        torrent.extend_from_slice(b"7:privatei0ee");

        assert_eq!(
            compute_torrent_info_hash(&torrent),
            Some(sha1_hex(info_dict))
        );
        assert_eq!(compute_torrent_info_hash(b"not-bencode"), None);
        assert_eq!(compute_torrent_info_hash(b"d4:name3:abce"), None);
    }
}
