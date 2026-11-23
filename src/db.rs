use crate::config::Config;
use crate::domain::parse_stremio_media_id;
use log::{error, info};
use rusqlite::{Connection, OptionalExtension, Result, params, params_from_iter};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Serialize, Clone)]
pub struct UserRecord {
    pub username: String,
    pub api_key: String,
    pub created_at: String,
    pub watch_count: u64,
    pub last_watched_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaMetadata {
    pub imdb_id: String,
    pub media_type: String,
    pub name: String,
    pub poster: Option<String>,
    pub background: Option<String>,
    pub logo: Option<String>,
    pub description: Option<String>,
    pub release_info: Option<String>,
    pub imdb_rating: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaHistoryEntry {
    pub imdb_id: String,
    pub media_type: String,
    pub latest_stremio_id: String,
    pub info_hash: String,
    pub last_watched_at: String,
}

#[derive(Clone)]
pub struct DbClient {
    conn: Arc<Mutex<Connection>>,
}

impl DbClient {
    pub fn new(db_path: &str) -> Self {
        let conn = Connection::open(db_path).expect("Failed to open SQLite database");

        let client = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        client
            .init_db()
            .expect("Failed to initialize database tables");
        client
    }

    fn init_db(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Create users table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL DEFAULT 'unknown',
                api_key TEXT UNIQUE NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        // Best-effort schema migration for existing early-dev dbs
        let _ = conn.execute(
            "ALTER TABLE users ADD COLUMN username TEXT NOT NULL DEFAULT 'unknown'",
            [],
        );

        // Create settings table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        // Create watch_history table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS watch_history (
                api_key TEXT NOT NULL,
                item_id TEXT NOT NULL,
                item_type TEXT NOT NULL,
                last_watched_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (api_key, item_id)
            )",
            [],
        )?;

        // Normalize legacy mixed-case item_id rows while preserving the latest timestamp per user/item
        conn.execute(
            "DELETE FROM watch_history
             WHERE rowid NOT IN (
                 SELECT rowid FROM (
                     SELECT rowid, MAX(last_watched_at)
                     FROM watch_history
                     GROUP BY api_key, LOWER(item_id)
                 )
             )",
            [],
        )?;
        conn.execute(
            "UPDATE watch_history SET item_id = LOWER(item_id) WHERE item_id != LOWER(item_id)",
            [],
        )?;

        // Create media_watch_history table for per-user and community catalog tracking
        conn.execute(
            "CREATE TABLE IF NOT EXISTS media_watch_history (
                api_key TEXT NOT NULL,
                stremio_id TEXT NOT NULL,
                imdb_id TEXT NOT NULL,
                media_type TEXT NOT NULL,
                info_hash TEXT NOT NULL,
                last_watched_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                watch_seq INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (api_key, stremio_id)
            )",
            [],
        )?;
        let _ = conn.execute(
            "ALTER TABLE media_watch_history ADD COLUMN watch_seq INTEGER NOT NULL DEFAULT 0",
            [],
        );

        // Create media_metadata cache table for Cinemeta responses
        conn.execute(
            "CREATE TABLE IF NOT EXISTS media_metadata (
                imdb_id TEXT PRIMARY KEY,
                media_type TEXT NOT NULL,
                name TEXT NOT NULL,
                poster TEXT,
                background TEXT,
                logo TEXT,
                description TEXT,
                release_info TEXT,
                imdb_rating TEXT,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        Ok(())
    }

