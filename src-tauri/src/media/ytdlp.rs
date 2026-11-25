use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::engine::filename::sanitize_filename;
use crate::engine::probe::probe_url;
use crate::media::dash::{download_dash_representation, parse_mpd};
use crate::media::hls::{download_hls_media_playlist, fetch_text, parse_m3u8, HlsPlaylist};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn apply_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

static YTDLP_INSTALL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn install_lock() -> &'static Mutex<()> {
    YTDLP_INSTALL_LOCK.get_or_init(|| Mutex::new(()))
}

struct TempWorkDir(PathBuf);

impl TempWorkDir {
    fn create(parent: &Path) -> Result<Self> {
        let dir = parent.join(format!(".turboget_tmp_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempWorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub fps: Option<f64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub filesize: Option<u64>,
    #[serde(default)]
    pub is_filesize_estimated: bool,
    pub tbr: Option<f64>,
    pub format_note: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    pub url: Option<String>,
    pub has_video: bool,
    pub has_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProbeResult {
    pub title: String,
    pub url: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub extractor: Option<String>,
    pub formats: Vec<MediaFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDownloadRequest {
    pub url: String,
    pub format_id: Option<String>,
    pub audio_only: bool,
    pub save_dir: PathBuf,
    pub file_name: Option<String>,
    pub headers: Vec<(String, String)>,
    pub concurrency: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct MediaProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaToolsStatus {
    pub ytdlp_installed: bool,
    pub ytdlp_path: Option<String>,
    pub ffmpeg_installed: bool,
    pub ffmpeg_path: Option<String>,
}

pub fn managed_bin_dir() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("turboget")
        .join("bin")
}

/// Discover sidecar, managed, or system binaries (`yt-dlp`, `ffmpeg`)
pub fn find_binary(name: &str, env_var: &str) -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var(env_var) {
        let p = PathBuf::from(override_path.trim());
        if p.exists() {
            return Some(p);
        }
    }

    let exe_name = if cfg!(windows) && !name.ends_with(".exe") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };

    // 1. Check managed TFM bin directory (%APPDATA%\turboget\bin)
    let managed = managed_bin_dir().join(&exe_name);
    if managed.exists() {
        return Some(managed);
    }

    // 2. Check alongside current executable (Tauri bundled sidecar location)
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            let direct = dir.join(&exe_name);
            if direct.exists() {
                return Some(direct);
            }
            if cfg!(windows) {
                let target_triple = dir.join(format!("{}-x86_64-pc-windows-msvc.exe", name));
                if target_triple.exists() {
                    return Some(target_triple);
                }
            }
        }
    }

    // 3. Search system PATH
    if let Ok(path_env) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_env) {
            let candidate = dir.join(&exe_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 4. Common Windows package manager locations (WinGet / Scoop / Chocolatey)
    #[cfg(windows)]
    {
        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            let winget_link = Path::new(&local_app)
                .join("Microsoft\\WinGet\\Links")
                .join(&exe_name);
            if winget_link.exists() {
                return Some(winget_link);
            }
            let pkgs_dir = Path::new(&local_app).join("Microsoft\\WinGet\\Packages");
            if let Ok(entries) = std::fs::read_dir(&pkgs_dir) {
                for entry in entries.flatten() {
                    let pkg = entry.path();
                    if !pkg.is_dir() {
                        continue;
                    }
                    let direct = pkg.join(&exe_name);
                    if direct.exists() {
                        return Some(direct);
                    }
                    if let Ok(sub_entries) = std::fs::read_dir(&pkg) {
                        for sub in sub_entries.flatten() {
                            let sub_p = sub.path();
                            if sub_p.join(&exe_name).exists() {
                                return Some(sub_p.join(&exe_name));
                            }
                            if sub_p.join("bin").join(&exe_name).exists() {
                                return Some(sub_p.join("bin").join(&exe_name));
                            }
                        }
                    }
                }
            }
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let scoop = Path::new(&profile).join("scoop\\shims").join(&exe_name);
            if scoop.exists() {
                return Some(scoop);
            }
        }
        let choco = Path::new(r"C:\ProgramData\chocolatey\bin").join(&exe_name);
        if choco.exists() {
            return Some(choco);
        }
        if name.eq_ignore_ascii_case("node") {
            let prog_files = Path::new(r"C:\Program Files\nodejs\node.exe");
            if prog_files.exists() {
                return Some(prog_files.to_path_buf());
            }
        }
    }

    None
}

fn find_js_runtime_arg() -> Option<String> {
    if let Some(deno) = find_binary("deno", "TURBOGET_DENO_PATH") {
        return Some(format!("deno:{}", deno.to_string_lossy()));
    }
    if let Some(node) = find_binary("node", "TURBOGET_NODE_PATH") {
        return Some(format!("node:{}", node.to_string_lossy()));
    }
    None
}

/// Filters headers before passing them to `yt-dlp --add-header`.
/// `yt-dlp` deprecates/breaks YouTube extraction when `Cookie` is injected as a raw header
/// or when desktop browser `User-Agent` conflicts with internal YouTube player clients.
pub fn filter_ytdlp_headers<'a>(
    url: &str,
    headers: &'a [(String, String)],
) -> Vec<(&'a str, &'a str)> {
    let is_extractor_site = is_video_page_url(url);
    headers
        .iter()
        .filter(|(k, v)| {
            if v.trim().is_empty() {
                return false;
            }
            let low = k.trim().to_ascii_lowercase();
            if low == "cookie" {
                return false;
            }
            if is_extractor_site && low == "user-agent" {
                return false;
            }
            true
        })
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

fn build_ytdlp_base_cmd(ytdlp_bin: &Path, url: &str, headers: &[(String, String)]) -> Command {
    let mut cmd = Command::new(ytdlp_bin);
    apply_no_window(&mut cmd);
    cmd.args(["--no-playlist", "--no-warnings"]);
    if let Some(rt) = find_js_runtime_arg() {
        cmd.args(["--js-runtimes", &rt]);
    }
    for (k, v) in filter_ytdlp_headers(url, headers) {
        cmd.args(["--add-header", &format!("{}: {}", k, v)]);
    }
    cmd
}

pub fn get_media_tools_status() -> MediaToolsStatus {
    let ytdlp = find_binary("yt-dlp", "TURBOGET_YTDLP_PATH");
    let ffmpeg = find_binary("ffmpeg", "TURBOGET_FFMPEG_PATH");
    MediaToolsStatus {
        ytdlp_installed: ytdlp.is_some(),
        ytdlp_path: ytdlp.map(|p| p.to_string_lossy().to_string()),
        ffmpeg_installed: ffmpeg.is_some(),
        ffmpeg_path: ffmpeg.map(|p| p.to_string_lossy().to_string()),
    }
}

/// Ensures `yt-dlp` binary is available, automatically downloading the official release
/// into `%APPDATA%\turboget\bin\yt-dlp.exe` if missing on Windows/macOS/Linux.
pub async fn ensure_ytdlp_binary(client: &Client) -> Result<PathBuf> {
    if let Some(existing) = find_binary("yt-dlp", "TURBOGET_YTDLP_PATH") {
        return Ok(existing);
    }

    let _guard = install_lock().lock().await;
    if let Some(existing) = find_binary("yt-dlp", "TURBOGET_YTDLP_PATH") {
        return Ok(existing);
    }

    let bin_dir = managed_bin_dir();
    tokio::fs::create_dir_all(&bin_dir).await?;

    let (asset_name, file_name) = if cfg!(windows) {
        ("yt-dlp.exe", "yt-dlp.exe")
    } else if cfg!(target_os = "macos") {
        ("yt-dlp_macos", "yt-dlp")
    } else {
        ("yt-dlp_linux", "yt-dlp")
    };

    let url = format!(
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}",
        asset_name
    );
    let dest = bin_dir.join(file_name);
    let part = bin_dir.join(format!("{}.part", file_name));

    let resp = client.get(&url).send().await.map_err(|e| {
        anyhow!(
            "yt-dlp tidak ditemukan dan gagal mengunduh otomatis dari GitHub ({})",
            e
        )
    })?;

    if !resp.status().is_success() {
        return Err(anyhow!(
            "Gagal mengunduh yt-dlp (HTTP {}). Anda dapat meletakkan yt-dlp.exe di {}",
            resp.status(),
            bin_dir.display()
        ));
    }

    let bytes = resp.bytes().await?;
    if bytes.len() < 1_000_000 {
        return Err(anyhow!("Berkas yt-dlp terunduh tidak valid (terlalu kecil)"));
    }

    tokio::fs::write(&part, &bytes).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        tokio::fs::set_permissions(&part, perms).await?;
    }
    if dest.exists() {
        let _ = tokio::fs::remove_file(&dest).await;
    }
    tokio::fs::rename(&part, &dest).await?;
    Ok(dest)
}

