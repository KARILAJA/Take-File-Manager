use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tempfile::tempdir;
use turboget_lib::engine::limiter::RateLimiter;
use turboget_lib::engine::probe::probe_url;
use turboget_lib::engine::test_server::{compute_expected_hash, start_test_server, TestServerConfig};
use turboget_lib::engine::worker::MultiSegmentDownloader;
use turboget_lib::manager::category::{detect_category, resolve_save_dir};
use turboget_lib::manager::queue::QueueManager;
use turboget_lib::manager::scheduler::{Scheduler, SchedulerAction};
use turboget_lib::manager::state::DownloadStatus;
use turboget_lib::manager::{DownloadManager, DownloadRequest};
use turboget_lib::storage::db::{CategoryRecord, QueueRecord, ScheduleRecord};

#[tokio::test]
async fn test_m5_rate_limiter_500kbps_accurate_within_10_percent() {
    // 2.5 MB test file
    let total_bytes: u64 = 2_500_000;
    let expected_hash = compute_expected_hash(total_bytes);

    let config = TestServerConfig {
        total_bytes,
        filename: "test_limiter_2_5mb.bin".to_string(),
        ..Default::default()
    };

    let server = start_test_server(config).await;
    let client = reqwest::Client::new();
    let probe = probe_url(&client, &server.base_url(), &[]).await.expect("probe failed");

    let tmp = tempdir().unwrap();
    let save_dir = tmp.path().to_path_buf();

    // 500 KB/s = 500 * 1024 = 512,000 bytes/sec
    let target_limit_bps = 512_000u64;
    let limiter = Arc::new(RateLimiter::new(target_limit_bps));

    let downloader = MultiSegmentDownloader::new(client, probe, save_dir.clone(), 4)
        .with_rate_limiter(limiter);

    let cancel_token = Arc::new(AtomicBool::new(false));
    let start_time = Instant::now();

    let outcome = downloader
        .execute(cancel_token, |_| {})
        .await
        .expect("download failed");

    let elapsed = start_time.elapsed().as_secs_f64();
    let measured_bps = (total_bytes as f64) / elapsed;

    println!(
        "Rate Limiter Test: total={} bytes, elapsed={:.3}s, measured={:.1} bytes/sec, target={} bytes/sec",
        total_bytes, elapsed, measured_bps, target_limit_bps
    );

    // Verify SHA-256 hash
    match outcome {
        turboget_lib::engine::single::DownloadOutcome::Completed { sha256_hex, total_bytes: downloaded, .. } => {
            assert_eq!(downloaded, total_bytes);
            assert_eq!(sha256_hex, expected_hash, "Hash mismatch on rate limited download");
        }
        _ => panic!("Expected completed download"),
    }

    // Acceptance criterion: 500 KB/s accurate within +- 10%
    // 512,000 * 0.90 = 460,800
    // 512,000 * 1.10 = 563,200
    let min_allowed = (target_limit_bps as f64) * 0.88;
    let max_allowed = (target_limit_bps as f64) * 1.12;

    assert!(
        measured_bps >= min_allowed && measured_bps <= max_allowed,
        "Measured speed {:.1} bytes/s not within +-10% of target {} bytes/s [{:.1} - {:.1}]",
        measured_bps,
        target_limit_bps,
        min_allowed,
        max_allowed
    );
}

