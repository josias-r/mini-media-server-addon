use crate::db::{DbClient, MediaMetadata};
use log::error;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct MetaResponse {
    meta: Option<Meta>,
}

#[derive(Debug, Deserialize)]
struct Meta {
    name: Option<String>,
    poster: Option<String>,
    background: Option<String>,
    logo: Option<String>,
    description: Option<String>,
    #[serde(rename = "releaseInfo")]
    release_info: Option<serde_json::Value>,
    #[serde(rename = "imdbRating")]
    imdb_rating: Option<serde_json::Value>,
}

pub fn metahub_poster_url(imdb_id: &str) -> String {
    format!(
        "https://images.metahub.space/poster/medium/{}/img",
        imdb_id.trim().to_lowercase()
    )
}

pub fn metahub_background_url(imdb_id: &str) -> String {
    format!(
        "https://images.metahub.space/background/medium/{}/img",
        imdb_id.trim().to_lowercase()
    )
}

pub fn metahub_logo_url(imdb_id: &str) -> String {
    format!(
        "https://images.metahub.space/logo/medium/{}/img",
        imdb_id.trim().to_lowercase()
    )
}

fn non_empty_string(opt: Option<String>) -> Option<String> {
    opt.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn value_to_non_empty_string(opt: Option<serde_json::Value>) -> Option<String> {
    match opt {
        Some(serde_json::Value::String(s)) => non_empty_string(Some(s)),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

pub async fn fetch_cinemeta_metadata(
    client: &Client,
    media_type: &str,
    imdb_id: &str,
) -> Option<MediaMetadata> {
    let norm_id = imdb_id.trim().to_lowercase();
    if norm_id.is_empty() {
        return None;
    }

    let url = format!(
        "https://v3-cinemeta.strem.io/meta/{}/{}.json",
        media_type, norm_id
    );

    match client
        .get(&url)
        .timeout(Duration::from_secs(4))
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            if let Ok(data) = res.json::<MetaResponse>().await {
                if let Some(meta) = data.meta
                    && let Some(name) = non_empty_string(meta.name)
                {
                    return Some(MediaMetadata {
                        imdb_id: norm_id.clone(),
                        media_type: media_type.to_string(),
                        name,
                        poster: non_empty_string(meta.poster)
                            .or_else(|| Some(metahub_poster_url(&norm_id))),
                        background: non_empty_string(meta.background)
                            .or_else(|| Some(metahub_background_url(&norm_id))),
                        logo: non_empty_string(meta.logo)
                            .or_else(|| Some(metahub_logo_url(&norm_id))),
                        description: non_empty_string(meta.description),
                        release_info: value_to_non_empty_string(meta.release_info),
                        imdb_rating: value_to_non_empty_string(meta.imdb_rating),
                    });
                }
            } else {
                error!("Failed to parse Cinemeta JSON for {}", norm_id);
            }
        }
        Ok(res) => {
            error!(
                "Cinemeta API responded with status: {} for {}",
                res.status(),
                norm_id
            );
        }
        Err(e) => {
            error!(
                "Failed to fetch metadata from Cinemeta for {}: {}",
                norm_id, e
            );
        }
    }
    None
}

pub async fn resolve_media_metadata(
    client: &Client,
    db: &DbClient,
    media_type: &str,
    imdb_id: &str,
) -> Option<MediaMetadata> {
    let norm_id = imdb_id.trim().to_lowercase();
    if norm_id.is_empty() {
        return None;
    }

    if let Ok(Some(cached)) = db.get_media_metadata(&norm_id) {
        return Some(cached);
    }

    if let Some(fetched) = fetch_cinemeta_metadata(client, media_type, &norm_id).await {
        let _ = db.upsert_media_metadata(&fetched);
        return Some(fetched);
    }

    None
}

pub async fn get_or_fetch_cinemeta_title(
    client: &Client,
    db: &DbClient,
    media_type: &str,
    imdb_id: &str,
) -> Option<String> {
    resolve_media_metadata(client, db, media_type, imdb_id)
        .await
        .map(|m| m.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cinemeta_payload_with_string_or_numeric_rating() {
        let raw = serde_json::json!({
            "meta": {
                "name": "Inception",
                "poster": "https://example.com/poster.jpg",
                "background": "",
                "logo": null,
                "description": "Mind heist",
                "releaseInfo": 2010,
                "imdbRating": 8.8
            }
        });
        let parsed: MetaResponse = serde_json::from_value(raw).unwrap();
        let meta = parsed.meta.unwrap();
        assert_eq!(non_empty_string(meta.name).as_deref(), Some("Inception"));
        assert_eq!(
            value_to_non_empty_string(meta.release_info).as_deref(),
            Some("2010")
        );
        assert_eq!(
            value_to_non_empty_string(meta.imdb_rating).as_deref(),
            Some("8.8")
        );
        assert_eq!(non_empty_string(meta.background), None);
    }

    #[test]
    fn builds_deterministic_metahub_urls() {
        assert_eq!(
            metahub_poster_url("TT1375666"),
            "https://images.metahub.space/poster/medium/tt1375666/img"
        );
        assert_eq!(
            metahub_background_url("tt1375666"),
            "https://images.metahub.space/background/medium/tt1375666/img"
        );
        assert_eq!(
            metahub_logo_url("tt1375666"),
            "https://images.metahub.space/logo/medium/tt1375666/img"
        );
    }
}
