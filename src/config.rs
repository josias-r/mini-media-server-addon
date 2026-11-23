use crate::db::DbClient;
use crate::placeholder::{DEFAULT_DURATION_HOURS, LiveSessionStore, PlaceholderBank};
use crate::qbittorrent::QbitClient;
use dotenvy::dotenv;
use log::info;
use reqwest::Client;
use std::collections::HashSet;
use std::env;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub public_url: String,
    pub jackett_url: String,
    pub jackett_api_key: String,
    pub jackett_search_type: String, // "imdb" or "text"
    pub qbittorrent_url: String,
    pub qbittorrent_username: String,
    pub qbittorrent_password: String,
    pub download_path: String,
    pub retention_days: u64,
}

impl Config {
    pub fn load() -> Self {
        let _ = dotenv();

        Self {
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .trim()
                .parse()
                .expect("PORT must be a number"),
            public_url: get_req_env("PUBLIC_URL"),
            jackett_url: get_req_env("JACKETT_URL"),
            jackett_api_key: get_req_env("JACKETT_API_KEY"),
            jackett_search_type: get_req_env("JACKETT_SEARCH_TYPE"),
            qbittorrent_url: get_req_env("QBITTORRENT_URL"),
            qbittorrent_username: get_req_env("QBITTORRENT_USERNAME"),
            qbittorrent_password: get_req_env("QBITTORRENT_PASSWORD"),
            download_path: get_req_env("DOWNLOAD_PATH"),
            retention_days: load_retention_days(),
        }
    }

    pub fn static_dir() -> String {
        env::var("STATIC_DIR").unwrap_or_else(|_| "app/dist".to_string())
    }

    pub fn admin_password_from_env() -> Option<String> {
        env::var("ADMIN_PASSWORD")
            .ok()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub qbit: Arc<QbitClient>,
    pub http_client: Client,
    pub db: Arc<DbClient>,
    pub admin_session_token: String,
    pub triggered_torrents: Arc<RwLock<HashSet<String>>>,
    pub placeholder_bank: Option<Arc<PlaceholderBank>>,
    pub live_sessions: Arc<LiveSessionStore>,
}

impl AppState {
    /// Initializes shared application state from loaded configuration.
    pub fn from_config(config: Config) -> Self {
        let qbit = QbitClient::new(config.clone());

        let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| ".".to_string());
        let db = DbClient::new(&format!("{}/data.db", data_dir));
        let _ = db.get_admin_password().unwrap();

        let placeholders_dir =
            env::var("PLACEHOLDERS_DIR").unwrap_or_else(|_| "placeholders".to_string());
        let duration_hours = env::var("PLACEHOLDER_DURATION_HOURS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_DURATION_HOURS);
        let placeholder_bank = match PlaceholderBank::load(
            Path::new(&placeholders_dir),
            duration_hours,
        ) {
            Ok(bank) => {
                info!(
                    "Loaded virtual placeholder bank from {:?} (slot_size={} B, total_len={} MiB, fragments={})",
                    placeholders_dir,
                    bank.slot_size,
                    bank.total_len / (1024 * 1024),
                    bank.total_fragments
                );
                Some(Arc::new(bank))
            }
            Err(e) => {
                log::warn!(
                    "Could not load placeholder bank from {:?}: {}; falling back to static placeholder.mp4",
                    placeholders_dir,
                    e
                );
                None
            }
        };

        Self {
            config: Arc::new(config),
            qbit: Arc::new(qbit),
            http_client: Client::builder().build().unwrap(),
            db: Arc::new(db),
            admin_session_token: uuid::Uuid::new_v4().to_string(),
            triggered_torrents: Arc::new(RwLock::new(HashSet::new())),
            placeholder_bank,
            live_sessions: Arc::new(LiveSessionStore::default()),
        }
    }
}

pub fn parse_retention_days_from(raw: Option<&str>) -> Result<u64, String> {
    let Some(raw_val) = raw else {
        return Ok(0);
    };

    let cleaned = raw_val.trim().trim_matches(['"', '\'']).trim();
    if cleaned.is_empty() {
        return Ok(0);
    }

    let lower = cleaned.to_ascii_lowercase();
    let numeric_part = lower
        .strip_suffix("days")
        .or_else(|| lower.strip_suffix("day"))
        .or_else(|| lower.strip_suffix('d'))
        .map(str::trim)
        .unwrap_or(cleaned);

    if numeric_part.is_empty() {
        return Err(raw_val.to_string());
    }

    numeric_part.parse::<u64>().map_err(|_| raw_val.to_string())
}

fn load_retention_days() -> u64 {
    let from_primary = env::var("RETENTION_DAYS")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let (env_name, raw_opt) = match from_primary {
        Some(val) => ("RETENTION_DAYS", Some(val)),
        None => match env::var("RETENTION").ok().filter(|v| !v.trim().is_empty()) {
            Some(val) => ("RETENTION", Some(val)),
            None => ("RETENTION_DAYS", None),
        },
    };

    match parse_retention_days_from(raw_opt.as_deref()) {
        Ok(days) => days,
        Err(bad) => {
            log::warn!(
                "Invalid {} value {:?}, falling back to 0 (unlimited)",
                env_name,
                bad
            );
            0
        }
    }
}

fn get_req_env(name: &str) -> String {
    env::var(name)
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|_| panic!("Environment variable {} is required", name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_retention_days_variants() {
        assert_eq!(parse_retention_days_from(Some("90")), Ok(90));
        assert_eq!(parse_retention_days_from(Some(" 90\n")), Ok(90));
        assert_eq!(parse_retention_days_from(Some("\"90\"")), Ok(90));
        assert_eq!(parse_retention_days_from(Some("'90'")), Ok(90));
        assert_eq!(parse_retention_days_from(Some("90d")), Ok(90));
        assert_eq!(parse_retention_days_from(Some("90 days")), Ok(90));
        assert_eq!(parse_retention_days_from(Some("1 day")), Ok(1));
        assert_eq!(parse_retention_days_from(Some("0")), Ok(0));
        assert_eq!(parse_retention_days_from(Some("")), Ok(0));
        assert_eq!(parse_retention_days_from(Some("   ")), Ok(0));
        assert_eq!(parse_retention_days_from(None), Ok(0));
    }

    #[test]
    fn rejects_invalid_retention_days_values() {
        assert!(parse_retention_days_from(Some("abc")).is_err());
        assert!(parse_retention_days_from(Some("-1")).is_err());
        assert!(parse_retention_days_from(Some("days")).is_err());
        assert!(parse_retention_days_from(Some("90 weeks")).is_err());
    }
}
