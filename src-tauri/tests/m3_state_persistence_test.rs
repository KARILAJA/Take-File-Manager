use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::tempdir;
use turboget_lib::engine::test_server::{compute_expected_hash, start_test_server, TestServerConfig};
use turboget_lib::manager::state::DownloadStatus;
use turboget_lib::manager::{DownloadManager, DownloadRequest};

#[tokio::test]
async fn test_m3_crash_and_kill_recovery_from_checkpoint() {
    let total_bytes: u64 = 40 * 1024 * 1024; // 40 MB
    let expected_hash = compute_expected_hash(total_bytes);

    let config = TestServerConfig {
        total_bytes,
        delay_ms_per_chunk: Arc::new(AtomicU64::new(10)), // 10ms per 64KB chunk per connection
        ..Default::default()
    };
    let srv = start_test_server(config).await;
    let temp_dir = tempdir().expect("tempdir failed");
    let db_path = temp_dir.path().join("turboget.db");
    let save_dir = temp_dir.path().join("downloads");

    let download_id: String;

    // 1. Session 1: Start download and simulate sudden abnormal process termination (crash / kill)
    {
        let manager_1 = DownloadManager::new(&db_path).expect("Failed to open DB session 1");

        let req = DownloadRequest {
            url: srv.base_url(),
            save_dir: save_dir.clone(),
            file_name: Some("crash_test.bin".to_string()),
            category_id: None,
            queue_id: None,
            segments_cfg: Some(4),
            speed_limit: None,
            headers: Vec::new(),
            start_now: true,
        };

        download_id = manager_1.add_download(req).await.expect("add_download failed");

        // Wait deterministically until partially downloaded (~5MB to 20MB)
        let mut downloaded_midway = 0u64;
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let rec = manager_1.get_download(&download_id).await.unwrap().unwrap();
            downloaded_midway = rec.downloaded;
            if downloaded_midway >= 5 * 1024 * 1024 {
                break;
            }
        }

        assert!(
            downloaded_midway >= 5 * 1024 * 1024,
            "Must have downloaded at least 5 MB before interruption: got {}",
            downloaded_midway
        );
        assert!(downloaded_midway < total_bytes);

        let rec_midway = manager_1.get_download(&download_id).await.unwrap().unwrap();
        assert_eq!(rec_midway.status, DownloadStatus::Downloading);

        // Abruptly terminate tasks in Session 1 to simulate sudden crash / SIGKILL
        {
            let tasks = manager_1.active_tasks.lock().await;
            if let Some(task) = tasks.get(&download_id) {
                task.cancel_token.store(true, Ordering::Relaxed);
                task.handle.abort();
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        drop(manager_1);
    }

    // 2. Session 2: New application session (restart after crash)
    {
        let manager_2 = DownloadManager::new(&db_path).expect("Failed to open DB session 2");

        // Recover interrupted downloads
        let recovered = manager_2
            .recover_interrupted_downloads()
            .await
            .expect("recover failed");
        assert!(recovered.contains(&download_id), "Interrupted download must be detected");

        let rec_recovered = manager_2.get_download(&download_id).await.unwrap().unwrap();
        assert_eq!(rec_recovered.status, DownloadStatus::Paused);
        assert!(
            rec_recovered.downloaded >= 4 * 1024 * 1024,
            "Progress must be preserved from the SQLite checkpoint: got {}",
            rec_recovered.downloaded
        );

        // Remove throttling on test server for fast completion
        srv.config.delay_ms_per_chunk.store(0, Ordering::Relaxed);

        // Resume the recovered download from checkpoint
        manager_2.start(&download_id).await.expect("Resume must succeed");

        // Poll until completed
        let mut final_status = DownloadStatus::Downloading;
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let rec = manager_2.get_download(&download_id).await.unwrap().unwrap();
            final_status = rec.status;
            if final_status == DownloadStatus::Completed || final_status == DownloadStatus::Failed {
                break;
            }
        }

        let final_rec = manager_2.get_download(&download_id).await.unwrap().unwrap();
        assert_eq!(
            final_status,
            DownloadStatus::Completed,
            "Expected completed, got status {:?} with error: {:?}",
            final_status,
            final_rec.error
        );

        assert_eq!(final_rec.downloaded, total_bytes);
        assert_eq!(final_rec.checksum_value, Some(expected_hash));

        let final_file = save_dir.join(&final_rec.file_name);
        assert!(final_file.exists());
        assert_eq!(std::fs::metadata(&final_file).unwrap().len(), total_bytes);
    }

    srv.shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test]
async fn test_m3_refresh_url_when_expired() {
    let temp_dir = tempdir().expect("tempdir failed");
    let db_path = temp_dir.path().join("turboget_refresh.db");
    let manager = DownloadManager::new(&db_path).expect("Failed to open DB");

    let req = DownloadRequest {
        url: "http://127.0.0.1:9999/expired_link".to_string(), // Invalid port
        save_dir: temp_dir.path().to_path_buf(),
        file_name: Some("expired.bin".to_string()),
        category_id: None,
        queue_id: None,
        segments_cfg: Some(4),
        speed_limit: None,
        headers: Vec::new(),
        start_now: false,
    };

    let id = manager.add_download(req).await.unwrap();

    // Manually simulate link expired state
    {
        let conn = turboget_lib::storage::db::init_db(&db_path).unwrap();
        turboget_lib::storage::db::update_download_status(
            &conn,
            &id,
            DownloadStatus::WaitingRefresh,
            Some("HTTP 403 Forbidden"),
        )
        .unwrap();
    }

    let rec = manager.get_download(&id).await.unwrap().unwrap();
    assert_eq!(rec.status, DownloadStatus::WaitingRefresh);

    // Refresh URL with a new valid link
    let new_url = "http://127.0.0.1:8088/new_valid_link".to_string();
    manager.refresh_url(&id, &new_url).await.unwrap();

    let updated_rec = manager.get_download(&id).await.unwrap().unwrap();
    assert_eq!(updated_rec.url, new_url);
    assert_eq!(updated_rec.status, DownloadStatus::Paused);
}
