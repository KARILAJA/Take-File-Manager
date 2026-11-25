use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use turboget_lib::engine::network::{
    build_http_client, encode_basic_auth, load_network_settings, save_network_settings,
    NetworkSettings, ProxyMode,
};
use turboget_lib::engine::test_server::{compute_expected_hash, start_test_server, TestServerConfig};
use turboget_lib::manager::{state::DownloadStatus, DownloadManager, DownloadRequest};
use turboget_lib::storage::db::list_logs;
use turboget_lib::system::security::apply_mark_of_the_web;
use turboget_lib::system::updater::check_for_updates;

fn sha256_file(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("read downloaded file");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}

#[tokio::test]
async fn test_m10_mark_of_the_web_on_download_and_direct() {
    let tmp = tempdir().unwrap();
    let save_dir = tmp.path().to_path_buf();

    let server = start_test_server(TestServerConfig {
        total_bytes: 128 * 1024,
        filename: "motw_sample.bin".to_string(),
        ..Default::default()
    })
    .await;

    let manager = DownloadManager::new_in_memory().unwrap();
    let id = manager
        .add_download(DownloadRequest {
            url: server.base_url(),
            save_dir: save_dir.clone(),
            file_name: Some("motw_sample.bin".to_string()),
            category_id: None,
            queue_id: None,
            segments_cfg: Some(2),
            speed_limit: None,
            headers: vec![("Referer".to_string(), "https://example.org/page".to_string())],
            start_now: true,
        })
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let rec = manager.get_download(&id).await.unwrap().unwrap();
        if rec.status == DownloadStatus::Completed {
            break;
        }
        assert_ne!(rec.status, DownloadStatus::Failed, "download failed: {:?}", rec.error);
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for motw_sample.bin completion"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let final_path = save_dir.join("motw_sample.bin");
    assert!(final_path.exists());
    assert_eq!(sha256_file(&final_path), compute_expected_hash(128 * 1024));

    #[cfg(windows)]
    {
        let ads_path = format!("{}:Zone.Identifier", final_path.to_string_lossy());
        let ads_content = std::fs::read_to_string(&ads_path).expect("Zone.Identifier ADS should exist");
        assert!(ads_content.contains("[ZoneTransfer]"));
        assert!(ads_content.contains("ZoneId=3"));
        assert!(ads_content.contains(&format!("HostUrl={}", server.base_url())));
    }

    // Also test direct call with referrer sanitization
    let extra_file = save_dir.join("direct_motw.txt");
    std::fs::write(&extra_file, b"hello").unwrap();
    apply_mark_of_the_web(
        &extra_file,
        "https://cdn.example.org/direct_motw.txt\r\nInjected=1",
        Some("https://example.org/ref"),
    )
    .unwrap();

    #[cfg(windows)]
    {
        let ads = std::fs::read_to_string(format!("{}:Zone.Identifier", extra_file.to_string_lossy())).unwrap();
        assert!(ads.contains("ZoneId=3"));
        assert!(ads.contains("ReferrerUrl=https://example.org/ref"));
        assert!(!ads.lines().any(|l| l.starts_with("Injected=")));
    }

    // Completion should also record an INFO log in SQLite
    let conn = manager.get_db_conn().await;
    let logs = list_logs(&conn, 50).unwrap();
    assert!(
        logs.iter()
            .any(|l| l.level == "INFO" && l.message.contains("motw_sample.bin")),
        "expected INFO completion log in SQLite"
    );
}