    pub fn get_admin_password(&self) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        if let Some(pw) = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'admin_password'",
                [],
                |row| row.get(0),
            )
            .optional()?
        {
            return Ok(pw);
        }

        // Seed from ADMIN_PASSWORD env var if provided, otherwise generate a random default password
        let password = match Config::admin_password_from_env() {
            Some(p) => {
                info!("Initialized admin password from ADMIN_PASSWORD environment variable");
                p
            }
            None => {
                let generated = Uuid::new_v4().to_string().replace('-', "")[..12].to_string();
                info!("Generated default admin password: {}", generated);
                generated
            }
        };
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('admin_password', ?)",
            params![&password],
        )?;
        Ok(password)
    }

    pub fn set_admin_password(&self, password: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('admin_password', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![password],
        )?;
        Ok(())
    }

    pub fn create_user(&self, username: &str) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let api_key = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (username, api_key) VALUES (?, ?)",
            params![username, &api_key],
        )?;
        Ok(api_key)
    }

    pub fn delete_user(&self, api_key: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM watch_history WHERE api_key = ?",
            params![api_key],
        )?;
        conn.execute(
            "DELETE FROM media_watch_history WHERE api_key = ?",
            params![api_key],
        )?;
        conn.execute("DELETE FROM users WHERE api_key = ?", params![api_key])?;
        Ok(())
    }

    pub fn validate_api_key(&self, api_key: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT 1 FROM users WHERE api_key = ?",
            params![api_key],
            |_| Ok(()),
        )
        .optional()
        .unwrap_or_else(|e| {
            error!("Database error preparing validate query: {}", e);
            None
        })
        .is_some()
    }

    pub fn update_watch_history(
        &self,
        api_key: &str,
        item_id: &str,
        item_type: &str,
    ) -> Result<()> {
        let item_id = item_id.trim().to_lowercase();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO watch_history (api_key, item_id, item_type, last_watched_at) 
             VALUES (?, ?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT(api_key, item_id) 
             DO UPDATE SET last_watched_at = CURRENT_TIMESTAMP",
            params![api_key, item_id, item_type],
        )?;
        Ok(())
    }

    pub fn get_torrent_watch_times(&self) -> Result<std::collections::HashMap<String, String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT LOWER(item_id), MAX(last_watched_at) FROM watch_history GROUP BY LOWER(item_id)",
        )?;
        let history = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        history.collect()
    }

    pub fn delete_watch_history(&self, item_id: &str) -> Result<()> {
        let item_id = item_id.trim().to_lowercase();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM watch_history WHERE LOWER(item_id) = ?",
            params![item_id],
        )?;
        conn.execute(
            "DELETE FROM media_watch_history WHERE LOWER(info_hash) = ?",
            params![item_id],
        )?;
        Ok(())
    }

    pub fn record_media_watch(
        &self,
        api_key: &str,
        stremio_id: &str,
        info_hash: &str,
    ) -> Result<()> {
        let info_hash = info_hash.trim().to_lowercase();

        if !info_hash.is_empty() && info_hash != "unknown" {
            self.update_watch_history(api_key, &info_hash, "torrent")?;
        }

        if let Some((norm_stremio_id, imdb_id, media_type)) = parse_stremio_media_id(stremio_id) {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO media_watch_history (
                    api_key, stremio_id, imdb_id, media_type, info_hash, last_watched_at, watch_seq
                 )
                 VALUES (
                    ?, ?, ?, ?, ?, CURRENT_TIMESTAMP,
                    (SELECT COALESCE(MAX(watch_seq), 0) + 1 FROM media_watch_history)
                 )
                 ON CONFLICT(api_key, stremio_id)
                 DO UPDATE SET
                    info_hash = CASE
                        WHEN excluded.info_hash != '' AND excluded.info_hash != 'unknown'
                        THEN excluded.info_hash
                        ELSE media_watch_history.info_hash
                    END,
                    last_watched_at = CURRENT_TIMESTAMP,
                    watch_seq = (SELECT COALESCE(MAX(watch_seq), 0) + 1 FROM media_watch_history)",
                params![api_key, norm_stremio_id, imdb_id, media_type, &info_hash],
            )?;
        }

        Ok(())
    }

    fn query_catalog_entries(
        &self,
        api_key: Option<&str>,
        media_type: &str,
    ) -> Result<Vec<MediaHistoryEntry>> {
        let conn = self.conn.lock().unwrap();
        let (where_clause, param_vals): (&str, Vec<&str>) = match api_key {
            Some(key) => (
                "WHERE api_key = ? AND media_type = ?",
                vec![key, media_type],
            ),
            None => ("WHERE media_type = ?", vec![media_type]),
        };
        let sql = format!(
            "SELECT imdb_id, media_type, stremio_id, info_hash, last_watched_at
             FROM (
                 SELECT
                     imdb_id,
                     media_type,
                     stremio_id,
                     info_hash,
                     last_watched_at,
                     watch_seq,
                     rowid,
                     ROW_NUMBER() OVER (
                         PARTITION BY imdb_id
                         ORDER BY last_watched_at DESC, watch_seq DESC, rowid DESC
                     ) AS rn
                 FROM media_watch_history
                 {where_clause}
             )
             WHERE rn = 1
             ORDER BY last_watched_at DESC, watch_seq DESC, rowid DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(param_vals), |row| {
            Ok(MediaHistoryEntry {
                imdb_id: row.get(0)?,
                media_type: row.get(1)?,
                latest_stremio_id: row.get(2)?,
                info_hash: row.get(3)?,
                last_watched_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_user_catalog_entries(
        &self,
        api_key: &str,
        media_type: &str,
    ) -> Result<Vec<MediaHistoryEntry>> {
        self.query_catalog_entries(Some(api_key), media_type)
    }

    pub fn get_community_catalog_entries(
        &self,
        media_type: &str,
    ) -> Result<Vec<MediaHistoryEntry>> {
        self.query_catalog_entries(None, media_type)
    }

    pub fn get_media_metadata(&self, imdb_id: &str) -> Result<Option<MediaMetadata>> {
        let imdb_id = imdb_id.trim().to_lowercase();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT imdb_id, media_type, name, poster, background, logo, description, release_info, imdb_rating
             FROM media_metadata
             WHERE imdb_id = ?",
        )?;
        stmt.query_row(params![imdb_id], |row| {
            Ok(MediaMetadata {
                imdb_id: row.get(0)?,
                media_type: row.get(1)?,
                name: row.get(2)?,
                poster: row.get(3)?,
                background: row.get(4)?,
                logo: row.get(5)?,
                description: row.get(6)?,
                release_info: row.get(7)?,
                imdb_rating: row.get(8)?,
            })
        })
        .optional()
    }

    pub fn upsert_media_metadata(&self, meta: &MediaMetadata) -> Result<()> {
        let imdb_id = meta.imdb_id.trim().to_lowercase();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO media_metadata (
                imdb_id, media_type, name, poster, background, logo, description, release_info, imdb_rating, updated_at
             )
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT(imdb_id)
             DO UPDATE SET
                media_type = excluded.media_type,
                name = excluded.name,
                poster = excluded.poster,
                background = excluded.background,
                logo = excluded.logo,
                description = excluded.description,
                release_info = excluded.release_info,
                imdb_rating = excluded.imdb_rating,
                updated_at = CURRENT_TIMESTAMP",
            params![
                imdb_id,
                meta.media_type,
                meta.name,
                meta.poster,
                meta.background,
                meta.logo,
                meta.description,
                meta.release_info,
                meta.imdb_rating
            ],
        )?;
        Ok(())
    }

    pub fn get_user_by_key(&self, api_key: &str) -> Result<Option<(String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT username, api_key, created_at FROM users WHERE api_key = ?")?;
        stmt.query_row(params![api_key], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .optional()
    }

    pub fn get_user_watch_history(&self, api_key: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT LOWER(item_id), MAX(last_watched_at) as last_watched
             FROM watch_history
             WHERE api_key = ?
             GROUP BY LOWER(item_id)
             ORDER BY last_watched DESC",
        )?;
        let rows = stmt.query_map(params![api_key], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }

    pub fn get_users_with_stats(&self) -> Result<Vec<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT u.username, u.api_key, u.created_at, COUNT(DISTINCT LOWER(w.item_id)), MAX(w.last_watched_at)
             FROM users u
             LEFT JOIN watch_history w ON u.api_key = w.api_key
             GROUP BY u.id
             ORDER BY u.created_at DESC",
        )?;
        let users = stmt.query_map([], |row| {
            Ok(UserRecord {
                username: row.get(0)?,
                api_key: row.get(1)?,
                created_at: row.get(2)?,
                watch_count: row.get::<_, i64>(3)? as u64,
                last_watched_at: row.get(4)?,
            })
        })?;
        users.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_history_normalizes_hash_case_on_update_query_and_delete() {
        let db = DbClient::new(":memory:");
        let api_key = db.create_user("alice").unwrap();
        let upper_hash = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        let lower_hash = upper_hash.to_lowercase();

        db.update_watch_history(&api_key, upper_hash, "torrent")
            .unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "UPDATE watch_history SET last_watched_at = '2020-01-01 00:00:00'",
                [],
            )
            .unwrap();
        }

        db.update_watch_history(&api_key, &format!("  {}  ", lower_hash), "torrent")
            .unwrap();

        {
            let conn = db.conn.lock().unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM watch_history", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 1);

            let stored_id: String = conn
                .query_row("SELECT item_id FROM watch_history", [], |r| r.get(0))
                .unwrap();
            assert_eq!(stored_id, lower_hash);
        }

        let watch_times = db.get_torrent_watch_times().unwrap();
        assert_eq!(watch_times.len(), 1);
        assert_ne!(watch_times.get(&lower_hash).unwrap(), "2020-01-01 00:00:00");

        let user_history = db.get_user_watch_history(&api_key).unwrap();
        assert_eq!(user_history.len(), 1);
        assert_eq!(user_history[0].0, lower_hash);

        let users = db.get_users_with_stats().unwrap();
        assert_eq!(users[0].watch_count, 1);

        db.delete_watch_history(upper_hash).unwrap();
        assert!(db.get_torrent_watch_times().unwrap().is_empty());
    }

    #[test]
    fn init_db_deduplicates_legacy_mixed_case_watch_history() {
        let db = DbClient::new(":memory:");
        let api_key = db.create_user("bob").unwrap();
        let upper_hash = "DEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF";
        let lower_hash = upper_hash.to_lowercase();

        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO watch_history (api_key, item_id, item_type, last_watched_at)
                 VALUES (?, ?, 'torrent', '2024-01-01 00:00:00')",
                params![&api_key, upper_hash],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO watch_history (api_key, item_id, item_type, last_watched_at)
                 VALUES (?, ?, 'torrent', '2025-06-15 12:30:00')",
                params![&api_key, &lower_hash],
            )
            .unwrap();
        }

        db.init_db().unwrap();

        let watch_times = db.get_torrent_watch_times().unwrap();
        assert_eq!(watch_times.len(), 1);
        assert_eq!(
            watch_times.get(&lower_hash).map(String::as_str),
            Some("2025-06-15 12:30:00")
        );

        let conn = db.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM watch_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn records_media_watch_dedups_series_orders_catalogs_and_cleans_up() {
        let db = DbClient::new(":memory:");
        let alice = db.create_user("alice").unwrap();
        let bob = db.create_user("bob").unwrap();

        let season_pack_hash = "AAAA1111BBBB2222CCCC3333DDDD4444EEEE5555";
        let movie_hash = "1111222233334444555566667777888899990000";
        let other_series_hash = "FFFF9999EEEE8888DDDD7777CCCC6666BBBB5555";

        // Alice watches two episodes from the same season pack, plus another series and a movie
        db.record_media_watch(&alice, "tt0903747:1:1", season_pack_hash)
            .unwrap();
        db.record_media_watch(&alice, "tt0903747:1:2", "unknown")
            .unwrap();
        db.record_media_watch(&alice, "tt0944947:1:1", other_series_hash)
            .unwrap();
        // Re-watch S01E01 of tt0903747 after watching tt0944947 -> tt0903747 should move to top and pick 1:1
        db.record_media_watch(&alice, "tt0903747:1:1", season_pack_hash)
            .unwrap();
        db.record_media_watch(&alice, "tt1375666", movie_hash)
            .unwrap();

        // Bob watches a different movie after Alice
        db.record_media_watch(
            &bob,
            "tt0816692",
            "aaaa0000bbbb1111cccc2222dddd3333eeee4444",
        )
        .unwrap();

        // Alice's series catalog has 2 deduplicated series, with tt0903747 first and latest episode 1:1
        let alice_series = db.get_user_catalog_entries(&alice, "series").unwrap();
        assert_eq!(alice_series.len(), 2);
        assert_eq!(alice_series[0].imdb_id, "tt0903747");
        assert_eq!(alice_series[0].latest_stremio_id, "tt0903747:1:1");
        assert_eq!(alice_series[0].info_hash, season_pack_hash.to_lowercase());
        assert_eq!(alice_series[1].imdb_id, "tt0944947");

        // Bob's user series catalog is empty; his movie catalog has only his movie
        assert!(
            db.get_user_catalog_entries(&bob, "series")
                .unwrap()
                .is_empty()
        );
        let bob_movies = db.get_user_catalog_entries(&bob, "movie").unwrap();
        assert_eq!(bob_movies.len(), 1);
        assert_eq!(bob_movies[0].imdb_id, "tt0816692");

        // Community movie catalog has Bob's movie (most recent) then Alice's movie
        let community_movies = db.get_community_catalog_entries("movie").unwrap();
        assert_eq!(community_movies.len(), 2);
        assert_eq!(community_movies[0].imdb_id, "tt0816692");
        assert_eq!(community_movies[1].imdb_id, "tt1375666");

        // Deleting Alice's season pack hash clears all episodes tied to that hash
        db.delete_watch_history(season_pack_hash).unwrap();
        let alice_series_after_torrent_del = db.get_user_catalog_entries(&alice, "series").unwrap();
        assert_eq!(alice_series_after_torrent_del.len(), 2); // tt0903747:1:2 had "unknown" hash, 1:1 was deleted
        assert_eq!(alice_series_after_torrent_del[0].imdb_id, "tt0944947");
        assert_eq!(
            alice_series_after_torrent_del[1].latest_stremio_id,
            "tt0903747:1:2"
        );

        // Deleting user removes all their media watch history
        db.delete_user(&alice).unwrap();
        assert!(
            db.get_user_catalog_entries(&alice, "movie")
                .unwrap()
                .is_empty()
        );
        assert!(
            db.get_community_catalog_entries("series")
                .unwrap()
                .is_empty()
        );
        assert_eq!(db.get_community_catalog_entries("movie").unwrap().len(), 1);
    }

    #[test]
    fn upserts_and_fetches_media_metadata_case_insensitively() {
        let db = DbClient::new(":memory:");
        let meta = MediaMetadata {
            imdb_id: "TT1375666".to_string(),
            media_type: "movie".to_string(),
            name: "Inception".to_string(),
            poster: Some("https://images.metahub.space/poster/medium/tt1375666/img".to_string()),
            background: Some(
                "https://images.metahub.space/background/medium/tt1375666/img".to_string(),
            ),
            logo: None,
            description: Some("A thief steals corporate secrets.".to_string()),
            release_info: Some("2010".to_string()),
            imdb_rating: Some("8.8".to_string()),
        };

        db.upsert_media_metadata(&meta).unwrap();
        let loaded = db.get_media_metadata(" tt1375666 ").unwrap().unwrap();
        assert_eq!(loaded.imdb_id, "tt1375666");
        assert_eq!(loaded.name, "Inception");
        assert_eq!(loaded.imdb_rating.as_deref(), Some("8.8"));

        let updated = MediaMetadata {
            name: "Inception (2010)".to_string(),
            ..meta
        };
        db.upsert_media_metadata(&updated).unwrap();
        let reloaded = db.get_media_metadata("tt1375666").unwrap().unwrap();
        assert_eq!(reloaded.name, "Inception (2010)");
    }
}
