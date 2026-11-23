use crate::config::AppState;
use crate::domain::should_delete_torrent;
use chrono::Utc;
use std::time::Duration;

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