pub fn is_hls_url(url: &str) -> bool {
    let lower = url.split('?').next().unwrap_or(url).to_lowercase();
    lower.ends_with(".m3u8") || lower.contains(".m3u8")
}

pub fn is_dash_url(url: &str) -> bool {
    let lower = url.split('?').next().unwrap_or(url).to_lowercase();
    lower.ends_with(".mpd") || lower.contains(".mpd")
}

pub fn is_direct_media_url(url: &str) -> bool {
    let clean = url
        .split('?')
        .next()
        .unwrap_or(url)
        .split('#')
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    [
        ".mp4", ".webm", ".mkv", ".mov", ".avi", ".flv", ".m4v", ".mp3", ".m4a", ".aac", ".ogg",
        ".flac", ".wav",
    ]
    .iter()
    .any(|ext| clean.ends_with(ext))
}

pub fn is_video_page_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let path = parsed.path().to_ascii_lowercase();

    (host.contains("youtube.com")
        && (path.starts_with("/watch") || path.starts_with("/shorts/") || path.starts_with("/live/")))
        || (host == "youtu.be" && path.len() > 1)
        || (host.contains("tiktok.com") && (path.contains("/video/") || path.contains("/@")))
        || (host.contains("instagram.com")
            && (path.starts_with("/reel/") || path.starts_with("/reels/") || path.starts_with("/p/")))
        || ((host.contains("facebook.com") || host == "fb.watch")
            && (path.contains("/watch")
                || path.contains("/reel")
                || path.contains("/videos/")
                || host == "fb.watch"))
        || ((host.contains("twitter.com") || host.contains("x.com")) && path.contains("/status/"))
        || (host.contains("vimeo.com") && path.len() > 1 && path[1..].chars().all(|c| c.is_ascii_digit()))
        || (host.contains("dailymotion.com") && path.contains("/video/"))
        || (host.contains("twitch.tv") && (path.contains("/videos/") || path.contains("/clip/")))
        || (host.contains("bilibili.com") && path.contains("/video/"))
}

