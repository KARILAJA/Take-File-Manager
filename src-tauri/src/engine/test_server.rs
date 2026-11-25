use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct TestServerConfig {
    pub total_bytes: u64,
    pub drop_after_bytes: Arc<AtomicU64>,
    pub no_range: Arc<AtomicBool>,
    pub delay_ms_per_chunk: Arc<AtomicU64>,
    pub etag: String,
    pub filename: String,
    pub required_cookie: Option<String>,
}

impl Default for TestServerConfig {
    fn default() -> Self {
        Self {
            total_bytes: 100 * 1024 * 1024, // 100 MB
            drop_after_bytes: Arc::new(AtomicU64::new(0)),
            no_range: Arc::new(AtomicBool::new(false)),
            delay_ms_per_chunk: Arc::new(AtomicU64::new(0)),
            etag: "turboget-test-etag-v1".to_string(),
            filename: "turboget_100mb.bin".to_string(),
            required_cookie: None,
        }
    }
}

pub struct TestServerHandle {
    pub port: u16,
    pub config: Arc<TestServerConfig>,
    pub shutdown: Arc<AtomicBool>,
}

impl TestServerHandle {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/download", self.port)
    }

    pub fn set_drop_after(&self, bytes: u64) {
        self.config.drop_after_bytes.store(bytes, Ordering::SeqCst);
    }
}

/// Compute the expected SHA-256 hash of the deterministic test stream
pub fn compute_expected_hash(total_bytes: u64) -> String {
    let mut hasher = Sha256::new();
    let chunk_size = 64 * 1024;
    let mut buffer = vec![0u8; chunk_size];
    let mut offset = 0u64;

    while offset < total_bytes {
        let remaining = total_bytes - offset;
        let to_generate = (chunk_size as u64).min(remaining) as usize;
        for (i, byte) in buffer.iter_mut().enumerate().take(to_generate) {
            *byte = ((offset + i as u64) % 251) as u8;
        }
        hasher.update(&buffer[..to_generate]);
        offset += to_generate as u64;
    }

    format!("{:x}", hasher.finalize())
}

/// Start an in-process mock HTTP test server on an ephemeral port
pub async fn start_test_server(config: TestServerConfig) -> TestServerHandle {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(config);
    let shutdown = Arc::new(AtomicBool::new(false));

    let srv_config = config.clone();
    let srv_shutdown = shutdown.clone();

    tokio::spawn(async move {
        loop {
            if srv_shutdown.load(Ordering::Relaxed) {
                break;
            }

            let accept_res = tokio::select! {
                res = listener.accept() => res,
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {
                    continue;
                }
            };

            if let Ok((mut socket, _)) = accept_res {
                let cfg = srv_config.clone();
                tokio::spawn(async move {
                    let mut req_buf = vec![0u8; 4096];
                    let n = match socket.read(&mut req_buf).await {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    let req_str = String::from_utf8_lossy(&req_buf[..n]);

                    if let Some(ref req_cookie) = cfg.required_cookie {
                        let has_cookie = req_str.lines().any(|line| {
                            let lower = line.to_lowercase();
                            lower.starts_with("cookie:")
                                && line["cookie:".len()..].trim().contains(req_cookie.as_str())
                        });
                        if !has_cookie {
                            let resp = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                            let _ = socket.write_all(resp.as_bytes()).await;
                            return;
                        }
                    }

                    let is_head = req_str.starts_with("HEAD");
                    let total = cfg.total_bytes;
                    let no_range = cfg.no_range.load(Ordering::Relaxed);

                    // Parse Range header
                    let mut range_start = 0u64;
                    let mut range_end = total.saturating_sub(1);
                    let mut has_range = false;

                    if !no_range {
                        for line in req_str.lines() {
                            if line.to_lowercase().starts_with("range: bytes=") {
                                let spec = line["range: bytes=".len()..].trim();
                                let parts: Vec<&str> = spec.split('-').collect();
                                if let Ok(s) = parts[0].parse::<u64>() {
                                    range_start = s;
                                    has_range = true;
                                }
                                if parts.len() > 1 && !parts[1].is_empty() {
                                    if let Ok(e) = parts[1].parse::<u64>() {
                                        range_end = e.min(total - 1);
                                    }
                                }
                            }
                        }
                    }

                    let accept_ranges_hdr = if no_range {
                        "Accept-Ranges: none\r\n"
                    } else {
                        "Accept-Ranges: bytes\r\n"
                    };

                    if is_head {
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\n\
Content-Length: {}\r\n\
{}\
ETag: \"{}\"\r\n\
Content-Type: application/octet-stream\r\n\
Content-Disposition: attachment; filename=\"{}\"\r\n\
Connection: close\r\n\r\n",
                            total, accept_ranges_hdr, cfg.etag, cfg.filename
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                        return;
                    }

                    let (status_line, content_len, header_range) = if has_range {
                        let len = if range_end >= range_start {
                            range_end - range_start + 1
                        } else {
                            0
                        };
                        (
                            "HTTP/1.1 206 Partial Content",
                            len,
                            format!("Content-Range: bytes {}-{}/{}\r\n", range_start, range_end, total),
                        )
                    } else {
                        (
                            "HTTP/1.1 200 OK",
                            total,
                            String::new(),
                        )
                    };

                    let header = format!(
                        "{}\r\n\
Content-Length: {}\r\n\
{}\
{}\
ETag: \"{}\"\r\n\
Content-Type: application/octet-stream\r\n\
Content-Disposition: attachment; filename=\"{}\"\r\n\
Connection: close\r\n\r\n",
                        status_line, content_len, accept_ranges_hdr, header_range, cfg.etag, cfg.filename
                    );

                    if socket.write_all(header.as_bytes()).await.is_err() {
                        return;
                    }

                    // Stream payload
                    let drop_limit = cfg.drop_after_bytes.load(Ordering::SeqCst);
                    let mut bytes_sent = 0u64;
                    let chunk_size = 64 * 1024;
                    let mut current_offset = range_start;

                    while current_offset <= range_end {
                        let remaining = range_end - current_offset + 1;
                        let to_send = (chunk_size as u64).min(remaining) as usize;

                        // Check if we need to simulate connection disconnect
                        if drop_limit > 0 && bytes_sent + to_send as u64 > drop_limit {
                            let partial = (drop_limit - bytes_sent) as usize;
                            if partial > 0 {
                                let mut buf = vec![0u8; partial];
                                for (i, byte) in buf.iter_mut().enumerate().take(partial) {
                                    *byte = ((current_offset + i as u64) % 251) as u8;
                                }
                                let _ = socket.write_all(&buf).await;
                            }
                            // Abruptly drop connection
                            let _ = socket.shutdown().await;
                            return;
                        }

                        let mut buf = vec![0u8; to_send];
                        for (i, byte) in buf.iter_mut().enumerate().take(to_send) {
                            *byte = ((current_offset + i as u64) % 251) as u8;
                        }

                        if socket.write_all(&buf).await.is_err() {
                            return;
                        }

                        bytes_sent += to_send as u64;
                        current_offset += to_send as u64;

                        let delay = cfg.delay_ms_per_chunk.load(Ordering::Relaxed);
                        if delay > 0 {
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        }
                    }
                });
            }
        }
    });

    TestServerHandle {
        port,
        config,
        shutdown,
    }
}