#[tokio::test]
async fn test_m10_http_basic_auth_and_manual_proxy() {
    // 1. Origin server requiring Basic Auth
    let origin_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_port = origin_listener.local_addr().unwrap().port();
    let expected_auth = encode_basic_auth("user", "pass");
    tokio::spawn(async move {
        for _ in 0..4 {
            if let Ok((mut sock, _)) = origin_listener.accept().await {
                let expected = expected_auth.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let authorized = req
                        .lines()
                        .any(|l| l.to_ascii_lowercase().starts_with("authorization:") && l.contains(&expected));
                    let resp = if authorized {
                        "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nauth-ok"
                    } else {
                        "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"TurboGet\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    };
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        }
    });

    let direct_settings = NetworkSettings {
        proxy_mode: ProxyMode::Direct,
        ..Default::default()
    };
    let unauth_client = build_http_client(&direct_settings, false, None).unwrap();
    let r1 = unauth_client
        .get(format!("http://127.0.0.1:{}/secret", origin_port))
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status().as_u16(), 401);

    let auth_client = build_http_client(&direct_settings, false, Some(("user", "pass"))).unwrap();
    let r2 = auth_client
        .get(format!("http://127.0.0.1:{}/secret", origin_port))
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status().as_u16(), 200);
    assert_eq!(r2.text().await.unwrap(), "auth-ok");

    // 2. Manual HTTP Proxy with Proxy-Authorization + persistence in SQLite
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_port = proxy_listener.local_addr().unwrap().port();
    let expected_proxy_auth = encode_basic_auth("puser", "ppass");
    tokio::spawn(async move {
        if let Ok((mut sock, _)) = proxy_listener.accept().await {
            let mut buf = vec![0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let ok = req.lines().any(|l| {
                l.to_ascii_lowercase().starts_with("proxy-authorization:")
                    && l.contains(&expected_proxy_auth)
            });
            let resp = if ok {
                "HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nproxied-ok"
            } else {
                "HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            };
            let _ = sock.write_all(resp.as_bytes()).await;
        }
    });

    let manager = DownloadManager::new_in_memory().unwrap();
    let saved = NetworkSettings {
        proxy_mode: ProxyMode::Manual,
        proxy_url: Some(format!("http://127.0.0.1:{}", proxy_port)),
        proxy_username: Some("puser".to_string()),
        proxy_password: Some("ppass".to_string()),
        max_connections_per_host: 4,
        auto_update_enabled: true,
        update_feed_url: None,
        speed_limit_kbps: 512,
    };
    {
        let conn = manager.get_db_conn().await;
        save_network_settings(&conn, &saved).unwrap();
    }
    let loaded = {
        let conn = manager.get_db_conn().await;
        load_network_settings(&conn)
    };
    assert_eq!(loaded.proxy_mode, ProxyMode::Manual);

    let proxy_client = build_http_client(&loaded, false, None).unwrap();
    let r3 = proxy_client
        .get("http://example.invalid/artifact.zip")
        .send()
        .await
        .unwrap();
    assert_eq!(r3.status().as_u16(), 200);
    assert_eq!(r3.text().await.unwrap(), "proxied-ok");
}

#[tokio::test]
async fn test_m10_auto_update_checker() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        for _ in 0..2 {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = vec![0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let body = r#"{"version":"9.9.9","notes":"Release M10","download_url":"https://example.com/setup.exe"}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        }
    });

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let feed = format!("http://127.0.0.1:{}/latest.json", port);

    let res = check_for_updates(&client, env!("CARGO_PKG_VERSION"), &feed)
        .await
        .unwrap();
    assert!(res.update_available);
    assert_eq!(res.latest_version, "9.9.9");
    assert_eq!(res.notes.as_deref(), Some("Release M10"));

    let up_to_date = check_for_updates(&client, "9.9.9", &feed).await.unwrap();
    assert!(!up_to_date.update_available);
}

