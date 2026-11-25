use serde::{Deserialize, Serialize};

use crate::manager::state::DownloadStatus;
use crate::storage::db::{DownloadRecord, ScheduleRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSchedule {
    pub id: String,
    pub queue_id: String,
    pub days_mask: u8,
    pub start_time: Option<String>,
    pub stop_time: Option<String>,
    pub after_action: String,
    pub after_command: Option<String>,
    pub enabled: bool,
}

impl From<ScheduleRecord> for QueueSchedule {
    fn from(r: ScheduleRecord) -> Self {
        Self {
            id: r.id,
            queue_id: r.queue_id,
            days_mask: r.days_mask,
            start_time: r.start_time,
            stop_time: r.stop_time,
            after_action: r.after_action,
            after_command: r.after_command,
            enabled: r.enabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerAction {
    StartQueue(String),
    StopQueue(String),
    ExecuteAfterAction {
        queue_id: String,
        action: String,
        command: Option<String>,
    },
}

pub struct Scheduler;

impl Scheduler {
    /// Evaluate schedule triggers given day of week (0=Mon .. 6=Sun) and "HH:MM" 24h formatted time
    pub fn evaluate_time_triggers(
        schedules: &[ScheduleRecord],
        current_day: u8,
        current_time: &str,
    ) -> Vec<SchedulerAction> {
        let mut actions = Vec::new();
        let day_bit = 1u8 << (current_day % 7);

        for s in schedules {
            if !s.enabled {
                continue;
            }

            // Check if schedule applies to today
            if (s.days_mask & day_bit) == 0 {
                continue;
            }

            if let Some(ref start) = s.start_time {
                if start == current_time {
                    actions.push(SchedulerAction::StartQueue(s.queue_id.clone()));
                }
            }

            if let Some(ref stop) = s.stop_time {
                if stop == current_time {
                    actions.push(SchedulerAction::StopQueue(s.queue_id.clone()));
                }
            }
        }

        actions
    }

    /// Check if all downloads in a scheduled queue are finished, and trigger post-completion action
    pub fn evaluate_queue_completion(
        schedule: &ScheduleRecord,
        queue_downloads: &[DownloadRecord],
    ) -> Option<SchedulerAction> {
        if !schedule.enabled || schedule.after_action == "none" || queue_downloads.is_empty() {
            return None;
        }

        // All items must be completed (or terminal)
        let all_finished = queue_downloads.iter().all(|d| {
            d.status == DownloadStatus::Completed || d.status == DownloadStatus::Canceled
        });

        if all_finished {
            Some(SchedulerAction::ExecuteAfterAction {
                queue_id: schedule.queue_id.clone(),
                action: schedule.after_action.clone(),
                command: schedule.after_command.clone(),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_time_triggers() {
        let schedules = vec![
            ScheduleRecord {
                id: "sch-1".into(),
                queue_id: "q-night".into(),
                days_mask: 0b01111111, // All days (Mon-Sun)
                start_time: Some("02:00".into()),
                stop_time: Some("06:00".into()),
                after_action: "shutdown".into(),
                after_command: None,
                enabled: true,
            },
            ScheduleRecord {
                id: "sch-2".into(),
                queue_id: "q-work".into(),
                days_mask: 0b00011111, // Mon-Fri
                start_time: Some("09:00".into()),
                stop_time: Some("17:00".into()),
                after_action: "none".into(),
                after_command: None,
                enabled: true,
            },
        ];

        // Monday at 02:00
        let actions = Scheduler::evaluate_time_triggers(&schedules, 0, "02:00");
        assert_eq!(actions, vec![SchedulerAction::StartQueue("q-night".into())]);

        // Monday at 06:00
        let actions = Scheduler::evaluate_time_triggers(&schedules, 0, "06:00");
        assert_eq!(actions, vec![SchedulerAction::StopQueue("q-night".into())]);

        // Saturday (day 5) at 09:00 -> sch-2 is Mon-Fri only, so should not trigger
        let actions = Scheduler::evaluate_time_triggers(&schedules, 5, "09:00");
        assert!(actions.is_empty());
    }

    #[test]
    fn test_scheduler_completion_action() {
        let schedule = ScheduleRecord {
            id: "sch-1".into(),
            queue_id: "default".into(),
            days_mask: 127,
            start_time: None,
            stop_time: None,
            after_action: "sleep".into(),
            after_command: None,
            enabled: true,
        };

        let downloads_incomplete = vec![
            DownloadRecord {
                id: "d1".into(),
                url: "u".into(),
                final_url: None,
                file_name: "f".into(),
                save_dir: "s".into(),
                total_bytes: Some(10),
                downloaded: 5,
                status: DownloadStatus::Downloading,
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
                created_at: 0,
                started_at: None,
                completed_at: None,
            },
        ];

        assert_eq!(Scheduler::evaluate_queue_completion(&schedule, &downloads_incomplete), None);

        let downloads_completed = vec![
            DownloadRecord {
                id: "d1".into(),
                url: "u".into(),
                final_url: None,
                file_name: "f".into(),
                save_dir: "s".into(),
                total_bytes: Some(10),
                downloaded: 10,
                status: DownloadStatus::Completed,
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
                created_at: 0,
                started_at: None,
                completed_at: None,
            },
        ];

        let action = Scheduler::evaluate_queue_completion(&schedule, &downloads_completed);
        assert_eq!(
            action,
            Some(SchedulerAction::ExecuteAfterAction {
                queue_id: "default".into(),
                action: "sleep".into(),
                command: None,
            })
        );
    }
}
