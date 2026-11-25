use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_MESSAGE_BYTES: usize = 1024 * 1024; // 1 MiB limit per Chrome Native Messaging spec

#[derive(Debug, Deserialize)]
struct ApiConnectionInfo {
    port: u16,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HostRequest {
    Ping,
    AddDownload {
        payload: AddDownloadPayload,
    },
    Probe {
        payload: ProbePayload,
    },
    MediaProbe {
        payload: ProbePayload,
    },
    MediaDownload {
        payload: MediaDownloadPayload,
    },
    ListDownloads,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddDownloadPayload {
    url: String,
    file_name: Option<String>,
    save_dir: Option<String>,
    cookies: Option<String>,
    referrer: Option<String>,
    user_agent: Option<String>,
    #[allow(dead_code)]
    mime: Option<String>,
    segments: Option<u32>,
    start_now: Option<bool>,
    #[serde(default)]
    ask_save_location: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbePayload {
    url: String,
    cookies: Option<String>,
    referrer: Option<String>,
    user_agent: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaDownloadPayload {
    url: String,
    format_id: Option<String>,
    audio_only: Option<bool>,
    save_dir: Option<String>,
    file_name: Option<String>,
    cookies: Option<String>,
    referrer: Option<String>,
    user_agent: Option<String>,
    concurrency: Option<u8>,
    #[serde(default)]
    ask_save_location: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    core_running: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl HostResponse {
    fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            id: None,
            core_running: None,
            version: None,
            data: None,
            error: Some(msg.into()),
        }
    }
}

fn default_api_json_path() -> PathBuf {
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

fn read_api_info() -> Result<ApiConnectionInfo, String> {
    let path = default_api_json_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("TurboGet Core not running or api.json missing ({}): {}", path.display(), e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Invalid api.json format: {}", e))
}

fn http_local_request(
    info: &ApiConnectionInfo,
    method: &str,
    path: &str,
    json_body: Option<&str>,
) -> Result<(u16, String), String> {
    let addr = format!("127.0.0.1:{}", info.port);
    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| format!("Failed to connect to TurboGet Core at {}: {}", addr, e))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let body_bytes = json_body.map(|s| s.as_bytes()).unwrap_or(&[]);
    let mut req = format!(
        "{} {} HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Authorization: Bearer {}\r\n\
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
        .map_err(|e| format!("Failed sending request to Core: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| format!("Failed reading Core response status: {}", e))?;

    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| format!("Malformed HTTP status line: {}", status_line.trim()))?;

    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("Failed reading Core response headers: {}", e))?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("Content-Length") {
                content_length = v.trim().parse::<usize>().ok();
            }
        }
    }

    let body = if let Some(len) = content_length {
        let mut buf = vec![0u8; len];
        reader
            .read_exact(&mut buf)
            .map_err(|e| format!("Failed reading Core response body: {}", e))?;
        String::from_utf8_lossy(&buf).to_string()
    } else {
        let mut buf = String::new();
        reader
            .read_to_string(&mut buf)
            .map_err(|e| format!("Failed reading Core response stream: {}", e))?;
        buf
    };

    Ok((status_code, body))
}

fn validate_url(url: &str) -> Result<(), String> {
    let lower = url.trim().to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("ftp://")
        || lower.starts_with("magnet:")
    {
        Ok(())
    } else {
        Err("Only http://, https://, ftp://, or magnet: URLs are supported".to_string())
    }
}

fn handle_request(raw_json: &str) -> HostResponse {
    let req: HostRequest = match serde_json::from_str(raw_json) {
        Ok(r) => r,
        Err(e) => return HostResponse::error(format!("Invalid message schema: {}", e)),
    };

    match req {
        HostRequest::Ping => {
            let core_running = match read_api_info() {
                Ok(info) => matches!(http_local_request(&info, "GET", "/v1/queues", None), Ok((200, _))),
                Err(_) => false,
            };
            HostResponse {
                ok: true,
                id: None,
                core_running: Some(core_running),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                data: None,
                error: None,
            }
        }
        HostRequest::AddDownload { payload } => {
            if let Err(e) = validate_url(&payload.url) {
                return HostResponse::error(e);
            }
            let info = match read_api_info() {
                Ok(i) => i,
                Err(e) => return HostResponse::error(e),
            };

            let body = serde_json::json!({
                "url": payload.url,
                "file_name": payload.file_name,
                "save_dir": payload.save_dir,
                "cookies": payload.cookies,
                "referrer": payload.referrer,
                "user_agent": payload.user_agent,
                "segments": payload.segments,
                "start_now": payload.start_now.unwrap_or(true),
                "prompt_user": payload.ask_save_location.unwrap_or(false),
            });

            match http_local_request(&info, "POST", "/v1/downloads", Some(&body.to_string())) {
                Ok((status, resp_body)) if (200..300).contains(&status) => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&resp_body).unwrap_or(serde_json::Value::Null);
                    let id = parsed.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                    HostResponse {
                        ok: true,
                        id,
                        core_running: Some(true),
                        version: None,
                        data: Some(parsed),
                        error: None,
                    }
                }
                Ok((status, resp_body)) => {
                    HostResponse::error(format!("Core returned HTTP {}: {}", status, resp_body))
                }
                Err(e) => HostResponse::error(e),
            }
        }
        HostRequest::Probe { payload } => {
            if let Err(e) = validate_url(&payload.url) {
                return HostResponse::error(e);
            }
            let info = match read_api_info() {
                Ok(i) => i,
                Err(e) => return HostResponse::error(e),
            };

            let body = serde_json::json!({
                "url": payload.url,
                "cookies": payload.cookies,
                "referrer": payload.referrer,
                "user_agent": payload.user_agent,
            });

            match http_local_request(&info, "POST", "/v1/probe", Some(&body.to_string())) {
                Ok((status, resp_body)) if (200..300).contains(&status) => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&resp_body).unwrap_or(serde_json::Value::Null);
                    HostResponse {
                        ok: true,
                        id: None,
                        core_running: Some(true),
                        version: None,
                        data: Some(parsed),
                        error: None,
                    }
                }
                Ok((status, resp_body)) => {
                    HostResponse::error(format!("Probe failed HTTP {}: {}", status, resp_body))
                }
                Err(e) => HostResponse::error(e),
            }
        }
        HostRequest::ListDownloads => {
            let info = match read_api_info() {
                Ok(i) => i,
                Err(e) => return HostResponse::error(e),
            };
            match http_local_request(&info, "GET", "/v1/downloads", None) {
                Ok((status, resp_body)) if (200..300).contains(&status) => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&resp_body).unwrap_or(serde_json::Value::Null);
                    HostResponse {
                        ok: true,
                        id: None,
                        core_running: Some(true),
                        version: None,
                        data: Some(parsed),
                        error: None,
                    }
                }
                Ok((status, resp_body)) => {
                    HostResponse::error(format!("Core returned HTTP {}: {}", status, resp_body))
                }
                Err(e) => HostResponse::error(e),
            }
        }
        HostRequest::MediaProbe { payload } => {
            if let Err(e) = validate_url(&payload.url) {
                return HostResponse::error(e);
            }
            let info = match read_api_info() {
                Ok(i) => i,
                Err(e) => return HostResponse::error(e),
            };
            let body = serde_json::json!({
                "url": payload.url,
                "cookies": payload.cookies,
                "referrer": payload.referrer,
                "user_agent": payload.user_agent,
            });
            match http_local_request(&info, "POST", "/v1/media/probe", Some(&body.to_string())) {
                Ok((status, resp_body)) if (200..300).contains(&status) => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&resp_body).unwrap_or(serde_json::Value::Null);
                    HostResponse {
                        ok: true,
                        id: None,
                        core_running: Some(true),
                        version: None,
                        data: Some(parsed),
                        error: None,
                    }
                }
                Ok((status, resp_body)) => {
                    HostResponse::error(format!("Media probe failed HTTP {}: {}", status, resp_body))
                }
                Err(e) => HostResponse::error(e),
            }
        }
        HostRequest::MediaDownload { payload } => {
            if let Err(e) = validate_url(&payload.url) {
                return HostResponse::error(e);
            }
            let info = match read_api_info() {
                Ok(i) => i,
                Err(e) => return HostResponse::error(e),
            };
            let body = serde_json::json!({
                "url": payload.url,
                "format_id": payload.format_id,
                "audio_only": payload.audio_only.unwrap_or(false),
                "save_dir": payload.save_dir,
                "file_name": payload.file_name,
                "cookies": payload.cookies,
                "referrer": payload.referrer,
                "user_agent": payload.user_agent,
                "concurrency": payload.concurrency,
                "prompt_user": payload.ask_save_location.unwrap_or(false),
            });
            match http_local_request(&info, "POST", "/v1/media/download", Some(&body.to_string())) {
                Ok((status, resp_body)) if (200..300).contains(&status) => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&resp_body).unwrap_or(serde_json::Value::Null);
                    let id = parsed.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                    HostResponse {
                        ok: true,
                        id,
                        core_running: Some(true),
                        version: None,
                        data: Some(parsed),
                        error: None,
                    }
                }
                Ok((status, resp_body)) => {
                    HostResponse::error(format!("Media download failed HTTP {}: {}", status, resp_body))
                }
                Err(e) => HostResponse::error(e),
            }
        }
    }
}