#[tokio::test]
async fn test_m10_sustained_queue_batch_stability() {
    let tmp = tempdir().unwrap();
    let save_dir = tmp.path().to_path_buf();
    let total_bytes = 256 * 1024;
    let server = start_test_server(TestServerConfig {
        total_bytes,
        filename: "batch.bin".to_string(),
        ..Default::default()
    })
    .await;

    let manager = DownloadManager::new_in_memory().unwrap();
    let mut ids = Vec::new();
    for idx in 0..8 {
        let id = manager
            .add_download(DownloadRequest {
                url: server.base_url(),
                save_dir: save_dir.clone(),
                file_name: Some(format!("sustained_{}.bin", idx)),
                category_id: None,
                queue_id: Some("default".to_string()),
                segments_cfg: Some(2),
                speed_limit: None,
                headers: vec![],
                start_now: false,
            })
            .await
            .unwrap();
        ids.push(id);
    }

    manager.start_queue("default").await.unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let mut all_done = true;
        for id in &ids {
            let rec = manager.get_download(id).await.unwrap().unwrap();
            assert_ne!(rec.status, DownloadStatus::Failed, "batch download failed: {:?}", rec.error);
            if rec.status != DownloadStatus::Completed {
                all_done = false;
            }
        }
        if all_done {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for sustained 8-item queue"
        );
        tokio::time::sleep(Duration::from_millis(75)).await;
    }

    // Wait briefly for task cleanup from active_tasks
    let cleanup_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !manager.active_tasks.lock().await.is_empty() {
        assert!(tokio::time::Instant::now() < cleanup_deadline, "active_tasks leaked!");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let expected = compute_expected_hash(total_bytes);
    for id in &ids {
        let rec = manager.get_download(id).await.unwrap().unwrap();
        let fpath = std::path::Path::new(&rec.save_dir).join(&rec.file_name);
        assert!(fpath.exists(), "missing completed file: {:?}", fpath);
        assert_eq!(sha256_file(&fpath), expected);
    }

    // No .part leftovers
    let leftover_parts: Vec<_> = std::fs::read_dir(&save_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
        .collect();
    assert!(leftover_parts.is_empty(), "found leftover .part files: {:?}", leftover_parts);

    let conn = manager.get_db_conn().await;
    let logs = list_logs(&conn, 100).unwrap();
    let info_count = logs.iter().filter(|l| l.level == "INFO").count();
    assert!(info_count >= 8, "expected >=8 INFO completion logs, got {}", info_count);
}

#[tokio::test]
async fn test_m10_live_speed_stats_and_browser_save_prompt() {
    let manager = DownloadManager::new_in_memory().unwrap();

    // 1. Live speed smoothing populates non-zero speed and downloaded bytes
    manager.record_live_progress("dl-1", 512 * 1024, Some(2 * 1024 * 1024), 256 * 1024);
    let stats1 = manager.get_live_stats("dl-1").unwrap();
    assert_eq!(stats1.downloaded_bytes, 512 * 1024);
    assert_eq!(stats1.speed_bps, 256 * 1024);

    // Next sample stabilizes via EMA and computes remaining ETA inputs cleanly
    manager.record_live_progress("dl-1", 1024 * 1024, Some(2 * 1024 * 1024), 512 * 1024);
    let stats2 = manager.get_live_stats("dl-1").unwrap();
    assert!(stats2.speed_bps > 256 * 1024);

    // 2. LocalApi POST /v1/downloads with prompt_user=true queues prompt instead of auto-starting
    let api = turboget_lib::api::server::start_local_api(manager.clone(), None)
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/v1/downloads", api.port))
        .header("Authorization", format!("Bearer {}", api.token))
        .json(&serde_json::json!({
            "url": "https://cdn.example.org/installer.zip",
            "file_name": "installer.zip",
            "cookies": "sid=xyz",
            "prompt_user": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 202);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["prompted"], true);

    // Duplicate identical prompt within dedup window should be ignored
    let resp_dup = client
        .post(format!("http://127.0.0.1:{}/v1/downloads", api.port))
        .header("Authorization", format!("Bearer {}", api.token))
        .json(&serde_json::json!({
            "url": "https://cdn.example.org/installer.zip",
            "file_name": "installer.zip",
            "cookies": "sid=xyz",
            "prompt_user": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_dup.status().as_u16(), 202);

    // No download auto-started yet, prompt awaits folder selection in UI (only 1 prompt!)
    assert!(manager.list_downloads(None).await.unwrap().is_empty());
    let prompt = manager.pop_pending_prompt().expect("expected queued browser prompt");
    assert_eq!(prompt.url, "https://cdn.example.org/installer.zip");
    assert_eq!(prompt.file_name.as_deref(), Some("installer.zip"));
    assert!(prompt.headers.iter().any(|(k, v)| k == "Cookie" && v == "sid=xyz"));
    assert!(manager.pop_pending_prompt().is_none());
}
