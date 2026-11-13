use crate::routes::AppState;
use chrono::{DateTime, NaiveDateTime, Utc};
use std::time::Duration;

fn should_delete_torrent(
    hash: &str,
    added_on: u64,
    last_watched: Option<&str>,
    retention_days: u64,
    now_utc: DateTime<Utc>,
) -> Option<String> {
    if retention_days == 0 {
        return None;
    }

    let retention_secs = retention_days as i64 * 86_400;

    if let Some(last_watched) = last_watched
        && let Ok(parsed_time) = NaiveDateTime::parse_from_str(last_watched, "%Y-%m-%d %H:%M:%S")
    {
        let duration = now_utc.naive_utc().signed_duration_since(parsed_time);
        if duration.num_seconds() >= retention_secs {
            return Some(format!(
                "Torrent {} is stale (last watched {} days ago). Scheduling deletion.",
                hash,
                duration.num_days()
            ));
        }
        return None;
    }

    if added_on > 0 {
        let duration_seconds = now_utc.timestamp() - added_on as i64;
        if duration_seconds >= retention_secs {
            return Some(format!(
                "Torrent {} has no watch history and was added {} days ago. Scheduling deletion.",
                hash,
                duration_seconds / 86_400
            ));
        }
    }

    None
}

async fn wait_for_qbit_ready(state: &AppState) -> bool {
    let max_attempts = 12;
    for attempt in 1..=max_attempts {
        if state.qbit.check_health().await {
            return true;
        }
        if attempt < max_attempts {
            log::debug!(
                "Waiting for qBittorrent to become ready before retention check (attempt {}/{})...",
                attempt,
                max_attempts
            );
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
    false
}

pub fn start_retention_worker(state: AppState) {
    if state.config.retention_days > 0 {
        log::info!(
            "Starting background retention worker (Policy: {} days inactivity before deletion)",
            state.config.retention_days
        );
        let retention_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // check every hour
            loop {
                interval.tick().await;
                log::debug!("Running retention policy check...");

                if !wait_for_qbit_ready(&retention_state).await {
                    log::warn!("Skipping retention policy check: qBittorrent is not reachable.");
                    continue;
                }

                let torrents = retention_state.qbit.get_all_torrents().await;
                let watch_times = match retention_state.db.get_torrent_watch_times() {
                    Ok(times) => times,
                    Err(e) => {
                        log::error!("Failed to get watch times for retention check: {}", e);
                        continue;
                    }
                };

                let now = Utc::now();
                let mut deleted_count = 0;

                for torrent in &torrents {
                    let hash = torrent.hash.to_lowercase();
                    let last_watched = watch_times.get(&hash).map(String::as_str);

                    if let Some(reason) = should_delete_torrent(
                        &hash,
                        torrent.added_on,
                        last_watched,
                        retention_state.config.retention_days,
                        now,
                    ) {
                        log::debug!("{}", reason);
                        if retention_state.qbit.delete_torrent(&hash, true).await {
                            deleted_count += 1;
                            let _ = retention_state.db.delete_watch_history(&hash);
                        } else {
                            log::warn!("Failed to delete stale torrent {} from qBittorrent", hash);
                        }
                    }
                }

                if deleted_count > 0 {
                    log::info!(
                        "Retention policy check finished: found and deleted {} stale torrent(s).",
                        deleted_count
                    );
                } else {
                    log::debug!("Retention policy check finished: no stale torrents found.");
                }
            }
        });
    } else {
        log::info!(
            "Running without retention policy, no auto-deletion of torrents is configured (RETENTION_DAYS)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, sec)
            .unwrap()
    }

    #[test]
    fn keeps_torrent_with_recent_watch_even_if_added_long_ago() {
        let now = utc(2026, 11, 13, 23, 0, 0);
        let old_added_on = utc(2026, 1, 1, 0, 0, 0).timestamp() as u64;

        let res =
            should_delete_torrent("abc123", old_added_on, Some("2026-11-10 12:00:00"), 7, now);
        assert!(res.is_none());
    }

    #[test]
    fn deletes_torrent_with_stale_watch_history() {
        let now = utc(2026, 11, 13, 23, 0, 0);
        let old_added_on = utc(2026, 1, 1, 0, 0, 0).timestamp() as u64;

        let res =
            should_delete_torrent("abc123", old_added_on, Some("2026-11-01 12:00:00"), 7, now);
        assert!(res.is_some());
    }

    #[test]
    fn deletes_unwatched_old_torrent_on_fresh_boot() {
        let now = utc(2026, 11, 13, 23, 0, 0);
        let old_added_on = utc(2026, 11, 1, 0, 0, 0).timestamp() as u64;

        let res = should_delete_torrent("abc123", old_added_on, None, 7, now);
        assert!(res.is_some());
    }

    #[test]
    fn keeps_unwatched_recent_torrent() {
        let now = utc(2026, 11, 13, 23, 0, 0);
        let recent_added_on = utc(2026, 11, 10, 0, 0, 0).timestamp() as u64;

        let res = should_delete_torrent("abc123", recent_added_on, None, 7, now);
        assert!(res.is_none());
    }

    #[test]
    fn malformed_watch_timestamp_falls_back_to_added_on() {
        let now = utc(2026, 11, 13, 23, 0, 0);
        let old_added_on = utc(2026, 11, 1, 0, 0, 0).timestamp() as u64;
        let recent_added_on = utc(2026, 11, 12, 0, 0, 0).timestamp() as u64;

        assert!(
            should_delete_torrent("abc123", old_added_on, Some("not-a-timestamp"), 7, now)
                .is_some()
        );
        assert!(
            should_delete_torrent("abc123", recent_added_on, Some("not-a-timestamp"), 7, now)
                .is_none()
        );
    }
}
