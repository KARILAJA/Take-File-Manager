use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{command, State};

use crate::engine::probe::{probe_url as engine_probe, ProbeInfo};
use crate::manager::state::DownloadStatus;
use crate::manager::{DownloadManager, DownloadRequest};
use crate::storage::db::{
    delete_download as db_delete, CategoryRecord, QueueRecord, ScheduleRecord, SegmentRecord,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddDownloadArgs {
    pub url: String,
    pub save_dir: Option<String>,
    pub file_name: Option<String>,
    pub category_id: Option<String>,
    pub queue_id: Option<String>,
    pub segments_cfg: Option<u32>,
    pub speed_limit: Option<u64>,
    pub headers: Option<Vec<(String, String)>>,
    pub start_now: Option<bool>,
    #[serde(default)]
    pub is_media: Option<bool>,
    #[serde(default)]
    pub format_id: Option<String>,
    #[serde(default)]
    pub audio_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItemDto {
    pub id: String,
    pub url: String,
    pub final_url: Option<String>,
    pub file_name: String,
    pub save_dir: String,
    pub total_bytes: Option<u64>,
    pub downloaded: u64,
    pub status: String,
    pub error: Option<String>,
    pub resumable: bool,
    pub etag: Option<String>,
    pub category: String,
    pub progress_percent: f64,
    pub speed_bps: u64,
    pub eta_seconds: Option<u64>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

pub fn detect_category(file_name: &str) -> &'static str {
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" => "compressed",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "csv" | "epub" => {
            "documents"
        }
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" => "music",
        "exe" | "msi" | "bat" | "cmd" | "sh" | "apk" | "appimage" | "dmg" => "programs",
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" => "video",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "ico" => "images",
        "torrent" => "torrent",
        _ => "others",
    }
}

pub fn default_download_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads")
    })
}

