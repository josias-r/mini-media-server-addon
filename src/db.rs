use log::{error, info};
use rusqlite::{Connection, OptionalExtension, Result, params};
use serde::Serialize;
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

        Ok(())
    }

    pub fn get_admin_password(&self) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = 'admin_password'")?;

        // We have to collect because mapping borrows stmt
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(row.get(0)?)
        } else {
            // Drop stmt before executing another query
            drop(rows);
            drop(stmt);

            // Seed from ADMIN_PASSWORD env var if provided, otherwise generate a random default password
            let env_password = std::env::var("ADMIN_PASSWORD")
                .ok()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty());
            let password = match env_password {
                Some(p) => {
                    info!("Initialized admin password from ADMIN_PASSWORD environment variable");
                    p
                }
                None => {
                    let generated = Uuid::new_v4().to_string().replace("-", "")[..12].to_string();
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
        conn.execute("DELETE FROM users WHERE api_key = ?", params![api_key])?;
        Ok(())
    }

    pub fn validate_api_key(&self, api_key: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare("SELECT 1 FROM users WHERE api_key = ?") {
            Ok(s) => s,
            Err(e) => {
                error!("Database error preparing validate query: {}", e);
                return false;
            }
        };

        stmt.exists(params![api_key]).unwrap_or(false)
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

        let mut map = std::collections::HashMap::new();
        for item in history {
            let (id, time) = item?;
            map.insert(id, time);
        }
        Ok(map)
    }

    pub fn delete_watch_history(&self, item_id: &str) -> Result<()> {
        let item_id = item_id.trim().to_lowercase();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM watch_history WHERE LOWER(item_id) = ?",
            params![item_id],
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
        let mut results = Vec::new();
        for item in rows {
            results.push(item?);
        }
        Ok(results)
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

        let mut results = Vec::new();
        for u in users {
            results.push(u?);
        }
        Ok(results)
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
}
