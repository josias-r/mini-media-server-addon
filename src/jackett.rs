use crate::config::Config;
use log::{debug, error};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct JackettResult {
    pub title: String,
    pub tracker: String,
    // pub tracker_id: String,
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
    // #[serde(rename = "@id")]
    // id: Option<String>,
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
            let magnet_uri = if let Some(url) = enclosure_url.clone() {
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

            for attr in item.attrs {
                if let (Some(name), Some(value)) = (attr.name, attr.value) {
                    if name == "seeders" {
                        seeders = value.parse().unwrap_or(0);
                    } else if name == "infohash" {
                        info_hash = Some(value);
                    }
                }
            }

            // Fallback: Try to extract info_hash from magnet_uri if missing
            if info_hash.is_none()
                && let Some(ref magnet) = magnet_uri
                && magnet.starts_with("magnet:")
                && let Some(start) = magnet.find("urn:btih:")
            {
                let hash_part = &magnet[start + 9..];
                let end = hash_part.find('&').unwrap_or(hash_part.len());
                info_hash = Some(hash_part[..end].to_string());
            }

            // let mut tracker_id = String::new();
            let mut tracker_name = "Unknown".to_string();

            if let Some(indexer) = item.jackettindexer {
                // tracker_id = indexer.id.unwrap_or_default();
                tracker_name = indexer.name.unwrap_or_else(|| "Unknown".to_string());
            }

            if seeders > 0 {
                results.push(JackettResult {
                    title,
                    tracker: tracker_name,
                    // tracker_id,
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
