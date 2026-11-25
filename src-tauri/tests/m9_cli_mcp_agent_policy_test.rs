use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::tempdir;
use turboget_lib::api::server::start_local_api;
use turboget_lib::engine::test_server::{compute_expected_hash, start_test_server, TestServerConfig};
use turboget_lib::manager::agent_policy::{save_agent_policy, AgentAccessMode, AgentAccessPolicy};
use turboget_lib::manager::DownloadManager;
use turboget_lib::storage::db::list_logs;

fn cli_binary_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .join("cli")
        .join("target")
        .join("debug")
        .join(if cfg!(windows) { "turboget.exe" } else { "turboget" })
}

#[tokio::test]
async fn test_m9_agent_policy_modes_ssrf_and_ratelimit() {
    let tmp = tempdir().unwrap();
    let allowed_dir = tmp.path().join("allowed");
    std::fs::create_dir_all(&allowed_dir).unwrap();

    let manager = DownloadManager::new_in_memory().unwrap();
    let api = start_local_api(manager.clone(), None).await.unwrap();
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{}", api.port);

    // 1. Default mode is Ask -> agent POST /v1/downloads is blocked (403)
    let resp = client
        .post(format!("{}/v1/downloads", base))
        .header("Authorization", format!("Bearer {}", api.token))
        .header("X-TurboGet-Actor", "agent")
        .json(&serde_json::json!({
            "url": "https://example.com/file.zip",
            "save_dir": allowed_dir.to_string_lossy(),
            "start_now": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403);

    // 2. Deny mode blocks even read queries from agent
    {
        let conn = manager.get_db_conn().await;
        save_agent_policy(
            &conn,
            &AgentAccessPolicy {
                mode: AgentAccessMode::Deny,
                allowed_dirs: vec![allowed_dir.to_string_lossy().to_string()],
                block_private_ips: true,
                max_requests_per_minute: 30,
            },
        )
        .unwrap();
    }
    let resp = client
        .get(format!("{}/v1/downloads", base))
        .header("Authorization", format!("Bearer {}", api.token))
        .header("X-TurboGet-Actor", "agent")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403);

    // 3. Allowlist mode with block_private_ips=true blocks SSRF localhost/private IP
    {
        let conn = manager.get_db_conn().await;
        save_agent_policy(
            &conn,
            &AgentAccessPolicy {
                mode: AgentAccessMode::Allowlist,
                allowed_dirs: vec![allowed_dir.to_string_lossy().to_string()],
                block_private_ips: true,
                max_requests_per_minute: 4,
            },
        )
        .unwrap();
    }
    let resp = client
        .post(format!("{}/v1/downloads", base))
        .header("Authorization", format!("Bearer {}", api.token))
        .header("X-TurboGet-Actor", "agent")
        .json(&serde_json::json!({
            "url": "http://127.0.0.1:9999/secret.bin",
            "save_dir": allowed_dir.to_string_lossy(),
            "start_now": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403);

    // 4. Rate limit kicks in after max_requests_per_minute (already 3 agent calls above, 4th succeeds, 5th returns 429)
    let r4 = client
        .get(format!("{}/v1/downloads", base))
        .header("Authorization", format!("Bearer {}", api.token))
        .header("X-TurboGet-Actor", "agent")
        .send()
        .await
        .unwrap();
    assert_eq!(r4.status().as_u16(), 200);

    let r5 = client
        .get(format!("{}/v1/downloads", base))
        .header("Authorization", format!("Bearer {}", api.token))
        .header("X-TurboGet-Actor", "agent")
        .send()
        .await
        .unwrap();
    assert_eq!(r5.status().as_u16(), 429);

    // Audit logs recorded every request
    let conn = manager.get_db_conn().await;
    let logs = list_logs(&conn, 20).unwrap();
    assert!(logs.iter().any(|l| l.level == "AGENT_AUDIT" && l.message.contains("RATE_LIMITED")));
    assert!(logs.iter().any(|l| l.level == "AGENT_AUDIT" && l.message.contains("DENIED(ssrf)")));
}

#[tokio::test]
async fn test_m9_mcp_stdio_end_to_end_allowlist_and_denial() {
    let cli_bin = cli_binary_path();
    assert!(
        cli_bin.exists(),
        "Expected compiled turboget CLI binary at {}. Build cli crate first.",
        cli_bin.display()
    );

    let total_bytes: u64 = 512 * 1024;
    let expected_hash = compute_expected_hash(total_bytes);
    let server = start_test_server(TestServerConfig {
        total_bytes,
        filename: "mcp_payload.bin".to_string(),
        ..Default::default()
    })
    .await;

    let tmp = tempdir().unwrap();
    let allowed_dir = tmp.path().join("allowed_downloads");
    let forbidden_dir = tmp.path().join("outside_forbidden");
    std::fs::create_dir_all(&allowed_dir).unwrap();
    std::fs::create_dir_all(&forbidden_dir).unwrap();
    let api_json_path = tmp.path().join("api.json");

    let manager = DownloadManager::new_in_memory().unwrap();
    {
        let conn = manager.get_db_conn().await;
        save_agent_policy(
            &conn,
            &AgentAccessPolicy {
                mode: AgentAccessMode::Allowlist,
                allowed_dirs: vec![allowed_dir.to_string_lossy().to_string()],
                block_private_ips: false, // Disabled only for local loopback test_server
                max_requests_per_minute: 50,
            },
        )
        .unwrap();
    }

    let _api = start_local_api(manager.clone(), Some(api_json_path.clone()))
        .await
        .unwrap();

    let server_url = server.base_url();
    let allowed_str = allowed_dir.to_string_lossy().to_string();
    let forbidden_str = forbidden_dir.to_string_lossy().to_string();

    // Interact with `turboget mcp` over stdio in a blocking thread so async LocalApi keeps running
    let dl_id = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(&cli_bin)
            .arg("mcp")
            .env("TURBOGET_API_JSON", &api_json_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn turboget mcp");

        let mut stdin = child.stdin.take().unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());

        let mut rpc_call = |req: serde_json::Value| -> serde_json::Value {
            writeln!(stdin, "{}", req).unwrap();
            stdin.flush().unwrap();
            let mut line = String::new();
            reader.read_line(&mut line).expect("failed reading MCP response line");
            serde_json::from_str(&line).expect("invalid MCP JSON response")
        };

        // 1. Initialize & list tools
        let init = rpc_call(serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
        }));
        assert_eq!(init["result"]["serverInfo"]["name"], "turboget");

        let tools_resp = rpc_call(serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/list"
        }));
        assert_eq!(tools_resp["result"]["tools"].as_array().unwrap().len(), 8);

        // 2. Request to folder OUTSIDE allowlist MUST BE DENIED!
        let denied_resp = rpc_call(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "add_download",
                "arguments": {
                    "url": server_url,
                    "save_dir": forbidden_str,
                    "start_now": true
                }
            }
        }));
        assert_eq!(denied_resp["result"]["isError"], true, "Outside-allowlist folder must be denied!");
        let err_text = denied_resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            err_text.contains("outside allowed") || err_text.contains("Permission denied"),
            "Unexpected error message: {}",
            err_text
        );

        // 3. Request inside allowlist succeeds
        let ok_resp = rpc_call(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "add_download",
                "arguments": {
                    "url": server_url,
                    "save_dir": allowed_str,
                    "segments": 4,
                    "start_now": true
                }
            }
        }));
        assert_eq!(ok_resp["result"]["isError"], false, "Allowed folder failed: {:?}", ok_resp);
        let text_json: serde_json::Value = serde_json::from_str(
            ok_resp["result"]["content"][0]["text"].as_str().unwrap(),
        )
        .unwrap();
        let download_id = text_json["id"].as_str().unwrap().to_string();

        // 4. Poll via MCP `get_download` until Completed
        let mut completed = false;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            let get_resp = rpc_call(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "get_download",
                    "arguments": { "id": download_id }
                }
            }));
            assert_eq!(get_resp["result"]["isError"], false);
            let detail: serde_json::Value = serde_json::from_str(
                get_resp["result"]["content"][0]["text"].as_str().unwrap(),
            )
            .unwrap();
            if detail["download"]["status"] == "completed" {
                assert_eq!(
                    detail["download"]["checksum_value"].as_str(),
                    Some(expected_hash.as_str())
                );
                completed = true;
                break;
            }
        }
        assert!(completed, "MCP get_download did not observe Completed status in time");

        drop(stdin);
        let _ = child.wait();
        download_id
    })
    .await
    .unwrap();

    assert!(allowed_dir.join("mcp_payload.bin").exists());
    assert!(!forbidden_dir.join("mcp_payload.bin").exists());

    // Confirm audit logs captured both denial and completion download_id
    let conn = manager.get_db_conn().await;
    let logs = list_logs(&conn, 50).unwrap();
    assert!(logs.iter().any(|l| l.message.contains("DENIED(allowlist)")));
    assert!(logs.iter().any(|l| l.message.contains(&dl_id) || l.message.contains("ALLOWED action=add_download")));
}

