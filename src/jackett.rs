use crate::config::Config;
use log::{debug, error};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct JackettResult {
    pub title: String,
    pub tracker: String,
    pub link: Option<String>,
    pub magnet_uri: Option<String>,
    pub info_hash: Option<String>,
    pub seeders: u32,
    pub size: u64,
}

pub enum TorznabParams {
    MovieImdb {
        imdb_id: String,
    },
    MovieText {
        query: String,
    },
    SeriesImdb {
        imdb_id: String,
        season: u32,
        episode: u32,
    },
    SeriesText {
        query: String,
        season: u32,
        episode: u32,
    },
    SeriesSeasonImdb {
        imdb_id: String,
        season: u32,
    },
    SeriesSeasonText {
        query: String,
        season: u32,
    },
}

fn build_query_string(config: &Config, params: &TorznabParams) -> String {
    let mut qs = format!("apikey={}", config.jackett_api_key);
    match params {
        TorznabParams::MovieImdb { imdb_id } => {
            qs.push_str(&format!("&t=movie&imdbid={}", imdb_id));
        }
        TorznabParams::MovieText { query } => {
            qs.push_str(&format!("&t=movie&q={}", urlencoding::encode(query)));
        }
        TorznabParams::SeriesImdb {
            imdb_id,
            season,
            episode,
        } => {
            qs.push_str(&format!(
                "&t=tvsearch&imdbid={}&season={}&ep={}",
                imdb_id, season, episode
            ));
        }
        TorznabParams::SeriesText {
            query,
            season,
            episode,
        } => {
            let q = format!("{} S{:02}E{:02}", query, season, episode);
            qs.push_str(&format!("&t=search&q={}", urlencoding::encode(&q)));
        }
        TorznabParams::SeriesSeasonImdb { imdb_id, season } => {
            qs.push_str(&format!("&t=tvsearch&imdbid={}&season={}", imdb_id, season));
        }
        TorznabParams::SeriesSeasonText { query, season } => {
            let q = format!("{} S{:02}", query, season);
            qs.push_str(&format!("&t=search&q={}", urlencoding::encode(&q)));
        }
    }
    qs
}

// Quick-XML serde structs
#[derive(Debug, Deserialize)]
struct Rss {
    channel: Option<Channel>,
}

#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(rename = "item", default)]
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    title: Option<String>,
    size: Option<u64>,
    link: Option<String>,
    enclosure: Option<Enclosure>,
    #[serde(rename = "attr", default)]
    attrs: Vec<Attr>,
    jackettindexer: Option<JackettIndexer>,
}

#[derive(Debug, Deserialize)]
struct Enclosure {
    #[serde(rename = "@url")]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Attr {
    #[serde(rename = "@name")]
    name: Option<String>,
    #[serde(rename = "@value")]
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JackettIndexer {
    #[serde(rename = "$value")]
    name: Option<String>,
}

pub async fn fetch_jackett_results(
    client: &Client,
    config: &Config,
    params: &TorznabParams,
) -> Vec<JackettResult> {
    let qs = build_query_string(config, params);
    let url = format!(
        "{}/api/v2.0/indexers/all/results/torznab/api?{}",
        config.jackett_url, qs
    );

    debug!("Fetching from Jackett: {}", url);
    let response = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            error!("Jackett API responded with status: {}", r.status());
            return vec![];
        }
        Err(e) => {
            error!("Failed to fetch from Jackett API: {}", e);
            return vec![];
        }
    };

    let xml = match response.text().await {
        Ok(t) => t,
        Err(_) => return vec![],
    };

    let rss: Rss = match quick_xml::de::from_str(&xml) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to parse Jackett XML: {}", e);
            error!("Raw XML was: {}", xml);
            return vec![];
        }
    };

    let mut results = Vec::new();

    if let Some(channel) = rss.channel {
        if channel.items.is_empty() {
            debug!("Jackett returned a channel, but no items. Raw XML: {}", xml);
        }
        for item in channel.items {
            let title = item.title.unwrap_or_else(|| "Unknown".to_string());
            let size = item.size.unwrap_or(0);
            let link = item.link.clone();

            let enclosure_url = item.enclosure.and_then(|e| e.url);
            let mut magnet_uri = if let Some(url) = enclosure_url.clone() {
                if url.starts_with("magnet:") {
                    Some(url)
                } else {
                    link.clone().or(Some(url))
                }
            } else {
                link.clone()
            };

            let mut seeders = 0;
            let mut info_hash = None;
            let mut magnet_attr = None;

            for attr in item.attrs {
                if let (Some(name), Some(value)) = (attr.name, attr.value) {
                    if name == "seeders" {
                        seeders = value.parse().unwrap_or(0);
                    } else if name == "infohash" {
                        let trimmed = value.trim().to_lowercase();
                        if !trimmed.is_empty() {
                            info_hash = Some(trimmed);
                        }
                    } else if name == "magneturl" {
                        let trimmed = value.trim();
                        if trimmed.starts_with("magnet:") {
                            magnet_attr = Some(trimmed.to_string());
                        }
                    }
                }
            }

            if let Some(m) = magnet_attr
                && !magnet_uri
                    .as_deref()
                    .is_some_and(|u| u.starts_with("magnet:"))
            {
                magnet_uri = Some(m);
            }

            // Fallback: Try to extract info_hash from magnet_uri if missing
            if info_hash.is_none()
                && let Some(ref magnet) = magnet_uri
                && magnet.starts_with("magnet:")
            {
                info_hash = extract_btih_hex(magnet);
            }

            let mut tracker_name = "Unknown".to_string();

            if let Some(indexer) = item.jackettindexer {
                tracker_name = indexer.name.unwrap_or_else(|| "Unknown".to_string());
            }

            if seeders > 0 {
                results.push(JackettResult {
                    title,
                    tracker: tracker_name,
                    link,
                    magnet_uri,
                    info_hash,
                    seeders,
                    size,
                });
            }
        }
        debug!(
            "Jackett XML parsed successfully. Found {} items.",
            results.len()
        );
    } else {
        debug!("Jackett XML did not contain a <channel>. Raw XML: {}", xml);
    }

    results.sort_by_key(|b| std::cmp::Reverse(b.seeders));
    results
}

