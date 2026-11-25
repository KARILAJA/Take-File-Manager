use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::manager::state::DownloadStatus;
use crate::storage::db::{DownloadRecord, QueueRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadQueue {
    pub id: String,
    pub name: String,
    pub max_concurrent: u32,
    pub speed_limit: Option<u64>,
    pub enabled: bool,
}

impl From<QueueRecord> for DownloadQueue {
    fn from(r: QueueRecord) -> Self {
        Self {
            id: r.id,
            name: r.name,
            max_concurrent: r.max_concurrent,
            speed_limit: r.speed_limit,
            enabled: r.enabled,
        }
    }
}

pub struct QueueManager;

impl QueueManager {
    /// Calculate how many active (probing or downloading) items currently run in the given queue
    pub fn count_active_in_queue(
        queue_id: &str,
        active_ids: &[String],
        downloads: &[DownloadRecord],
    ) -> usize {
        downloads
            .iter()
            .filter(|d| {
                let belongs = match d.queue_id.as_deref() {
                    Some(q) => q == queue_id,
                    None => queue_id == "default",
                };
                belongs && active_ids.contains(&d.id) && (d.status == DownloadStatus::Downloading || d.status == DownloadStatus::Probing)
            })
            .count()
    }

    /// Select next queued downloads to promote, sorted by priority descending and created_at ascending
    pub fn select_next_to_start(
        queue_id: &str,
        downloads: &[DownloadRecord],
        slots_available: usize,
    ) -> Vec<String> {
        if slots_available == 0 {
            return Vec::new();
        }

        let mut queued_items: Vec<&DownloadRecord> = downloads
            .iter()
            .filter(|d| {
                let belongs = match d.queue_id.as_deref() {
                    Some(q) => q == queue_id,
                    None => queue_id == "default",
                };
                belongs && d.status == DownloadStatus::Queued
            })
            .collect();

        // Sort: highest priority first, then earliest created_at
        queued_items.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        queued_items
            .into_iter()
            .take(slots_available)
            .map(|d| d.id.clone())
            .collect()
    }

    /// Update priority of a download to change its position in the queue
    pub fn set_priority(conn: &Connection, download_id: &str, new_priority: i32) -> Result<()> {
        let rows = conn.execute(
            "UPDATE downloads SET priority = ?1 WHERE id = ?2",
            params![new_priority, download_id],
        )?;
        if rows == 0 {
            return Err(anyhow!("Download {} not found", download_id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_priority_and_slots() {
        let mut dls = vec![
            DownloadRecord {
                id: "dl-1".into(),
                url: "u1".into(),
                final_url: None,
                file_name: "f1".into(),
                save_dir: "d".into(),
                total_bytes: Some(100),
                downloaded: 0,
                status: DownloadStatus::Queued,
                error: None,
                resumable: true,
                etag: None,
                last_modified: None,
                category_id: None,
                queue_id: Some("default".into()),
                priority: 0,
                segments_cfg: 4,
                speed_limit: None,
                headers_json: None,
                checksum_kind: None,
                checksum_value: None,
                created_at: 100,
                started_at: None,
                completed_at: None,
            },
            DownloadRecord {
                id: "dl-2".into(),
                url: "u2".into(),
                final_url: None,
                file_name: "f2".into(),
                save_dir: "d".into(),
                total_bytes: Some(100),
                downloaded: 0,
                status: DownloadStatus::Queued,
                error: None,
                resumable: true,
                etag: None,
                last_modified: None,
                category_id: None,
                queue_id: Some("default".into()),
                priority: 10, // Higher priority!
                segments_cfg: 4,
                speed_limit: None,
                headers_json: None,
                checksum_kind: None,
                checksum_value: None,
                created_at: 200,
                started_at: None,
                completed_at: None,
            },
            DownloadRecord {
                id: "dl-3".into(),
                url: "u3".into(),
                final_url: None,
                file_name: "f3".into(),
                save_dir: "d".into(),
                total_bytes: Some(100),
                downloaded: 0,
                status: DownloadStatus::Queued,
                error: None,
                resumable: true,
                etag: None,
                last_modified: None,
                category_id: None,
                queue_id: Some("default".into()),
                priority: 5,
                segments_cfg: 4,
                speed_limit: None,
                headers_json: None,
                checksum_kind: None,
                checksum_value: None,
                created_at: 150,
                started_at: None,
                completed_at: None,
            },
        ];

        // Request 2 slots: should pick dl-2 (priority 10) then dl-3 (priority 5)
        let picked = QueueManager::select_next_to_start("default", &dls, 2);
        assert_eq!(picked, vec!["dl-2", "dl-3"]);

        // If dl-2 is downloading, active count is 1
        let active = vec!["dl-2".to_string()];
        dls[1].status = DownloadStatus::Downloading;
        let count = QueueManager::count_active_in_queue("default", &active, &dls);
        assert_eq!(count, 1);
    }
}