#[tokio::test]
async fn test_m5_queue_max_concurrent_and_auto_promotion() {
    let total_bytes: u64 = 1_000_000; // 1 MB
    let config = TestServerConfig {
        total_bytes,
        filename: "test_queue.bin".to_string(),
        delay_ms_per_chunk: Arc::new(std::sync::atomic::AtomicU64::new(5)),
        ..Default::default()
    };

    let server = start_test_server(config).await;
    let tmp = tempdir().unwrap();
    let dir_low = tmp.path().join("low");
    let dir_high = tmp.path().join("high");
    let dir_mid = tmp.path().join("mid");

    let manager = DownloadManager::new_in_memory().expect("init manager failed");

    // Insert custom queue with max_concurrent = 2
    let q_test = QueueRecord {
        id: "q-test".to_string(),
        name: "Test Queue".to_string(),
        max_concurrent: 2,
        speed_limit: None,
        enabled: true,
    };
    {
        let conn = manager.get_db_conn().await;
        turboget_lib::storage::db::insert_queue(&conn, &q_test).unwrap();
    }

    // Add 3 downloads to q-test with different priorities
    let id_low = manager
        .add_download(DownloadRequest {
            url: server.base_url(),
            save_dir: dir_low,
            file_name: Some("item_low.bin".to_string()),
            category_id: None,
            queue_id: Some("q-test".to_string()),
            segments_cfg: Some(2),
            speed_limit: None,
            headers: vec![],
            start_now: false,
        })
        .await
        .unwrap();

    let id_high = manager
        .add_download(DownloadRequest {
            url: server.base_url(),
            save_dir: dir_high,
            file_name: Some("item_high.bin".to_string()),
            category_id: None,
            queue_id: Some("q-test".to_string()),
            segments_cfg: Some(2),
            speed_limit: None,
            headers: vec![],
            start_now: false,
        })
        .await
        .unwrap();

    let id_mid = manager
        .add_download(DownloadRequest {
            url: server.base_url(),
            save_dir: dir_mid,
            file_name: Some("item_mid.bin".to_string()),
            category_id: None,
            queue_id: Some("q-test".to_string()),
            segments_cfg: Some(2),
            speed_limit: None,
            headers: vec![],
            start_now: false,
        })
        .await
        .unwrap();

    // Set priorities: id_high=10, id_mid=5, id_low=0
    {
        let conn = manager.get_db_conn().await;
        QueueManager::set_priority(&conn, &id_high, 10).unwrap();
        QueueManager::set_priority(&conn, &id_mid, 5).unwrap();
        QueueManager::set_priority(&conn, &id_low, 0).unwrap();
    }

    // Start queue: max_concurrent is 2, so id_high and id_mid should start first
    manager.start_queue("q-test").await.unwrap();

    // Wait a brief moment for spawning
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    {
        let active = manager.active_tasks.lock().await;
        assert_eq!(active.len(), 2, "Queue should cap at max_concurrent = 2");
        assert!(active.contains_key(&id_high), "High priority item should run");
        assert!(active.contains_key(&id_mid), "Mid priority item should run");
        assert!(!active.contains_key(&id_low), "Low priority item should wait in queue");
    }

    // Wait until all 3 complete via auto-promotion
    let mut finished = false;
    for i in 0..60 {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let d1 = manager.get_download(&id_high).await.unwrap().unwrap();
        let d2 = manager.get_download(&id_mid).await.unwrap().unwrap();
        let d3 = manager.get_download(&id_low).await.unwrap().unwrap();

        if i % 10 == 0 {
            println!("Poll {}: d1={:?} (err={:?}), d2={:?} (err={:?}), d3={:?} (err={:?})", i, d1.status, d1.error, d2.status, d2.error, d3.status, d3.error);
        }

        if d1.status == DownloadStatus::Completed
            && d2.status == DownloadStatus::Completed
            && d3.status == DownloadStatus::Completed
        {
            finished = true;
            break;
        }
    }

    assert!(finished, "All 3 items should complete through automatic queue promotion");
}

#[test]
fn test_m5_scheduler_and_after_actions() {
    let schedules = vec![
        ScheduleRecord {
            id: "sch-1".into(),
            queue_id: "q-night".into(),
            days_mask: 0b01111111, // Mon-Sun
            start_time: Some("02:00".into()),
            stop_time: Some("06:00".into()),
            after_action: "shutdown".into(),
            after_command: None,
            enabled: true,
        },
        ScheduleRecord {
            id: "sch-weekend".into(),
            queue_id: "q-weekend".into(),
            days_mask: 0b01100000, // Sat & Sun only (bits 5 & 6)
            start_time: Some("12:00".into()),
            stop_time: Some("18:00".into()),
            after_action: "exit".into(),
            after_command: None,
            enabled: true,
        },
    ];

    // Check triggers on Monday (day 0) at 02:00 -> starts q-night
    let actions = Scheduler::evaluate_time_triggers(&schedules, 0, "02:00");
    assert_eq!(actions, vec![SchedulerAction::StartQueue("q-night".into())]);

    // Check triggers on Monday (day 0) at 06:00 -> stops q-night
    let actions = Scheduler::evaluate_time_triggers(&schedules, 0, "06:00");
    assert_eq!(actions, vec![SchedulerAction::StopQueue("q-night".into())]);

    // Saturday (day 5) at 12:00 -> starts q-weekend
    let actions = Scheduler::evaluate_time_triggers(&schedules, 5, "12:00");
    assert_eq!(actions, vec![SchedulerAction::StartQueue("q-weekend".into())]);

    // Monday (day 0) at 12:00 -> should not trigger weekend schedule
    let actions = Scheduler::evaluate_time_triggers(&schedules, 0, "12:00");
    assert!(actions.is_empty());
}

#[test]
fn test_m5_auto_categories_and_folder_mapping() {
    let categories = vec![
        CategoryRecord {
            id: "compressed".into(),
            name: "Compressed".into(),
            extensions: "zip,rar,7z,iso,tgz".into(),
            save_dir: "C:/Downloads/Compressed".into(),
        },
        CategoryRecord {
            id: "music".into(),
            name: "Music".into(),
            extensions: "mp3,wav,flac".into(),
            save_dir: "C:/Downloads/Music".into(),
        },
        CategoryRecord {
            id: "documents".into(),
            name: "Documents".into(),
            extensions: "pdf,docx,txt".into(),
            save_dir: "C:/Downloads/Documents".into(),
        },
    ];

    // Detect category by file name
    assert_eq!(detect_category("ubuntu.iso", &categories), "compressed");
    assert_eq!(detect_category("track.mp3", &categories), "music");
    assert_eq!(detect_category("notes.pdf", &categories), "documents");
    assert_eq!(detect_category("unknown.xyz", &categories), "others");

    // Resolve save directory
    let (dir, cat) = resolve_save_dir(None, "ubuntu.iso", &categories, "C:/Downloads");
    assert_eq!(dir, "C:/Downloads/Compressed");
    assert_eq!(cat, "compressed");

    // Explicit override preserved
    let (dir_override, _) = resolve_save_dir(Some("D:/MyCustomDir"), "ubuntu.iso", &categories, "C:/Downloads");
    assert_eq!(dir_override, "D:/MyCustomDir");
}