pub fn is_media_download_url(url: &str) -> bool {
    is_hls_url(url) || is_dash_url(url) || is_video_page_url(url)
}

fn infer_title_from_url(url: &str) -> String {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        if let Some(mut segs) = parsed.path_segments() {
            if let Some(last) = segs.rfind(|s| !s.is_empty()) {
                let stem = last.split('.').next().unwrap_or(last);
                if !stem.is_empty() {
                    return sanitize_filename(stem);
                }
            }
        }
    }
    "video".to_string()
}

fn extract_ext_from_name(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "mp4".to_string())
}

/// Probe media URL: handles direct `.m3u8` / `.mpd` streams & direct `.mp4`/`.webm`/`.mp3` natively,
/// and delegates video pages (YouTube, TikTok, etc.) to `yt-dlp` (auto-installing it if missing).
pub async fn probe_media(
    client: &Client,
    url: &str,
    headers: &[(String, String)],
) -> Result<MediaProbeResult> {
    if is_hls_url(url) {
        let text = fetch_text(client, url, headers).await?;
        let mut formats = Vec::new();
        match parse_m3u8(&text, url)? {
            HlsPlaylist::Master { variants, .. } => {
                for (i, v) in variants.iter().enumerate() {
                    let height = v
                        .resolution
                        .as_deref()
                        .and_then(|r| r.split('x').nth(1))
                        .and_then(|h| h.parse::<u32>().ok());
                    let width = v
                        .resolution
                        .as_deref()
                        .and_then(|r| r.split('x').next())
                        .and_then(|w| w.parse::<u32>().ok());
                    formats.push(MediaFormat {
                        format_id: format!("hls-{}", i),
                        ext: "mp4".to_string(),
                        resolution: v.resolution.clone(),
                        width,
                        height,
                        fps: None,
                        vcodec: v.codecs.clone(),
                        acodec: v.codecs.clone(),
                        filesize: None,
                        is_filesize_estimated: false,
                        tbr: Some((v.bandwidth as f64) / 1000.0),
                        format_note: Some("HLS Stream".to_string()),
                        protocol: Some("m3u8".to_string()),
                        url: Some(v.uri.clone()),
                        has_video: true,
                        has_audio: true,
                    });
                }
            }
            HlsPlaylist::Media(_) => {
                formats.push(MediaFormat {
                    format_id: "hls-default".to_string(),
                    ext: "mp4".to_string(),
                    resolution: None,
                    width: None,
                    height: None,
                    fps: None,
                    vcodec: None,
                    acodec: None,
                    filesize: None,
                    is_filesize_estimated: false,
                    tbr: None,
                    format_note: Some("HLS Media Playlist".to_string()),
                    protocol: Some("m3u8".to_string()),
                    url: Some(url.to_string()),
                    has_video: true,
                    has_audio: true,
                });
            }
        }

        return Ok(MediaProbeResult {
            title: infer_title_from_url(url),
            url: url.to_string(),
            thumbnail: None,
            duration: None,
            extractor: Some("hls".to_string()),
            formats,
        });
    }

    if is_dash_url(url) {
        let xml = fetch_text(client, url, headers).await?;
        let manifest = parse_mpd(&xml, url)?;
        let formats = manifest
            .representations
            .into_iter()
            .map(|rep| MediaFormat {
                format_id: rep.id,
                ext: "mp4".to_string(),
                resolution: match (rep.width, rep.height) {
                    (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                    _ => None,
                },
                width: rep.width,
                height: rep.height,
                fps: None,
                vcodec: if rep.content_type == "video" {
                    rep.codecs.clone()
                } else {
                    None
                },
                acodec: if rep.content_type == "audio" {
                    rep.codecs.clone()
                } else {
                    None
                },
                filesize: None,
                is_filesize_estimated: false,
                tbr: Some((rep.bandwidth as f64) / 1000.0),
                format_note: Some(format!("DASH {}", rep.content_type)),
                protocol: Some("dash".to_string()),
                url: None,
                has_video: rep.content_type == "video",
                has_audio: rep.content_type == "audio",
            })
            .collect();

        return Ok(MediaProbeResult {
            title: infer_title_from_url(url),
            url: url.to_string(),
            thumbnail: None,
            duration: None,
            extractor: Some("dash".to_string()),
            formats,
        });
    }

    // Direct video/audio URLs (e.g. .mp4, .webm, .mp3) work natively without requiring yt-dlp
    if is_direct_media_url(url) {
        if let Ok(info) = probe_url(client, url, headers).await {
            let ext = extract_ext_from_name(&info.file_name);
            let is_audio = matches!(ext.as_str(), "mp3" | "m4a" | "aac" | "ogg" | "flac" | "wav");
            let title = Path::new(&info.file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(sanitize_filename)
                .unwrap_or_else(|| infer_title_from_url(url));
            return Ok(MediaProbeResult {
                title,
                url: info.final_url.clone(),
                thumbnail: None,
                duration: None,
                extractor: Some("direct".to_string()),
                formats: vec![MediaFormat {
                    format_id: "direct".to_string(),
                    ext: ext.clone(),
                    resolution: None,
                    width: None,
                    height: None,
                    fps: None,
                    vcodec: if is_audio { None } else { Some("direct".to_string()) },
                    acodec: Some("direct".to_string()),
                    filesize: info.total_bytes,
                    is_filesize_estimated: false,
                    tbr: None,
                    format_note: Some("Unduhan Langsung Multi-Segmen".to_string()),
                    protocol: Some("https".to_string()),
                    url: Some(info.final_url),
                    has_video: !is_audio,
                    has_audio: true,
                }],
            });
        }
    }

    // Fallback: invoke yt-dlp (auto-downloading it to %APPDATA%\turboget\bin if needed)
    let ytdlp_bin = ensure_ytdlp_binary(client).await?;

    let mut cmd = build_ytdlp_base_cmd(&ytdlp_bin, url, headers);
    cmd.arg("--dump-json").arg(url);

    let out = cmd.output().await?;
    if !out.status.success() {
        return Err(anyhow!(
            "yt-dlp probe failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    let val: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    parse_ytdlp_json(&val, url)
}

pub fn parse_ytdlp_json(val: &serde_json::Value, fallback_url: &str) -> Result<MediaProbeResult> {
    let title = val
        .get("title")
        .and_then(|v| v.as_str())
        .map(sanitize_filename)
        .unwrap_or_else(|| "video".to_string());
    let thumbnail = val
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = val.get("duration").and_then(|v| v.as_f64());
    let extractor = val
        .get("extractor")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut formats = Vec::new();
    if let Some(arr) = val.get("formats").and_then(|v| v.as_array()) {
        for f in arr {
            let format_id = match f.get("format_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            let ext = f
                .get("ext")
                .and_then(|v| v.as_str())
                .unwrap_or("mp4")
                .to_string();
            let width = f.get("width").and_then(|v| v.as_u64()).map(|n| n as u32);
            let height = f.get("height").and_then(|v| v.as_u64()).map(|n| n as u32);
            let fps = f.get("fps").and_then(|v| v.as_f64());
            let resolution = f
                .get("resolution")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| match (width, height) {
                    (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                    _ => None,
                });
            let vcodec = f
                .get("vcodec")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let acodec = f
                .get("acodec")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let has_video = vcodec.as_deref().map(|s| s != "none" && !s.is_empty()).unwrap_or(false);
            let has_audio = acodec
                .as_deref()
                .map(|s| s != "none" && !s.is_empty())
                .unwrap_or_else(|| resolution.as_deref() == Some("audio only"));

            // Skip storyboard mhtml thumbnails or non-media tracks
            if ext.eq_ignore_ascii_case("mhtml") || (!has_video && !has_audio) {
                continue;
            }

            let tbr = f.get("tbr").and_then(|v| v.as_f64());
            let exact_size = f.get("filesize").and_then(|v| v.as_u64());
            let approx_size = f.get("filesize_approx").and_then(|v| v.as_u64());
            let (filesize, is_filesize_estimated) = if let Some(sz) = exact_size {
                (Some(sz), false)
            } else if let Some(approx) = approx_size {
                (Some(approx), true)
            } else if let (Some(kbps), Some(dur)) = (tbr, duration) {
                if kbps > 0.0 && dur > 0.0 {
                    (Some(((kbps * 1000.0 / 8.0) * dur) as u64), true)
                } else {
                    (None, false)
                }
            } else {
                (None, false)
            };

            let format_note = f
                .get("format_note")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let protocol = f
                .get("protocol")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let fmt_url = f.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());

            formats.push(MediaFormat {
                format_id,
                ext,
                resolution,
                width,
                height,
                fps,
                vcodec,
                acodec,
                filesize,
                is_filesize_estimated,
                tbr,
                format_note,
                protocol,
                url: fmt_url,
                has_video,
                has_audio,
            });
        }
    }

    Ok(MediaProbeResult {
        title,
        url: fallback_url.to_string(),
        thumbnail,
        duration,
        extractor,
        formats,
    })
}

pub fn parse_ytdlp_progress_line(line: &str) -> Option<MediaProgress> {
    let rest = line.trim().strip_prefix("TFM_PROG|")?;
    let parts: Vec<&str> = rest.split('|').collect();
    if parts.len() < 4 {
        return None;
    }
    let parse_num = |s: &str| -> Option<u64> {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == "NA" || trimmed == "None" {
            return None;
        }
        trimmed
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .map(|v| v as u64)
    };
    let downloaded_bytes = parse_num(parts[0]).unwrap_or(0);
    let total_bytes = parse_num(parts[1]).or_else(|| parse_num(parts[2]));
    let speed_bps = parse_num(parts[3]).unwrap_or(0);
    Some(MediaProgress {
        downloaded_bytes,
        total_bytes,
        speed_bps,
    })
}

/// Merge/remux downloaded video (and optional separate audio) into final container via ffmpeg
pub async fn merge_audio_video_ffmpeg(
    ffmpeg_bin: &Path,
    video_path: &Path,
    audio_path: Option<&Path>,
    output_path: &Path,
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut cmd = Command::new(ffmpeg_bin);
    apply_no_window(&mut cmd);
    cmd.args(["-y", "-i"]).arg(video_path);

    if let Some(aud) = audio_path {
        cmd.arg("-i").arg(aud);
        cmd.args([
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c",
            "copy",
            "-movflags",
            "+faststart",
        ]);
    } else {
        cmd.args(["-c", "copy", "-movflags", "+faststart"]);
    }

    cmd.arg(output_path);

    let out = cmd.output().await?;
    if !out.status.success() {
        return Err(anyhow!(
            "ffmpeg merge failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    Ok(())
}

fn strip_media_extension(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for ext in [".mp4", ".mkv", ".webm", ".mp3", ".m4a", ".mov", ".flv", ".ts"] {
        if lower.ends_with(ext) && name.len() > ext.len() {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

/// Download media cleanly via native parallel HLS/DASH engine + ffmpeg merge, or yt-dlp sidecar.
pub async fn download_media(client: &Client, req: MediaDownloadRequest) -> Result<PathBuf> {
    let cancel = Arc::new(AtomicBool::new(false));
    download_media_with_progress(client, req, cancel, |_| {}).await
}

/// Cancellable, progress-reporting media download pipeline used by DownloadManager & API.
pub async fn download_media_with_progress<F>(
    client: &Client,
    req: MediaDownloadRequest,
    cancel_token: Arc<AtomicBool>,
    mut on_progress: F,
) -> Result<PathBuf>
where
    F: FnMut(MediaProgress) + Send,
{
    std::fs::create_dir_all(&req.save_dir)?;
    let concurrency = (req.concurrency.unwrap_or(6) as usize).clamp(1, 16);
    let ffmpeg_bin = find_binary("ffmpeg", "TURBOGET_FFMPEG_PATH");

    let base_name = req
        .file_name
        .as_deref()
        .map(sanitize_filename)
        .filter(|s| !s.is_empty() && s != "video" && s != "download.bin")
        .unwrap_or_else(|| infer_title_from_url(&req.url));
    let clean_stem = strip_media_extension(&base_name);

    if is_hls_url(&req.url) {
        let master_text = fetch_text(client, &req.url, &req.headers).await?;
        let (video_playlist_url, audio_playlist_url) = match parse_m3u8(&master_text, &req.url)? {
            HlsPlaylist::Master {
                variants,
                audio_tracks,
            } => {
                let selected_idx = req
                    .format_id
                    .as_deref()
                    .and_then(|fid| fid.strip_prefix("hls-"))
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                let variant = variants
                    .get(selected_idx)
                    .or_else(|| variants.first())
                    .ok_or_else(|| anyhow!("No HLS video variant available"))?;
                let audio_uri = variant.audio_group.as_ref().and_then(|grp| {
                    audio_tracks
                        .iter()
                        .find(|a| &a.group_id == grp)
                        .or_else(|| audio_tracks.first())
                        .map(|a| a.uri.clone())
                });
                (variant.uri.clone(), audio_uri)
            }
            HlsPlaylist::Media(_) => (req.url.clone(), None),
        };

        if cancel_token.load(Ordering::Relaxed) {
            return Err(anyhow!("Download canceled/paused"));
        }

        let tmp_dir = TempWorkDir::create(&req.save_dir)?;
        let video_part = tmp_dir.path().join("hls_video.ts");
        download_hls_media_playlist(
            client,
            &video_playlist_url,
            &video_part,
            concurrency,
            &req.headers,
        )
        .await?;

        let vid_len = std::fs::metadata(&video_part).map(|m| m.len()).unwrap_or(0);
        on_progress(MediaProgress {
            downloaded_bytes: vid_len,
            total_bytes: Some(vid_len),
            speed_bps: 0,
        });

        let audio_part = if let Some(aud_url) = audio_playlist_url {
            let p = tmp_dir.path().join("hls_audio.ts");
            download_hls_media_playlist(client, &aud_url, &p, concurrency, &req.headers).await?;
            Some(p)
        } else {
            None
        };

        let ext = if req.audio_only { "m4a" } else { "mp4" };
        let output_path = req.save_dir.join(format!("{}.{}", clean_stem, ext));

        let ff = ffmpeg_bin.ok_or_else(|| anyhow!("ffmpeg required to remux/merge HLS stream"))?;
        if req.audio_only {
            let source = audio_part.as_deref().unwrap_or(&video_part);
            let mut cmd = Command::new(&ff);
            apply_no_window(&mut cmd);
            cmd.args(["-y", "-i"]).arg(source);
            cmd.args(["-vn", "-c:a", "copy"]).arg(&output_path);
            let out = cmd.output().await?;
            if !out.status.success() {
                return Err(anyhow!(
                    "ffmpeg audio extract failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        } else {
            merge_audio_video_ffmpeg(&ff, &video_part, audio_part.as_deref(), &output_path).await?;
        }
        return Ok(output_path);
    }

    if is_dash_url(&req.url) {
        let xml = fetch_text(client, &req.url, &req.headers).await?;
        let manifest = parse_mpd(&xml, &req.url)?;

        let mut videos: Vec<_> = manifest
            .representations
            .iter()
            .filter(|r| r.content_type == "video")
            .cloned()
            .collect();
        videos.sort_by_key(|b| std::cmp::Reverse(b.bandwidth));

        let mut audios: Vec<_> = manifest
            .representations
            .iter()
            .filter(|r| r.content_type == "audio")
            .cloned()
            .collect();
        audios.sort_by_key(|b| std::cmp::Reverse(b.bandwidth));

        let tmp_dir = TempWorkDir::create(&req.save_dir)?;

        if req.audio_only {
            let best_audio = audios
                .first()
                .ok_or_else(|| anyhow!("No DASH audio representation available"))?;
            let aud_raw = tmp_dir.path().join("dash_audio.m4s");
            download_dash_representation(client, best_audio, &aud_raw, concurrency, &req.headers)
                .await?;
            let out_path = req.save_dir.join(format!("{}.m4a", clean_stem));
            if let Some(ff) = ffmpeg_bin {
                merge_audio_video_ffmpeg(&ff, &aud_raw, None, &out_path).await?;
            } else {
                std::fs::copy(&aud_raw, &out_path)?;
            }
            return Ok(out_path);
        }

        let selected_video = req
            .format_id
            .as_deref()
            .and_then(|fid| videos.iter().find(|v| v.id == fid))
            .or_else(|| videos.first())
            .ok_or_else(|| anyhow!("No DASH video representation available"))?;

        let vid_raw = tmp_dir.path().join("dash_video.mp4");
        download_dash_representation(client, selected_video, &vid_raw, concurrency, &req.headers)
            .await?;

        let vid_len = std::fs::metadata(&vid_raw).map(|m| m.len()).unwrap_or(0);
        on_progress(MediaProgress {
            downloaded_bytes: vid_len,
            total_bytes: None,
            speed_bps: 0,
        });

        let aud_raw = if let Some(best_audio) = audios.first() {
            let p = tmp_dir.path().join("dash_audio.m4a");
            download_dash_representation(client, best_audio, &p, concurrency, &req.headers).await?;
            Some(p)
        } else {
            None
        };

        let output_path = req.save_dir.join(format!("{}.mp4", clean_stem));

        let ff = ffmpeg_bin.ok_or_else(|| anyhow!("ffmpeg required to merge DASH video + audio"))?;
        merge_audio_video_ffmpeg(&ff, &vid_raw, aud_raw.as_deref(), &output_path).await?;
        return Ok(output_path);
    }

    // Standard page URL via yt-dlp sidecar (auto-installed if missing)
    let ytdlp_bin = ensure_ytdlp_binary(client).await?;

    let tmpl_stem = if req.file_name.is_some() && !clean_stem.is_empty() && clean_stem != "video" {
        clean_stem.clone()
    } else {
        "%(title).180B".to_string()
    };
    let out_tmpl = req.save_dir.join(format!("{}.%(ext)s", tmpl_stem));

    let mut cmd = build_ytdlp_base_cmd(&ytdlp_bin, &req.url, &req.headers);
    cmd.kill_on_drop(true);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.args([
        "--newline",
        "--progress",
        "--progress-template",
        "download:TFM_PROG|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s",
        "-o",
    ])
    .arg(&out_tmpl);

    if let Some(ref ff) = ffmpeg_bin {
        cmd.arg("--ffmpeg-location").arg(ff);
    }

    if req.audio_only {
        cmd.args(["-x", "--audio-format", "mp3"]);
    } else if let Some(ref fid) = req.format_id {
        if !fid.is_empty() && fid != "best" && fid != "direct" {
            let selector = format!("{}/bv*+ba/b", fid);
            cmd.args(["-f", &selector, "--merge-output-format", "mp4"]);
        } else {
            cmd.args(["-f", "bv*+ba/b", "--merge-output-format", "mp4"]);
        }
    } else {
        cmd.args(["-f", "bv*+ba/b", "--merge-output-format", "mp4"]);
    }

    cmd.args(["--print", "after_move:filepath"]).arg(&req.url);

    let mut child = cmd.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed capturing yt-dlp stdout"))?;
    let stderr = child.stderr.take();

    let stderr_handle = tokio::spawn(async move {
        let mut buf = String::new();
        if let Some(err_stream) = stderr {
            let mut r = BufReader::new(err_stream).lines();
            while let Ok(Some(line)) = r.next_line().await {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(&line);
            }
        }
        buf
    });

    let mut reader = BufReader::new(stdout).lines();
    let mut last_printed_path = String::new();

    loop {
        if cancel_token.load(Ordering::Relaxed) {
            let _ = child.kill().await;
            return Err(anyhow!("Download canceled/paused"));
        }

        tokio::select! {
            line_res = reader.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        if let Some(prog) = parse_ytdlp_progress_line(&line) {
                            on_progress(prog);
                        } else {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() && !trimmed.starts_with('[') && !trimmed.starts_with("TFM_PROG|") {
                                last_printed_path = trimmed.to_string();
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = child.kill().await;
                        return Err(e.into());
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                if cancel_token.load(Ordering::Relaxed) {
                    let _ = child.kill().await;
                    return Err(anyhow!("Download canceled/paused"));
                }
            }
        }
    }

    let status = child.wait().await?;
    let err_output = stderr_handle.await.unwrap_or_default();

    if !status.success() {
        return Err(anyhow!(
            "yt-dlp download failed: {}",
            if err_output.trim().is_empty() {
                format!("exit code {:?}", status.code())
            } else {
                err_output
            }
        ));
    }

    if !last_printed_path.is_empty() && Path::new(&last_printed_path).exists() {
        Ok(PathBuf::from(last_printed_path))
    } else {
        let ext = if req.audio_only { "mp3" } else { "mp4" };
        Ok(req.save_dir.join(format!("{}.{}", clean_stem, ext)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ytdlp_json() {
        let sample = serde_json::json!({
            "title": "Sample: Video/Test",
            "duration": 125.5,
            "extractor": "youtube",
            "thumbnail": "https://i.ytimg.com/vi/123/default.jpg",
            "formats": [
                {
                    "format_id": "sb0",
                    "ext": "mhtml",
                    "vcodec": "none",
                    "acodec": "none"
                },
                {
                    "format_id": "137",
                    "ext": "mp4",
                    "width": 1920,
                    "height": 1080,
                    "vcodec": "avc1.640028",
                    "acodec": "none",
                    "filesize": 20000000
                },
                {
                    "format_id": "140",
                    "ext": "m4a",
                    "vcodec": "none",
                    "acodec": "mp4a.40.2",
                    "tbr": 128.0
                }
            ]
        });

        let parsed = parse_ytdlp_json(&sample, "https://example.com/watch?v=1").unwrap();
        assert_eq!(parsed.title, "Sample_ Video_Test");
        assert_eq!(parsed.formats.len(), 2);
        assert!(parsed.formats[0].has_video);
        assert!(!parsed.formats[0].has_audio);
        assert!(!parsed.formats[0].is_filesize_estimated);
        assert!(!parsed.formats[1].has_video);
        assert!(parsed.formats[1].has_audio);
        assert!(parsed.formats[1].is_filesize_estimated);
        assert!(parsed.formats[1].filesize.unwrap() > 1_900_000);
    }

    #[test]
    fn test_direct_and_video_page_detection_and_progress() {
        assert!(is_direct_media_url("https://cdn.example.com/clip.mp4?token=abc"));
        assert!(is_direct_media_url("https://cdn.example.com/song.mp3"));
        assert!(!is_direct_media_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));

        assert!(is_video_page_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(is_video_page_url("https://youtu.be/dQw4w9WgXcQ"));
        assert!(is_video_page_url("https://www.tiktok.com/@user/video/12345"));
        assert!(!is_video_page_url("https://cdn.example.com/clip.mp4"));

        let prog = parse_ytdlp_progress_line("TFM_PROG|1048576|NA|4194304|524288.5").unwrap();
        assert_eq!(prog.downloaded_bytes, 1048576);
        assert_eq!(prog.total_bytes, Some(4194304));
        assert_eq!(prog.speed_bps, 524288);

        let sample_hdrs = vec![
            ("Cookie".to_string(), "SID=abc; HSID=xyz".to_string()),
            ("User-Agent".to_string(), "Mozilla/5.0".to_string()),
            ("Referer".to_string(), "https://www.youtube.com/".to_string()),
        ];
        let yt_filtered = filter_ytdlp_headers("https://www.youtube.com/watch?v=abc", &sample_hdrs);
        assert_eq!(yt_filtered, vec![("Referer", "https://www.youtube.com/")]);
    }
}
