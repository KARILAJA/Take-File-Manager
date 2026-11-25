use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = 8088;
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    println!("TurboGet Mock Test Server running at http://127.0.0.1:{}", port);
    println!("Simulates 100MB download with Range support & abrupt disconnects");

    let total_bytes: u64 = 100 * 1024 * 1024;
    let drop_limit = Arc::new(AtomicU64::new(0));

    loop {
        let (mut socket, addr) = listener.accept().await?;
        let drop_limit = drop_limit.clone();
        tokio::spawn(async move {
            let mut req_buf = vec![0u8; 4096];
            let n = match socket.read(&mut req_buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };
            let req_str = String::from_utf8_lossy(&req_buf[..n]);

            let is_head = req_str.starts_with("HEAD");
            let mut range_start = 0u64;
            let mut range_end = total_bytes.saturating_sub(1);
            let mut has_range = false;

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
                            range_end = e.min(total_bytes - 1);
                        }
                    }
                }
            }

            if is_head {
                let resp = format!(
                    "HTTP/1.1 200 OK\r\n\
                    Content-Length: {}\r\n\
                    Accept-Ranges: bytes\r\n\
                    ETag: \"turboget-test-etag\"\r\n\
                    Content-Type: application/octet-stream\r\n\
                    Content-Disposition: attachment; filename=\"test_100mb.bin\"\r\n\
                    Connection: close\r\n\r\n",
                    total_bytes
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
                    format!("Content-Range: bytes {}-{}/{}\r\n", range_start, range_end, total_bytes),
                )
            } else {
                (
                    "HTTP/1.1 200 OK",
                    total_bytes,
                    String::new(),
                )
            };

            let header = format!(
                "{}\r\n\
                Content-Length: {}\r\n\
                Accept-Ranges: bytes\r\n\
                {}\
                ETag: \"turboget-test-etag\"\r\n\
                Content-Type: application/octet-stream\r\n\
                Content-Disposition: attachment; filename=\"test_100mb.bin\"\r\n\
                Connection: close\r\n\r\n",
                status_line, content_len, header_range
            );

            if socket.write_all(header.as_bytes()).await.is_err() {
                return;
            }

            let mut current_offset = range_start;
            let chunk_size = 64 * 1024;
            while current_offset <= range_end {
                let remaining = range_end - current_offset + 1;
                let to_send = (chunk_size as u64).min(remaining) as usize;
                let mut buf = vec![0u8; to_send];
                for i in 0..to_send {
                    buf[i] = ((current_offset + i as u64) % 251) as u8;
                }
                if socket.write_all(&buf).await.is_err() {
                    return;
                }
                current_offset += to_send as u64;
            }
        });
    }
}