fn decode_base32_btih(encoded: &str) -> Option<String> {
    if encoded.len() != 32 {
        return None;
    }

    let mut bytes = [0u8; 20];
    let mut buffer: u32 = 0;
    let mut bits_left: u8 = 0;
    let mut out_idx = 0;

    for b in encoded.bytes() {
        let upper = b.to_ascii_uppercase();
        let val = match upper {
            b'A'..=b'Z' => upper - b'A',
            b'2'..=b'7' => upper - b'2' + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | u32::from(val);
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            bytes[out_idx] = ((buffer >> bits_left) & 0xff) as u8;
            out_idx += 1;
        }
    }

    let mut hex = String::with_capacity(40);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Some(hex)
}

/// Extracts a 40-character lowercase hex BitTorrent info_hash from a URI containing `urn:btih:`.
/// Supports both 40-character hex and 32-character RFC 4648 Base32 hashes.
pub fn extract_btih_hex(uri: &str) -> Option<String> {
    let lower = uri.to_ascii_lowercase();
    let start = lower.find("urn:btih:")?;
    let hash_part = &uri[start + 9..];
    let end = hash_part.find('&').unwrap_or(hash_part.len());
    let candidate = hash_part[..end].trim();

    if candidate.len() == 40 && candidate.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(candidate.to_lowercase());
    }

    if candidate.len() == 32 {
        return decode_base32_btih(candidate);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_hex_btih_case_insensitively() {
        let magnet = "magnet:?xt=URN:BTIH:BA0B39DE2C19CEADCFB1B441D8B6D668E57458EF&dn=Avatar";
        assert_eq!(
            extract_btih_hex(magnet).as_deref(),
            Some("ba0b39de2c19ceadcfb1b441d8b6d668e57458ef")
        );
    }

    #[test]
    fn extracts_and_decodes_base32_btih() {
        let magnet = "magnet:?xt=urn:btih:CWWVGHCJO23WHPWLZ7ZHDLR7KD4NXPQV&dn=Test";
        let hex = extract_btih_hex(magnet).unwrap();
        assert_eq!(hex.len(), 40);
        assert_eq!(hex, "15ad531c4976b763becbcff271ae3f50f8dbbe15");
    }

    #[test]
    fn rejects_invalid_btih() {
        assert_eq!(extract_btih_hex("https://example.com/file.torrent"), None);
        assert_eq!(extract_btih_hex("magnet:?xt=urn:btih:tooshort"), None);
        assert_eq!(
            extract_btih_hex("magnet:?xt=urn:btih:11111111111111111111111111111111"),
            None
        );
    }
}
