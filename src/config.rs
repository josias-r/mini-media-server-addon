use dotenvy::dotenv;
use std::env;

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
        // Load .env if it exists, silently ignore if it doesn't
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
    let numeric_part = if let Some(prefix) = lower.strip_suffix("days") {
        prefix.trim()
    } else if let Some(prefix) = lower.strip_suffix("day") {
        prefix.trim()
    } else if let Some(prefix) = lower.strip_suffix('d') {
        prefix.trim()
    } else {
        cleaned
    };

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
