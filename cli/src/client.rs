use serde::Deserialize;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct ApiConnectionInfo {
    pub port: u16,
    pub token: String,
}

#[derive(Debug)]
pub enum ClientError {
    AppNotRunning(String),
    PermissionDenied(String),
    ApiError(u16, String),
}

impl ClientError {
    pub fn exit_code(&self) -> i32 {
        match self {
            ClientError::AppNotRunning(_) => 3,
            ClientError::PermissionDenied(_) => 4,
            ClientError::ApiError(_, _) => 1,
        }
    }

    pub fn message(&self) -> String {
        match self {
            ClientError::AppNotRunning(m) => format!("TurboGet is not running: {}", m),
            ClientError::PermissionDenied(m) => format!("Permission denied: {}", m),
            ClientError::ApiError(code, m) => format!("API HTTP {}: {}", code, m),
        }
    }
}

pub fn api_json_path() -> PathBuf {
    if let Ok(override_path) = std::env::var("TURBOGET_API_JSON") {
        if !override_path.trim().is_empty() {
            return PathBuf::from(override_path);
        }
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("turboget")
        .join("api.json")
}

pub fn read_api_info() -> Result<ApiConnectionInfo, ClientError> {
    let path = api_json_path();
    let content = std::fs::read_to_string(&path).map_err(|e| {
        ClientError::AppNotRunning(format!("cannot read {}: {}", path.display(), e))
    })?;
    serde_json::from_str(&content)
        .map_err(|e| ClientError::AppNotRunning(format!("invalid api.json: {}", e)))
}

pub fn request(
    method: &str,
    path: &str,
    json_body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, ClientError> {
    let info = read_api_info()?;
    let addr = format!("127.0.0.1:{}", info.port);
    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| ClientError::AppNotRunning(format!("connect {}: {}", addr, e)))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(20)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let body_str = json_body.map(|v| v.to_string()).unwrap_or_default();
    let body_bytes = body_str.as_bytes();

    let mut req = format!(
        "{} {} HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Authorization: Bearer {}\r\n\
         X-TurboGet-Actor: agent\r\n\
         Connection: close\r\n",
        method, path, info.port, info.token
    );

    if !body_bytes.is_empty() {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body_bytes.len()
        ));
    }
    req.push_str("\r\n");

    stream
        .write_all(req.as_bytes())
        .and_then(|_| stream.write_all(body_bytes))
        .and_then(|_| stream.flush())
        .map_err(|e| ClientError::AppNotRunning(format!("write {}: {}", addr, e)))?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| ClientError::AppNotRunning(format!("read status: {}", e)))?;

    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| ClientError::ApiError(500, format!("Malformed status: {}", status_line.trim())))?;

    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| ClientError::ApiError(status_code, e.to_string()))?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("Content-Length") {
                content_length = v.trim().parse::<usize>().ok();
            }
        }
    }

    let raw_body = if let Some(len) = content_length {
        let mut buf = vec![0u8; len];
        reader
            .read_exact(&mut buf)
            .map_err(|e| ClientError::ApiError(status_code, e.to_string()))?;
        String::from_utf8_lossy(&buf).to_string()
    } else {
        let mut buf = String::new();
        reader
            .read_to_string(&mut buf)
            .map_err(|e| ClientError::ApiError(status_code, e.to_string()))?;
        buf
    };

    if status_code == 401 || status_code == 403 {
        return Err(ClientError::PermissionDenied(raw_body));
    }
    if !(200..300).contains(&status_code) {
        return Err(ClientError::ApiError(status_code, raw_body));
    }

    if raw_body.trim().is_empty() {
        Ok(serde_json::json!({ "ok": true }))
    } else {
        Ok(serde_json::from_str(&raw_body).unwrap_or_else(|_| serde_json::json!({ "raw": raw_body })))
    }
}

pub fn url_encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}
