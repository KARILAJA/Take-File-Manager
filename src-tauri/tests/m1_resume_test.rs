use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::tempdir;
use turboget_lib::engine::probe::probe_url;
use turboget_lib::engine::single::{DownloadOutcome, SingleStreamDownloader};
use turboget_lib::engine::test_server::{compute_expected_hash, start_test_server, TestServerConfig};

#[tokio::test]
async fn test_m1_probe_100mb() {
    let config = TestServerConfig::default();
    let srv = start_test_server(config).await;
    let client = reqwest::Client::new();

    let probe = probe_url(&client, &srv.base_url(), &[]).await.expect("probe failed");

    assert!(probe.resumable, "Server must support Range requests");
    assert_eq!(probe.total_bytes, Some(100 * 1024 * 1024));
    assert_eq!(probe.file_name, "turboget_100mb.bin");
    assert_eq!(probe.etag, Some("turboget-test-etag-v1".to_string()));

    srv.shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test]
async fn test_m1_download_100mb_disconnect_and_resume_hash_matches() {
    let total_bytes: u64 = 100 * 1024 * 1024; // 100 MB
    let expected_hash = compute_expected_hash(total_bytes);

    let config = TestServerConfig {
        total_bytes,
        ..Default::default()
    };
    let srv = start_test_server(config).await;
    let client = reqwest::Client::new();
    let temp_dir = tempdir().expect("failed to create temp dir");
    let save_dir = temp_dir.path().to_path_buf();

    // 1. Probe
    let probe = probe_url(&client, &srv.base_url(), &[]).await.expect("probe failed");

    // 2. Stage 1: Drop connection after ~35 MB
    let drop_point = 35 * 1024 * 1024; // 35 MB
    srv.set_drop_after(drop_point);

    let downloader = SingleStreamDownloader::new(
        client.clone(),
        srv.base_url(),
        save_dir.clone(),
        probe.file_name.clone(),
        probe.etag.clone(),
        probe.total_bytes,
    );

    // Disable auto-retries in stage 1 so we intentionally observe the interruption
    let mut d_stage1 = downloader;
    d_stage1.max_retries = 0;

    let cancel_token = Arc::new(AtomicBool::new(false));

    let stage1_res = d_stage1
        .execute(cancel_token.clone(), |p| {
            assert!(p.downloaded_bytes <= drop_point + 128 * 1024);
        })
        .await;

    // Disconnect occurred as expected
    assert!(stage1_res.is_err(), "Expected connection interruption");

    // Verify .part file exists with approx 35 MB
    let part_path = save_dir.join(format!("{}.part", probe.file_name));
    assert!(part_path.exists(), ".part file must persist after disconnect");
    let part_len = std::fs::metadata(&part_path).unwrap().len();
    assert!((34 * 1024 * 1024..=36 * 1024 * 1024).contains(&part_len));

    // 3. Stage 2: Resume download without connection drops
    srv.set_drop_after(0); // Normal server mode

    let mut d_stage2 = SingleStreamDownloader::new(
        client.clone(),
        srv.base_url(),
        save_dir.clone(),
        probe.file_name.clone(),
        probe.etag.clone(),
        probe.total_bytes,
    );
    d_stage2.max_retries = 3;

    let stage2_res = d_stage2
        .execute(cancel_token.clone(), |_| {})
        .await
        .expect("Stage 2 resume must succeed");

    match stage2_res {
        DownloadOutcome::Completed {
            final_path,
            total_bytes: completed_bytes,
            sha256_hex,
        } => {
            assert_eq!(completed_bytes, total_bytes);
            assert_eq!(
                sha256_hex, expected_hash,
                "Resume SHA-256 must match exactly the expected 100MB hash"
            );
            assert!(final_path.exists(), "Final file must exist");
            assert!(!part_path.exists(), ".part file should be renamed to final file");
            assert_eq!(std::fs::metadata(&final_path).unwrap().len(), total_bytes);
        }
        DownloadOutcome::Paused { .. } => {
            panic!("Expected completed download, got paused");
        }
    }

    srv.shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test]
async fn test_m1_pause_and_resume_cooperative() {
    let total_bytes: u64 = 20 * 1024 * 1024; // 20 MB
    let expected_hash = compute_expected_hash(total_bytes);

    let config = TestServerConfig {
        total_bytes,
        delay_ms_per_chunk: Arc::new(AtomicU64::new(3)), // 3ms delay per 64KB ensures window for cooperative pause
        ..Default::default()
    };
    let srv = start_test_server(config).await;
    let client = reqwest::Client::new();
    let temp_dir = tempdir().expect("failed to create temp dir");
    let save_dir = temp_dir.path().to_path_buf();

    let probe = probe_url(&client, &srv.base_url(), &[]).await.unwrap();

    let downloader = SingleStreamDownloader::new(
        client.clone(),
        srv.base_url(),
        save_dir.clone(),
        probe.file_name.clone(),
        probe.etag.clone(),
        probe.total_bytes,
    );

    let cancel_token = Arc::new(AtomicBool::new(false));

    // Pause when downloaded reaches ~5 MB
    let token_clone = cancel_token.clone();
    let outcome = downloader
        .execute(cancel_token.clone(), move |p| {
            if p.downloaded_bytes >= 5 * 1024 * 1024 {
                token_clone.store(true, Ordering::Relaxed);
            }
        })
        .await
        .unwrap();

    match outcome {
        DownloadOutcome::Paused { part_path, downloaded_bytes } => {
            assert!(part_path.exists());
            assert!(downloaded_bytes >= 5 * 1024 * 1024);
            assert!(downloaded_bytes < total_bytes);
        }
        _ => panic!("Expected paused outcome"),
    }

    // Disable delay for fast resume to completion
    srv.config.delay_ms_per_chunk.store(0, Ordering::Relaxed);
    cancel_token.store(false, Ordering::Relaxed);

    let outcome2 = downloader
        .execute(cancel_token.clone(), |_| {})
        .await
        .unwrap();

    match outcome2 {
        DownloadOutcome::Completed { total_bytes: b, sha256_hex, .. } => {
            assert_eq!(b, total_bytes);
            assert_eq!(sha256_hex, expected_hash);
        }
        _ => panic!("Expected completed outcome"),
    }

    srv.shutdown.store(true, Ordering::Relaxed);
}