/// Read one Native Messaging frame (4-byte native-endian length + UTF-8 JSON)
fn read_native_message<R: Read>(reader: &mut R) -> io::Result<Option<String>> {
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let len = u32::from_ne_bytes(len_bytes) as usize;
    if len == 0 || len > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid message length: {}", len),
        ));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write one Native Messaging frame (4-byte native-endian length + UTF-8 JSON)
fn write_native_message<W: Write>(writer: &mut W, resp: &HostResponse) -> io::Result<()> {
    let json = serde_json::to_string(resp).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let bytes = json.as_bytes();
    let len = bytes.len() as u32;
    writer.write_all(&len.to_ne_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}

/// Generate Chrome/Edge & Firefox native messaging host manifests and register on Windows
fn register_native_host(extension_id: &str, manifest_dir: Option<&Path>) -> Result<(), String> {
    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let out_dir = manifest_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| exe_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf());
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let chrome_manifest_path = out_dir.join("com.turboget.host.json");
    let chrome_manifest = serde_json::json!({
        "name": "com.turboget.host",
        "description": "TurboGet Download Manager Native Messaging Host",
        "path": exe_path.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [
            format!("chrome-extension://{}/", extension_id.trim())
        ]
    });

    std::fs::write(
        &chrome_manifest_path,
        serde_json::to_string_pretty(&chrome_manifest).unwrap(),
    )
    .map_err(|e| e.to_string())?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let path_str = chrome_manifest_path.to_string_lossy().to_string();
        for key in [
            r"HKCU\Software\Google\Chrome\NativeMessagingHosts\com.turboget.host",
            r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.turboget.host",
            r"HKCU\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.turboget.host",
        ] {
            let out = Command::new("reg")
                .args(["add", key, "/ve", "/t", "REG_SZ", "/d", &path_str, "/f"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| format!("Failed invoking reg.exe: {}", e))?;
            if !out.status.success() {
                return Err(format!(
                    "Registry write failed for {}: {}",
                    key,
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
    }

    println!("Registered com.turboget.host manifest at {}", chrome_manifest_path.display());
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--register") {
        let ext_id = args
            .get(pos + 1)
            .expect("Usage: turboget-host --register <CHROME_EXTENSION_ID> [--manifest-dir <DIR>]");
        let manifest_dir = args
            .iter()
            .position(|a| a == "--manifest-dir")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from);
        if let Err(e) = register_native_host(ext_id, manifest_dir.as_deref()) {
            eprintln!("Registration failed: {}", e);
            std::process::exit(1);
        }
        return;
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut in_lock = stdin.lock();
    let mut out_lock = stdout.lock();

    loop {
        match read_native_message(&mut in_lock) {
            Ok(Some(msg)) => {
                let resp = handle_request(&msg);
                if let Err(e) = write_native_message(&mut out_lock, &resp) {
                    eprintln!("Failed writing native message: {}", e);
                    break;
                }
            }
            Ok(None) => break, // EOF
            Err(e) => {
                let _ = write_native_message(&mut out_lock, &HostResponse::error(e.to_string()));
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_native_messaging_stdio_framing_roundtrip() {
        let resp = HostResponse {
            ok: true,
            id: Some("dl-123".to_string()),
            core_running: Some(true),
            version: None,
            data: None,
            error: None,
        };

        let mut buf = Vec::new();
        write_native_message(&mut buf, &resp).unwrap();

        let mut cursor = Cursor::new(buf);
        let read_back = read_native_message(&mut cursor).unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&read_back).unwrap();
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["id"], "dl-123");
    }

    #[test]
    fn test_schema_validation_rejects_bad_scheme() {
        let bad_req = r#"{"type":"add_download","payload":{"url":"javascript:alert(1)"}}"#;
        let resp = handle_request(bad_req);
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("Only http://"));
    }
}
