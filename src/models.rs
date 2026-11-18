use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Stream {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "ytId", skip_serializing_if = "Option::is_none")]
    pub yt_id: Option<String>,
    #[serde(rename = "infoHash", skip_serializing_if = "Option::is_none")]
    pub info_hash: Option<String>,
    #[serde(rename = "fileIdx", skip_serializing_if = "Option::is_none")]
    pub file_idx: Option<u32>,
    #[serde(rename = "externalUrl", skip_serializing_if = "Option::is_none")]
    pub external_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamResponse {
    pub streams: Vec<Stream>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MetaBehaviorHints {
    #[serde(rename = "defaultVideoId", skip_serializing_if = "Option::is_none")]
    pub default_video_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MetaPreview {
    pub id: String,
    #[serde(rename = "type")]
    pub media_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "releaseInfo", skip_serializing_if = "Option::is_none")]
    pub release_info: Option<String>,
    #[serde(rename = "imdbRating", skip_serializing_if = "Option::is_none")]
    pub imdb_rating: Option<String>,
    #[serde(rename = "behaviorHints", skip_serializing_if = "Option::is_none")]
    pub behavior_hints: Option<MetaBehaviorHints>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CatalogResponse {
    pub metas: Vec<MetaPreview>,
}
