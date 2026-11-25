use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::tempdir;
use turboget_lib::api::server::start_local_api;
use turboget_lib::engine::test_server::{compute_expected_hash, start_test_server, TestServerConfig};
use turboget_lib::manager::state::DownloadStatus;
use turboget_lib::manager::DownloadManager;

#[tokio::test]
async fn test_m6_local_api_security_host_origin_and_bearer_auth() {
    let tmp = tempdir().unwrap();
    let api_json_path = tmp.path().join("api.json");

    let manager = DownloadManager::new_in_memory().expect("init manager failed");
    let handle = start_local_api(manager, Some(api_json_path.clone()))
        .await
        .expect("start_local_api failed");

    let base_url = format!("http://127.0.0.1:{}", handle.port);
    let client = reqwest::Client::new();

    // 1. Missing Authorization -> 401
    let resp = client
        .get(format!("{}/v1/downloads", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);

    // 2. Wrong Bearer token -> 401
    let resp = client
        .get(format!("{}/v1/downloads", base_url))
        .header("Authorization", "Bearer wrong-token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);

    // 3. Disallowed web Origin (CSRF / DNS rebinding protection) -> 403
    let resp = client
        .get(format!("{}/v1/downloads", base_url))
        .header("Authorization", format!("Bearer {}", handle.token))
        .header("Origin", "https://evil-site.example")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403);

    // 4. Disallowed Host header -> 403
    let resp = client
        .get(format!("{}/v1/downloads", base_url))
        .header("Authorization", format!("Bearer {}", handle.token))
        .header("Host", "evil.example.com")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403);

    // 5. Valid Bearer token + loopback Host -> 200 OK
    let resp = client
        .get(format!("{}/v1/queues", base_url))
        .header("Authorization", format!("Bearer {}", handle.token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    // Verify api.json was persisted with matching port and token
    let api_json_str = std::fs::read_to_string(&api_json_path).unwrap();
    let api_val: serde_json::Value = serde_json::from_str(&api_json_str).unwrap();
    assert_eq!(api_val["port"].as_u64().unwrap() as u16, handle.port);
    assert_eq!(api_val["token"].as_str().unwrap(), handle.token);
}

#[tokio::test]
async fn test_m6_native_host_to_core_cookie_forwarding_and_download() {
    let total_bytes: u64 = 1_500_000; // 1.5 MB
    let expected_hash = compute_expected_hash(total_bytes);
    let required_cookie = "session=turboget_secret_42";

    let server_cfg = TestServerConfig {
        total_bytes,
        filename: "protected_archive.zip".to_string(),
        required_cookie: Some(required_cookie.to_string()),
        ..Default::default()
    };
    let server = start_test_server(server_cfg).await;

    let tmp = tempdir().unwrap();
    let api_json_path = tmp.path().join("api.json");
    let save_dir = tmp.path().join("downloads");
    std::fs::create_dir_all(&save_dir).unwrap();

    let manager = DownloadManager::new_in_memory().expect("init manager failed");
    let _api_handle = start_local_api(manager.clone(), Some(api_json_path.clone()))
        .await
        .expect("start_local_api failed");

    // Locate compiled turboget-host binary
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let host_bin = manifest_dir
        .parent()
        .unwrap()
        .join("native-host")
        .join("target")
        .join("debug")
        .join(if cfg!(windows) { "turboget-host.exe" } else { "turboget-host" });

    assert!(
        host_bin.exists(),
        "Expected compiled turboget-host binary at {}. Build native-host first.",
        host_bin.display()
    );

    // Prepare Chrome Native Messaging stdio JSON payload
    let msg_json = serde_json::json!({
        "type": "add_download",
        "payload": {
            "url": server.base_url(),
            "saveDir": save_dir.to_string_lossy(),
            "cookies": format!("{}; theme=dark", required_cookie),
            "referrer": "https://portal.example.com/files",
            "userAgent": "Mozilla/5.0 TurboGetTest",
            "mime": "application/zip",
            "startNow": true
        }
    })
    .to_string();

    let (status, resp_buf) = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(&host_bin)
            .env("TURBOGET_API_JSON", &api_json_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn turboget-host");

        {
            let mut stdin = child.stdin.take().unwrap();
            let len_bytes = (msg_json.len() as u32).to_ne_bytes();
            stdin.write_all(&len_bytes).unwrap();
            stdin.write_all(msg_json.as_bytes()).unwrap();
            stdin.flush().unwrap();
        }

        let mut stdout = child.stdout.take().unwrap();
        let mut resp_len_bytes = [0u8; 4];
        stdout.read_exact(&mut resp_len_bytes).expect("failed reading native host frame len");
        let resp_len = u32::from_ne_bytes(resp_len_bytes) as usize;
        let mut resp_buf = vec![0u8; resp_len];
        stdout.read_exact(&mut resp_buf).expect("failed reading native host frame body");

        let status = child.wait().unwrap();
        (status, resp_buf)
    })
    .await
    .expect("spawn_blocking panicked");

    assert!(status.success(), "turboget-host exited abnormally");

    let resp_json: serde_json::Value = serde_json::from_slice(&resp_buf).unwrap();
    assert_eq!(resp_json["ok"], true, "Native host returned error: {:?}", resp_json);
    let download_id = resp_json["id"].as_str().expect("missing download id").to_string();

    // Poll DownloadManager until download completes using forwarded cookie
    let mut completed_rec = None;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(rec) = manager.get_download(&download_id).await.unwrap() {
            if rec.status == DownloadStatus::Completed {
                completed_rec = Some(rec);
                break;
            }
            if rec.status == DownloadStatus::Failed || rec.status == DownloadStatus::WaitingRefresh {
                panic!(
                    "Download failed unexpectedly (cookie may not have been forwarded!): status={:?}, error={:?}",
                    rec.status, rec.error
                );
            }
        }
    }

    let rec = completed_rec.expect("Timed out waiting for download completion");
    assert_eq!(rec.checksum_value.as_deref(), Some(expected_hash.as_str()));
    assert_eq!(rec.file_name, "protected_archive.zip");
    assert!(save_dir.join(&rec.file_name).exists());
}