#[tokio::test]
async fn test_m9_cli_commands_json_and_exit_codes() {
    let cli_bin = cli_binary_path();
    assert!(cli_bin.exists());

    let tmp = tempdir().unwrap();
    let missing_api_json = tmp.path().join("nonexistent_api.json");

    // 1. Missing api.json -> Exit code 3 ("app tidak berjalan")
    let bin1 = cli_bin.clone();
    let code_not_running = tokio::task::spawn_blocking(move || {
        Command::new(&bin1)
            .args(["list", "--json"])
            .env("TURBOGET_API_JSON", &missing_api_json)
            .output()
            .unwrap()
            .status
            .code()
    })
    .await
    .unwrap();
    assert_eq!(code_not_running, Some(3));

    // Start Core + LocalApi
    let total_bytes: u64 = 128 * 1024;
    let server = start_test_server(TestServerConfig {
        total_bytes,
        filename: "cli_test.bin".to_string(),
        ..Default::default()
    })
    .await;

    let allowed_dir = tmp.path().join("cli_allowed");
    std::fs::create_dir_all(&allowed_dir).unwrap();
    let api_json_path = tmp.path().join("api.json");

    let manager = DownloadManager::new_in_memory().unwrap();
    {
        let conn = manager.get_db_conn().await;
        save_agent_policy(
            &conn,
            &AgentAccessPolicy {
                mode: AgentAccessMode::Allowlist,
                allowed_dirs: vec![allowed_dir.to_string_lossy().to_string()],
                block_private_ips: false,
                max_requests_per_minute: 30,
            },
        )
        .unwrap();
    }
    let _api = start_local_api(manager, Some(api_json_path.clone()))
        .await
        .unwrap();

    let srv_url = server.base_url();

    // 2. Folder outside allowlist -> Exit code 4 ("ditolak izin")
    let bin2 = cli_bin.clone();
    let api2 = api_json_path.clone();
    let url2 = srv_url.clone();
    let code_denied = tokio::task::spawn_blocking(move || {
        Command::new(&bin2)
            .args(["add", &url2, "--dir", "C:\\Windows\\System32", "--json"])
            .env("TURBOGET_API_JSON", &api2)
            .output()
            .unwrap()
            .status
            .code()
    })
    .await
    .unwrap();
    assert_eq!(code_denied, Some(4));

    // 3. `turboget probe <url> --json` & `turboget add <url> --dir <allowed> --json` -> Exit code 0
    let bin3 = cli_bin.clone();
    let api3 = api_json_path.clone();
    let url3 = srv_url.clone();
    let dir3 = allowed_dir.to_string_lossy().to_string();
    tokio::task::spawn_blocking(move || {
        let probe_out = Command::new(&bin3)
            .args(["probe", &url3, "--json"])
            .env("TURBOGET_API_JSON", &api3)
            .output()
            .unwrap();
        assert_eq!(probe_out.status.code(), Some(0));
        let probe_json: serde_json::Value = serde_json::from_slice(&probe_out.stdout).unwrap();
        assert_eq!(probe_json["total_bytes"].as_u64(), Some(total_bytes));

        let add_out = Command::new(&bin3)
            .args(["add", &url3, "--dir", &dir3, "--segments", "2", "--json"])
            .env("TURBOGET_API_JSON", &api3)
            .output()
            .unwrap();
        assert_eq!(add_out.status.code(), Some(0));
        let add_json: serde_json::Value = serde_json::from_slice(&add_out.stdout).unwrap();
        let dl_id = add_json["id"].as_str().unwrap();

        let status_out = Command::new(&bin3)
            .args(["status", dl_id, "--json"])
            .env("TURBOGET_API_JSON", &api3)
            .output()
            .unwrap();
        assert_eq!(status_out.status.code(), Some(0));
        let status_json: serde_json::Value = serde_json::from_slice(&status_out.stdout).unwrap();
        assert_eq!(status_json["download"]["id"].as_str(), Some(dl_id));
    })
    .await
    .unwrap();
}
