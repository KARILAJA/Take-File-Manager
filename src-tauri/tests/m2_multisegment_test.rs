use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tempfile::tempdir;
use turboget_lib::engine::probe::probe_url;
use turboget_lib::engine::segmenter::SegmentManager;
use turboget_lib::engine::test_server::{compute_expected_hash, start_test_server, TestServerConfig};
use turboget_lib::engine::worker::MultiSegmentDownloader;

#[test]
fn test_m2_property_invariants_no_overlap_no_gap() {
    let total_size = 50_000_000u64; // 50 MB
    let mut mgr = SegmentManager::new(total_size, 4, 256 * 1024);
    mgr.verify_invariants().expect("Initial segment manager invariants failed");

    // Perform 300 randomized claims, progress updates, and dynamic splits
    let mut rng_seed = 123456789u64;
    let mut pseudo_rand = move || {
        rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (rng_seed >> 32) as u32
    };

    for worker_id in 0..300 {
        let action = pseudo_rand() % 3;
        match action {
            0 => {
                // Claim
                let _ = mgr.claim_next_segment(worker_id as usize);
            }
            1 => {
                // Record progress
                if !mgr.segments.is_empty() {
                    let idx = pseudo_rand() % mgr.segments.len() as u32;
                    let bytes = (pseudo_rand() % 100_000) as u64;
                    mgr.record_progress(idx, bytes);
                }
            }
            _ => {
                // Dynamic split
                let _ = mgr.try_dynamic_split(worker_id as usize, 256 * 1024);
            }
        }

        mgr.verify_invariants().expect("Invariants must hold after every single mutation");
    }
}

#[tokio::test]
async fn test_m2_fallback_when_server_lacks_range() {
    let total_bytes: u64 = 2 * 1024 * 1024; // 2 MB
    let expected_hash = compute_expected_hash(total_bytes);

    let config = TestServerConfig {
        total_bytes,
        no_range: Arc::new(AtomicBool::new(true)), // Disables Range
        ..Default::default()
    };
    let srv = start_test_server(config).await;
    let client = reqwest::Client::new();
    let temp_dir = tempdir().expect("tempdir failed");
    let save_dir = temp_dir.path().to_path_buf();

    let probe = probe_url(&client, &srv.base_url(), &[]).await.expect("probe failed");
    assert!(!probe.resumable, "Server must report not resumable without Range");

    let downloader = MultiSegmentDownloader::new(
        client.clone(),
        probe,
        save_dir.clone(),
        8,
    );

    let cancel_token = Arc::new(AtomicBool::new(false));
    let outcome = downloader
        .execute(cancel_token, |_| {})
        .await
        .expect("Fallback download must succeed");

    match outcome {
        turboget_lib::engine::single::DownloadOutcome::Completed {
            total_bytes: completed_bytes,
            sha256_hex,
            ..
        } => {
            assert_eq!(completed_bytes, total_bytes);
            assert_eq!(sha256_hex, expected_hash);
        }
        _ => panic!("Expected completed outcome"),
    }

    srv.shutdown.store(true, Ordering::Relaxed);
}

#[tokio::test]
async fn test_m2_multisegment_speedup_and_hash_match() {
    // 8 MB payload with 15ms per chunk throttle per connection
    // 8 MB = 128 chunks of 64 KB.
    // 1 worker: 128 chunks * 15ms ≈ 1.92 seconds.
    // 8 workers: 16 chunks * 15ms ≈ 0.24 seconds.
    // Speedup is approximately 8x, reliably >= 5.0x!
    let total_bytes: u64 = 8 * 1024 * 1024; // 8 MB
    let expected_hash = compute_expected_hash(total_bytes);

    let config = TestServerConfig {
        total_bytes,
        delay_ms_per_chunk: Arc::new(AtomicU64::new(15)),
        ..Default::default()
    };
    let srv = start_test_server(config).await;
    let client = reqwest::Client::new();
    let temp_dir = tempdir().expect("tempdir failed");

    let probe = probe_url(&client, &srv.base_url(), &[]).await.expect("probe failed");
    assert!(probe.resumable);

    // 1. Single-segment measurement
    let dir_1 = temp_dir.path().join("single");
    let downloader_1 = MultiSegmentDownloader::new(
        client.clone(),
        probe.clone(),
        dir_1,
        1, // 1 segment
    );
    let cancel_1 = Arc::new(AtomicBool::new(false));

    let start_1 = Instant::now();
    let _outcome_1 = downloader_1
        .execute(cancel_1, |_| {})
        .await
        .expect("1-segment download failed");
    let dur_1 = start_1.elapsed();

    // 2. 8-segment measurement
    let dir_8 = temp_dir.path().join("multi");
    let downloader_8 = MultiSegmentDownloader::new(
        client.clone(),
        probe.clone(),
        dir_8,
        8, // 8 segments
    );
    let cancel_8 = Arc::new(AtomicBool::new(false));

    let start_8 = Instant::now();
    let outcome_8 = downloader_8
        .execute(cancel_8, |_| {})
        .await
        .expect("8-segment download failed");
    let dur_8 = start_8.elapsed();

    let speedup = dur_1.as_secs_f64() / dur_8.as_secs_f64();
    println!(
        "Benchmark: 1 segment: {:.3}s, 8 segments: {:.3}s, Speedup: {:.2}x",
        dur_1.as_secs_f64(),
        dur_8.as_secs_f64(),
        speedup
    );

    // Check speedup >= 5.0x
    assert!(
        speedup >= 5.0,
        "8 segments must be >= 5x faster than 1 segment under per-connection throttling (actual: {:.2}x)",
        speedup
    );

    // Verify hash
    match outcome_8 {
        turboget_lib::engine::single::DownloadOutcome::Completed {
            total_bytes: completed_bytes,
            sha256_hex,
            ..
        } => {
            assert_eq!(completed_bytes, total_bytes);
            assert_eq!(sha256_hex, expected_hash, "8-segment SHA-256 must match expected hash");
        }
        _ => panic!("Expected completed outcome for 8-segment download"),
    }

    srv.shutdown.store(true, Ordering::Relaxed);
}
