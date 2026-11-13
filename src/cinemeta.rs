use log::error;
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct MetaResponse {
    meta: Option<Meta>,
}

#[derive(Debug, Deserialize)]
struct Meta {
    name: Option<String>,
}

pub async fn get_cinemeta_title(
    client: &Client,
    media_type: &str,
    imdb_id: &str,
) -> Option<String> {
    let url = format!(
        "https://v3-cinemeta.strem.io/meta/{}/{}.json",
        media_type, imdb_id
    );

    match client.get(&url).send().await {
        Ok(res) if res.status().is_success() => {
            if let Ok(data) = res.json::<MetaResponse>().await {
                if let Some(meta) = data.meta {
                    return meta.name;
                }
            } else {
                error!("Failed to parse Cinemeta JSON for {}", imdb_id);
            }
        }
        Ok(res) => {
            error!(
                "Cinemeta API responded with status: {} for {}",
                res.status(),
                imdb_id
            );
        }
        Err(e) => {
            error!("Failed to fetch title from Cinemeta for {}: {}", imdb_id, e);
        }
    }
    None
}