#[command]
pub fn get_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[command]
pub async fn probe_url(url: String) -> Result<ProbeInfo, String> {
    if crate::engine::ftp::is_ftp_url(&url) {
        return crate::engine::ftp::probe_ftp(&url)
            .await
            .map_err(|e| e.to_string());
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;

    if crate::engine::torrent::is_torrent_url(&url) {
        return crate::engine::torrent::probe_torrent(&client, &url)
            .await
            .map_err(|e| e.to_string());
    }

    engine_probe(&client, &url, &[])
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn add_download(
    manager: State<'_, DownloadManager>,
    args: AddDownloadArgs,
) -> Result<String, String> {
    let save_dir = args
        .save_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_download_dir);

    if args.is_media.unwrap_or(false)
        || args.audio_only.unwrap_or(false)
        || args.format_id.as_deref().is_some_and(|s| !s.is_empty())
    {
        return manager
            .add_media_download(crate::manager::MediaDownloadManagerReq {
                url: args.url,
                save_dir,
                file_name: args.file_name,
                format_id: args.format_id,
                audio_only: args.audio_only.unwrap_or(false),
                queue_id: args.queue_id,
                concurrency: args.segments_cfg.map(|v| v.clamp(1, 16) as u8),
                headers: args.headers.unwrap_or_default(),
                start_now: args.start_now.unwrap_or(true),
            })
            .await
            .map_err(|e| e.to_string());
    }

    let req = DownloadRequest {
        url: args.url,
        save_dir,
        file_name: args.file_name,
        category_id: args.category_id,
        queue_id: args.queue_id,
        segments_cfg: args.segments_cfg,
        speed_limit: args.speed_limit,
        headers: args.headers.unwrap_or_default(),
        start_now: args.start_now.unwrap_or(true),
    };

    manager.add_download(req).await.map_err(|e| e.to_string())
}

#[command]
pub async fn pause_download(manager: State<'_, DownloadManager>, id: String) -> Result<(), String> {
    manager.pause(&id).await.map_err(|e| e.to_string())
}

#[command]
pub async fn resume_download(
    manager: State<'_, DownloadManager>,
    id: String,
) -> Result<(), String> {
    manager.start(&id).await.map_err(|e| e.to_string())
}

#[command]
pub async fn cancel_download(
    manager: State<'_, DownloadManager>,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    manager
        .cancel(&id, delete_file)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn delete_download(
    manager: State<'_, DownloadManager>,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    // If running, cancel first
    let _ = manager.cancel(&id, delete_file).await;

    // Remove from DB
    {
        let conn = manager.get_db_conn().await;
        db_delete(&conn, &id).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn record_to_dto(manager: &DownloadManager, r: crate::storage::db::DownloadRecord) -> DownloadItemDto {
    let cat = if crate::engine::torrent::is_torrent_url(&r.url)
        || r.category_id.as_deref() == Some("torrent")
    {
        "torrent".to_string()
    } else {
        detect_category(&r.file_name).to_string()
    };
    let live = if r.status == DownloadStatus::Downloading {
        manager.get_live_stats(&r.id)
    } else {
        None
    };

    let downloaded = live
        .as_ref()
        .map(|l| l.downloaded_bytes.max(r.downloaded))
        .unwrap_or(r.downloaded);
    let total_bytes = live.as_ref().and_then(|l| l.total_bytes).or(r.total_bytes);
    let speed_bps = live.as_ref().map(|l| l.speed_bps).unwrap_or(0);

    let progress_percent = if let Some(total) = total_bytes {
        if total > 0 {
            (downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        }
    } else {
        0.0
    };

    let eta_seconds = if r.status == DownloadStatus::Downloading && speed_bps > 0 {
        total_bytes.map(|total| {
            if total > downloaded {
                (total - downloaded).div_ceil(speed_bps)
            } else {
                0
            }
        })
    } else {
        None
    };

    DownloadItemDto {
        id: r.id,
        url: r.url,
        final_url: r.final_url,
        file_name: r.file_name,
        save_dir: r.save_dir,
        total_bytes,
        downloaded,
        status: r.status.to_string(),
        error: r.error,
        resumable: r.resumable,
        etag: r.etag,
        category: cat,
        progress_percent,
        speed_bps,
        eta_seconds,
        created_at: r.created_at,
        completed_at: r.completed_at,
    }
}

#[command]
pub async fn list_downloads(
    manager: State<'_, DownloadManager>,
    status: Option<String>,
    search: Option<String>,
    category: Option<String>,
) -> Result<Vec<DownloadItemDto>, String> {
    let filter_status = status.and_then(|s| s.parse::<DownloadStatus>().ok());
    let records = manager
        .list_downloads(filter_status)
        .await
        .map_err(|e| e.to_string())?;

    let mut dtos = Vec::with_capacity(records.len());
    let search_lower = search.as_deref().map(|s| s.to_lowercase());
    let category_lower = category.as_deref().map(|c| c.to_lowercase());

    for r in records {
        let cat = if crate::engine::torrent::is_torrent_url(&r.url)
            || r.category_id.as_deref() == Some("torrent")
        {
            "torrent"
        } else {
            detect_category(&r.file_name)
        };

        if let Some(ref cat_filter) = category_lower {
            if cat_filter != "all" && cat != cat_filter {
                continue;
            }
        }

        if let Some(ref q) = search_lower {
            let match_name = r.file_name.to_lowercase().contains(q);
            let match_url = r.url.to_lowercase().contains(q);
            if !match_name && !match_url {
                continue;
            }
        }

        dtos.push(record_to_dto(&manager, r));
    }

    Ok(dtos)
}

#[command]
pub async fn get_download(
    manager: State<'_, DownloadManager>,
    id: String,
) -> Result<Option<DownloadItemDto>, String> {
    let record = manager
        .get_download(&id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(record.map(|r| record_to_dto(&manager, r)))
}

#[command]
pub async fn get_segments(
    manager: State<'_, DownloadManager>,
    id: String,
) -> Result<Vec<SegmentRecord>, String> {
    manager.get_segments(&id).await.map_err(|e| e.to_string())
}

#[cfg(windows)]
fn normalize_win_path(raw: &str) -> PathBuf {
    let cleaned = raw.trim().trim_start_matches(r"\\?\").replace('/', "\\");
    let p = PathBuf::from(&cleaned);
    if let Ok(canon) = std::fs::canonicalize(&p) {
        let s = canon.to_string_lossy().to_string();
        return PathBuf::from(s.trim_start_matches(r"\\?\"));
    }
    p
}

#[command]
pub fn open_file(path: String) -> Result<(), String> {
    #[cfg(windows)]
    let p = normalize_win_path(&path);
    #[cfg(not(windows))]
    let p = PathBuf::from(&path);

    if !p.exists() {
        return Err(format!("File does not exist: {}", p.display()));
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .raw_arg(format!("/c start \"\" \"{}\"", p.display()))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        open::that(p).map_err(|e| e.to_string())
    }
}

#[command]
pub fn open_folder(path: String) -> Result<(), String> {
    #[cfg(windows)]
    let p = normalize_win_path(&path);
    #[cfg(not(windows))]
    let p = PathBuf::from(&path);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if p.exists() && p.is_file() {
            std::process::Command::new("explorer.exe")
                .raw_arg(format!("/select,\"{}\"", p.display()))
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        let folder = if p.is_dir() {
            Some(p.clone())
        } else {
            p.parent().filter(|parent| parent.exists()).map(Path::to_path_buf)
        };
        if let Some(dir) = folder {
            std::process::Command::new("explorer.exe")
                .arg(&dir)
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        Err(format!("Path does not exist: {}", p.display()))
    }
    #[cfg(not(windows))]
    {
        if !p.exists() {
            return Err(format!("Path does not exist: {}", p.display()));
        }
        open::that(p).map_err(|e| e.to_string())
    }
}

#[command]
pub async fn set_speed_limit(
    manager: State<'_, DownloadManager>,
    limit_bps: u64,
) -> Result<(), String> {
    manager.set_global_speed_limit(limit_bps).await;
    Ok(())
}

#[command]
pub async fn queue_list(manager: State<'_, DownloadManager>) -> Result<Vec<QueueRecord>, String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::list_queues(&conn).map_err(|e| e.to_string())
}

#[command]
pub async fn queue_create(
    manager: State<'_, DownloadManager>,
    queue: QueueRecord,
) -> Result<(), String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::insert_queue(&conn, &queue).map_err(|e| e.to_string())
}

#[command]
pub async fn queue_update(
    manager: State<'_, DownloadManager>,
    queue: QueueRecord,
) -> Result<(), String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::update_queue(&conn, &queue).map_err(|e| e.to_string())
}

#[command]
pub async fn queue_delete(manager: State<'_, DownloadManager>, id: String) -> Result<(), String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::delete_queue(&conn, &id).map_err(|e| e.to_string())
}

#[command]
pub async fn queue_start(manager: State<'_, DownloadManager>, id: String) -> Result<(), String> {
    manager.start_queue(&id).await.map_err(|e| e.to_string())
}

#[command]
pub async fn queue_pause(manager: State<'_, DownloadManager>, id: String) -> Result<(), String> {
    manager.pause_queue(&id).await.map_err(|e| e.to_string())
}

#[command]
pub async fn schedule_list(
    manager: State<'_, DownloadManager>,
) -> Result<Vec<ScheduleRecord>, String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::list_schedules(&conn).map_err(|e| e.to_string())
}

#[command]
pub async fn schedule_create(
    manager: State<'_, DownloadManager>,
    schedule: ScheduleRecord,
) -> Result<(), String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::insert_schedule(&conn, &schedule).map_err(|e| e.to_string())
}

#[command]
pub async fn schedule_update(
    manager: State<'_, DownloadManager>,
    schedule: ScheduleRecord,
) -> Result<(), String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::update_schedule(&conn, &schedule).map_err(|e| e.to_string())
}

#[command]
pub async fn schedule_delete(
    manager: State<'_, DownloadManager>,
    id: String,
) -> Result<(), String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::delete_schedule(&conn, &id).map_err(|e| e.to_string())
}

#[command]
pub async fn category_list(
    manager: State<'_, DownloadManager>,
) -> Result<Vec<CategoryRecord>, String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::list_categories(&conn).map_err(|e| e.to_string())
}

#[command]
pub async fn category_update(
    manager: State<'_, DownloadManager>,
    category: CategoryRecord,
) -> Result<(), String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::update_category(&conn, &category).map_err(|e| e.to_string())
}

#[command]
pub async fn set_download_priority(
    manager: State<'_, DownloadManager>,
    id: String,
    priority: i32,
) -> Result<(), String> {
    let conn = manager.get_db_conn().await;
    crate::manager::queue::QueueManager::set_priority(&conn, &id, priority)
        .map_err(|e| e.to_string())
}

#[command]
pub fn execute_power_action(action: String, command: Option<String>) -> Result<(), String> {
    crate::system::power::execute_power_action(&action, command.as_deref()).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDownloadArgs {
    pub url: String,
    pub format_id: Option<String>,
    pub audio_only: Option<bool>,
    pub save_dir: Option<String>,
    pub file_name: Option<String>,
    pub queue_id: Option<String>,
    pub headers: Option<Vec<(String, String)>>,
    pub concurrency: Option<u8>,
}

#[command]
pub fn media_tools_status() -> crate::media::ytdlp::MediaToolsStatus {
    crate::media::ytdlp::get_media_tools_status()
}

#[command]
pub async fn media_probe(
    url: String,
    headers: Option<Vec<(String, String)>>,
) -> Result<crate::media::ytdlp::MediaProbeResult, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;
    crate::media::ytdlp::probe_media(&client, &url, &headers.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn media_download(
    manager: State<'_, DownloadManager>,
    args: MediaDownloadArgs,
) -> Result<String, String> {
    let save_dir = args
        .save_dir
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_download_dir);

    manager
        .add_media_download(crate::manager::MediaDownloadManagerReq {
            url: args.url,
            save_dir,
            file_name: args.file_name,
            format_id: args.format_id,
            audio_only: args.audio_only.unwrap_or(false),
            queue_id: args.queue_id,
            concurrency: args.concurrency,
            headers: args.headers.unwrap_or_default(),
            start_now: true,
        })
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// M8: Batch, Pattern Expansion, Import/Export & Site Grabber Commands
// ---------------------------------------------------------------------------

pub struct CrawlCancelState(pub std::sync::Arc<tokio::sync::Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDownloadArgs {
    pub urls_or_text: String,
    pub save_dir: Option<String>,
    pub category_id: Option<String>,
    pub queue_id: Option<String>,
    pub segments_cfg: Option<u32>,
    pub start_now: Option<bool>,
}

#[command]
pub fn expand_url_pattern(pattern: String) -> Result<Vec<String>, String> {
    crate::sitegrab::batch::expand_url_pattern(&pattern).map_err(|e| e.to_string())
}

#[command]
pub async fn add_batch(
    manager: State<'_, DownloadManager>,
    args: BatchDownloadArgs,
) -> Result<Vec<String>, String> {
    let urls = crate::sitegrab::batch::parse_pasted_urls(&args.urls_or_text);
    let save_dir = args
        .save_dir
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_download_dir);

    let mut ids = Vec::with_capacity(urls.len());
    for url in urls {
        let req = DownloadRequest {
            url,
            save_dir: save_dir.clone(),
            file_name: None,
            category_id: args.category_id.clone(),
            queue_id: args.queue_id.clone(),
            segments_cfg: args.segments_cfg,
            speed_limit: None,
            headers: Vec::new(),
            start_now: args.start_now.unwrap_or(false),
        };
        let id = manager.add_download(req).await.map_err(|e| e.to_string())?;
        ids.push(id);
    }
    Ok(ids)
}

#[command]
pub async fn add_from_pattern(
    manager: State<'_, DownloadManager>,
    args: BatchDownloadArgs,
) -> Result<Vec<String>, String> {
    let urls = crate::sitegrab::batch::expand_url_pattern(&args.urls_or_text)
        .map_err(|e| e.to_string())?;
    let save_dir = args
        .save_dir
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_download_dir);

    let mut ids = Vec::with_capacity(urls.len());
    for url in urls {
        let req = DownloadRequest {
            url,
            save_dir: save_dir.clone(),
            file_name: None,
            category_id: args.category_id.clone(),
            queue_id: args.queue_id.clone(),
            segments_cfg: args.segments_cfg,
            speed_limit: None,
            headers: Vec::new(),
            start_now: args.start_now.unwrap_or(false),
        };
        let id = manager.add_download(req).await.map_err(|e| e.to_string())?;
        ids.push(id);
    }
    Ok(ids)
}

#[command]
pub async fn export_list(
    manager: State<'_, DownloadManager>,
    format: String,
    path: Option<String>,
) -> Result<String, String> {
    let records = manager.list_downloads(None).await.map_err(|e| e.to_string())?;
    let exported = crate::sitegrab::batch::export_downloads(&records, &format)
        .map_err(|e| e.to_string())?;
    if let Some(out_path) = path.filter(|s| !s.trim().is_empty()) {
        std::fs::write(&out_path, &exported).map_err(|e| e.to_string())?;
    }
    Ok(exported)
}

#[command]
pub async fn import_list(
    manager: State<'_, DownloadManager>,
    format: String,
    content: String,
    save_dir: Option<String>,
    start_now: Option<bool>,
) -> Result<Vec<String>, String> {
    let entries = crate::sitegrab::batch::import_downloads(&content, &format)
        .map_err(|e| e.to_string())?;
    let fallback_dir = save_dir
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_download_dir);

    let mut ids = Vec::with_capacity(entries.len());
    for entry in entries {
        let dir = entry
            .save_dir
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| fallback_dir.clone());
        let req = DownloadRequest {
            url: entry.url,
            save_dir: dir,
            file_name: entry.file_name,
            category_id: entry.category_id,
            queue_id: entry.queue_id,
            segments_cfg: entry.segments_cfg,
            speed_limit: None,
            headers: Vec::new(),
            start_now: start_now.unwrap_or(false),
        };
        let id = manager.add_download(req).await.map_err(|e| e.to_string())?;
        ids.push(id);
    }
    Ok(ids)
}

#[command]
pub async fn grab_site_start(
    crawl_state: State<'_, CrawlCancelState>,
    config: crate::sitegrab::crawler::CrawlerConfig,
) -> Result<Vec<crate::sitegrab::crawler::CrawledItem>, String> {
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut guard = crawl_state.0.lock().await;
        if let Some(prev) = guard.replace(cancel.clone()) {
            prev.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;

    crate::sitegrab::crawler::crawl_site(&client, config, cancel)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn grab_site_stop(crawl_state: State<'_, CrawlCancelState>) -> Result<(), String> {
    let guard = crawl_state.0.lock().await;
    if let Some(ref cancel) = *guard {
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// M9: Agent Access Policy & Audit Log Commands
// ---------------------------------------------------------------------------

#[command]
pub async fn agent_access_get(
    manager: State<'_, DownloadManager>,
) -> Result<crate::manager::agent_policy::AgentAccessPolicy, String> {
    let conn = manager.get_db_conn().await;
    Ok(crate::manager::agent_policy::load_agent_policy(&conn))
}

#[command]
pub async fn agent_access_set(
    manager: State<'_, DownloadManager>,
    policy: crate::manager::agent_policy::AgentAccessPolicy,
) -> Result<(), String> {
    let conn = manager.get_db_conn().await;
    crate::manager::agent_policy::save_agent_policy(&conn, &policy).map_err(|e| e.to_string())
}

#[command]
pub async fn get_audit_logs(
    manager: State<'_, DownloadManager>,
    limit: Option<usize>,
) -> Result<Vec<crate::storage::db::LogRecord>, String> {
    let conn = manager.get_db_conn().await;
    crate::storage::db::list_logs(&conn, limit.unwrap_or(100)).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// M10: Network Settings & Auto-Update Check Commands
// ---------------------------------------------------------------------------

#[command]
pub async fn settings_get(
    manager: State<'_, DownloadManager>,
) -> Result<crate::engine::network::NetworkSettings, String> {
    let conn = manager.get_db_conn().await;
    Ok(crate::engine::network::load_network_settings(&conn))
}

#[command]
pub async fn settings_set(
    manager: State<'_, DownloadManager>,
    settings: crate::engine::network::NetworkSettings,
) -> Result<(), String> {
    {
        let conn = manager.get_db_conn().await;
        crate::engine::network::save_network_settings(&conn, &settings)
            .map_err(|e| e.to_string())?;
    }
    manager
        .set_global_speed_limit(settings.speed_limit_kbps.saturating_mul(1024))
        .await;
    Ok(())
}

#[command]
pub async fn check_app_update(
    manager: State<'_, DownloadManager>,
    feed_url: Option<String>,
) -> Result<crate::system::updater::UpdateCheckResult, String> {
    let settings = {
        let conn = manager.get_db_conn().await;
        crate::engine::network::load_network_settings(&conn)
    };
    let target_url = feed_url
        .or(settings.update_feed_url.clone())
        .ok_or_else(|| "Update feed URL not configured".to_string())?;
    let client = crate::engine::network::build_http_client(&settings, false, None)
        .map_err(|e| e.to_string())?;
    crate::system::updater::check_for_updates(&client, env!("CARGO_PKG_VERSION"), &target_url)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub fn get_default_download_dir() -> String {
    default_download_dir().to_string_lossy().to_string()
}

#[command]
pub fn pop_pending_download_prompt(
    manager: State<'_, DownloadManager>,
) -> Option<crate::manager::PendingDownloadPrompt> {
    manager.pop_pending_prompt()
}

#[command]
pub async fn pick_torrent_file(window: tauri::Window) -> Option<String> {
    #[cfg(windows)]
    {
        let owner_hwnd = window.hwnd().ok().map(|h| h.0 as isize);
        tokio::task::spawn_blocking(move || {
            crate::system::dialog::pick_file_native(
                owner_hwnd,
                "Pilih file .torrent",
                &[
                    ("Torrent Files (*.torrent)", "*.torrent"),
                    ("All Files (*.*)", "*.*"),
                ],
            )
        })
        .await
        .ok()
        .flatten()
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChecksumResult {
    pub algorithm: String,
    pub hash: String,
    pub matches: Option<bool>,
    pub file_size: u64,
    pub elapsed_ms: u64,
}

#[command]
pub async fn verify_file_checksum(
    path: String,
    algorithm: String,
    expected: Option<String>,
) -> Result<FileChecksumResult, String> {
    tokio::task::spawn_blocking(move || {
        use sha2::{Digest, Sha256, Sha512};
        use std::io::Read;
        #[cfg(windows)]
        let file_path = normalize_win_path(&path);
        #[cfg(not(windows))]
        let file_path = PathBuf::from(&path);

        if !file_path.exists() {
            return Err(format!("File does not exist: {}", file_path.display()));
        }

        let started = std::time::Instant::now();
        let meta = std::fs::metadata(&file_path).map_err(|e| e.to_string())?;
        let mut file = std::fs::File::open(&file_path).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 256 * 1024];

        let algo_norm = algorithm.trim().to_ascii_lowercase();
        let hash = match algo_norm.as_str() {
            "sha512" | "sha-512" => {
                let mut hasher = Sha512::new();
                loop {
                    let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buf[..n]);
                }
                format!("{:x}", hasher.finalize())
            }
            _ => {
                let mut hasher = Sha256::new();
                loop {
                    let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buf[..n]);
                }
                format!("{:x}", hasher.finalize())
            }
        };

        let expected_clean = expected
            .map(|e| e.trim().to_ascii_lowercase())
            .filter(|e| !e.is_empty());
        let matches = expected_clean.map(|exp| exp == hash);

        Ok(FileChecksumResult {
            algorithm: if algo_norm.contains("512") {
                "SHA-512".to_string()
            } else {
                "SHA-256".to_string()
            },
            hash,
            matches,
            file_size: meta.len(),
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[command]
pub async fn pause_all_downloads(manager: State<'_, DownloadManager>) -> Result<usize, String> {
    let records = manager.list_downloads(None).await.map_err(|e| e.to_string())?;
    let mut paused = 0usize;
    for r in records {
        if matches!(
            r.status,
            DownloadStatus::Downloading | DownloadStatus::Probing | DownloadStatus::Queued
        ) && manager.pause(&r.id).await.is_ok()
        {
            paused += 1;
        }
    }
    Ok(paused)
}

#[command]
pub fn window_minimize(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[command]
pub fn window_toggle_maximize(window: tauri::Window) -> Result<bool, String> {
    let is_max = window.is_maximized().unwrap_or(false);
    if is_max {
        window.unmaximize().map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        window.maximize().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[command]
pub fn window_close_to_tray(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[command]
pub async fn pick_folder(window: tauri::Window, initial_dir: Option<String>) -> Option<String> {
    #[cfg(windows)]
    {
        let owner_hwnd = window.hwnd().ok().map(|h| h.0 as isize);
        tokio::task::spawn_blocking(move || {
            crate::system::dialog::pick_folder_native(
                owner_hwnd,
                initial_dir.as_deref(),
                "Pilih folder tujuan simpan unduhan",
            )
        })
        .await
        .ok()
        .flatten()
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        initial_dir
    }
}

#[command]
pub async fn autostart_get(manager: State<'_, DownloadManager>) -> Result<bool, String> {
    let conn = manager.get_db_conn().await;
    let pref = crate::storage::db::get_setting(&conn, "autostart_enabled")
        .ok()
        .flatten();
    Ok(match pref.as_deref() {
        Some("true") => crate::system::autostart::is_autostart_enabled(),
        Some("false") => false,
        _ => crate::system::autostart::is_autostart_enabled(),
    })
}

#[command]
pub async fn autostart_set(
    manager: State<'_, DownloadManager>,
    enabled: bool,
) -> Result<bool, String> {
    crate::system::autostart::set_autostart(enabled).map_err(|e| e.to_string())?;
    let conn = manager.get_db_conn().await;
    let _ = crate::storage::db::set_setting(&conn, "autostart_configured", "1");
    let _ = crate::storage::db::set_setting(
        &conn,
        "autostart_enabled",
        if enabled { "true" } else { "false" },
    );
    Ok(crate::system::autostart::is_autostart_enabled())
}

#[command]
pub fn browsers_detect() -> Vec<crate::system::browsers::BrowserIntegrationInfo> {
    crate::system::browsers::detect_browsers()
}

#[command]
pub fn browser_integrate(
    browser_id: String,
    relaunch: Option<bool>,
) -> Result<crate::system::browsers::BrowserIntegrateResult, String> {
    let app_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("turboget");
    std::fs::create_dir_all(&app_dir).ok();
    crate::system::browsers::integrate_browser(&app_dir, &browser_id, relaunch.unwrap_or(false))
}

#[command]
pub fn get_extension_dir() -> Option<String> {
    crate::system::native_host::resolve_extension_dir()
        .map(|p| p.to_string_lossy().to_string())
}


