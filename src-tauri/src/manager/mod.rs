pub mod agent_policy;
pub mod category;
pub mod queue;
pub mod scheduler;
pub mod state;

use anyhow::{anyhow, Result};
use chrono::Datelike;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::engine::limiter::RateLimiter;
use crate::engine::probe::probe_url;
use crate::engine::worker::MultiSegmentDownloader;
use crate::storage::db::{
    checkpoint_download, get_download, get_queue, get_segments, init_db, init_in_memory,
    insert_download, list_categories, list_downloads, list_schedules,
    seed_default_categories_if_empty, seed_default_queue_if_empty, update_download_meta,
    update_download_status, update_download_url, DownloadMetadataUpdate, DownloadRecord,
    SegmentRecord,
};
use state::DownloadStatus;

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub save_dir: PathBuf,
    pub file_name: Option<String>,
    pub category_id: Option<String>,
    pub queue_id: Option<String>,
    pub segments_cfg: Option<u32>,
    pub speed_limit: Option<u64>,
    pub headers: Vec<(String, String)>,
    pub start_now: bool,
}

#[derive(Debug, Clone)]
pub struct MediaOpts {
    pub format_id: Option<String>,
    pub audio_only: bool,
}

#[derive(Debug, Clone)]
pub struct MediaDownloadManagerReq {
    pub url: String,
    pub save_dir: PathBuf,
    pub file_name: Option<String>,
    pub format_id: Option<String>,
    pub audio_only: bool,
    pub queue_id: Option<String>,
    pub concurrency: Option<u8>,
    pub headers: Vec<(String, String)>,
    pub start_now: bool,
}

fn pack_media_headers(
    headers: &[(String, String)],
    format_id: Option<&str>,
    audio_only: bool,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = headers
        .iter()
        .filter(|(k, _)| !k.to_ascii_lowercase().starts_with("x-tfm-internal-"))
        .cloned()
        .collect();
    out.push(("x-tfm-internal-media".to_string(), "1".to_string()));
    if let Some(fid) = format_id.filter(|s| !s.trim().is_empty()) {
        out.push(("x-tfm-internal-format-id".to_string(), fid.to_string()));
    }
    if audio_only {
        out.push(("x-tfm-internal-audio-only".to_string(), "1".to_string()));
    }
    out
}

fn split_internal_media_opts(headers: &[(String, String)]) -> (Vec<(String, String)>, Option<MediaOpts>) {
    let mut clean = Vec::with_capacity(headers.len());
    let mut is_media = false;
    let mut format_id = None;
    let mut audio_only = false;

    for (k, v) in headers {
        let low = k.to_ascii_lowercase();
        if low == "x-tfm-internal-media" {
            is_media = true;
        } else if low == "x-tfm-internal-format-id" {
            is_media = true;
            format_id = Some(v.clone());
        } else if low == "x-tfm-internal-audio-only" {
            is_media = true;
            audio_only = v == "1" || v.eq_ignore_ascii_case("true");
        } else {
            clean.push((k.clone(), v.clone()));
        }
    }

    let opts = if is_media {
        Some(MediaOpts {
            format_id,
            audio_only,
        })
    } else {
        None
    };
    (clean, opts)
}

fn compute_file_sha256_and_size(path: &Path) -> Result<(u64, String)> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let meta = std::fs::metadata(path)?;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok((meta.len(), format!("{:x}", hasher.finalize())))
}

pub type UiWaker = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, Clone)]
pub struct CompletionEvent {
    pub success: bool,
    pub file_name: String,
    pub final_path: PathBuf,
    pub total_bytes: u64,
    pub error: Option<String>,
}

pub type CompletionNotifier = Arc<dyn Fn(CompletionEvent) + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveProgressStats {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDownloadPrompt {
    pub prompt_id: String,
    pub url: String,
    pub file_name: Option<String>,
    pub save_dir: String,
    pub headers: Vec<(String, String)>,
    pub segments_cfg: Option<u32>,
    pub speed_limit: Option<u64>,
    #[serde(default)]
    pub is_media: bool,
    #[serde(default)]
    pub format_id: Option<String>,
    #[serde(default)]
    pub audio_only: bool,
}

pub struct ActiveTask {
    pub cancel_token: Arc<AtomicBool>,
    pub handle: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
pub struct DownloadManager {
    db_conn: Arc<Mutex<rusqlite::Connection>>,
    pub active_tasks: Arc<Mutex<HashMap<String, ActiveTask>>>,
    client: Client,
    pub global_limiter: Arc<RateLimiter>,
    live_stats: Arc<std::sync::Mutex<HashMap<String, LiveProgressStats>>>,
    pending_prompts: Arc<std::sync::Mutex<Vec<PendingDownloadPrompt>>>,
    recent_prompt_urls: Arc<std::sync::Mutex<HashMap<String, std::time::Instant>>>,
    ui_waker: Arc<std::sync::Mutex<Option<UiWaker>>>,
    completion_notifier: Arc<std::sync::Mutex<Option<CompletionNotifier>>>,
    pub torrent_engine: crate::engine::torrent::TorrentEngine,
}

impl DownloadManager {
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let conn = init_db(db_path)?;
        seed_default_queue_if_empty(&conn)?;
        let default_dir = dirs::download_dir()
            .unwrap_or_else(|| PathBuf::from("downloads"))
            .to_string_lossy()
            .to_string();
        seed_default_categories_if_empty(&conn, &default_dir)?;

        Ok(Self {
            db_conn: Arc::new(Mutex::new(conn)),
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()?,
            global_limiter: Arc::new(RateLimiter::unlimited()),
            live_stats: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_prompts: Arc::new(std::sync::Mutex::new(Vec::new())),
            recent_prompt_urls: Arc::new(std::sync::Mutex::new(HashMap::new())),
            ui_waker: Arc::new(std::sync::Mutex::new(None)),
            completion_notifier: Arc::new(std::sync::Mutex::new(None)),
            torrent_engine: crate::engine::torrent::TorrentEngine::new(),
        })
    }

    pub fn new_in_memory() -> Result<Self> {
        let conn = init_in_memory()?;
        seed_default_queue_if_empty(&conn)?;
        seed_default_categories_if_empty(&conn, "downloads")?;

        Ok(Self {
            db_conn: Arc::new(Mutex::new(conn)),
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()?,
            global_limiter: Arc::new(RateLimiter::unlimited()),
            live_stats: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_prompts: Arc::new(std::sync::Mutex::new(Vec::new())),
            recent_prompt_urls: Arc::new(std::sync::Mutex::new(HashMap::new())),
            ui_waker: Arc::new(std::sync::Mutex::new(None)),
            completion_notifier: Arc::new(std::sync::Mutex::new(None)),
            torrent_engine: crate::engine::torrent::TorrentEngine::new(),
        })
    }

    /// Set global speed limit (in bytes/sec, 0 for unlimited)
    pub async fn set_global_speed_limit(&self, limit_bps: u64) {
        self.global_limiter.set_limit(limit_bps).await;
    }

    pub async fn get_global_speed_limit(&self) -> u64 {
        self.global_limiter.limit().await
    }

    /// Add a download and optionally start it immediately
    pub async fn add_download(&self, req: DownloadRequest) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let fallback_name = req.file_name.clone().unwrap_or_else(|| {
            if crate::engine::torrent::is_torrent_url(&req.url) {
                crate::engine::torrent::parse_magnet_display_name(&req.url)
                    .or_else(|| {
                        Path::new(req.url.split('?').next().unwrap_or(&req.url))
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "torrent_download".to_string())
            } else {
                "download.bin".to_string()
            }
        });
        let now = chrono::Utc::now().timestamp_millis();

        // Auto-detect category and resolve save directory
        let categories = {
            let conn = self.db_conn.lock().await;
            list_categories(&conn).unwrap_or_default()
        };

        let (final_save_dir, detected_cat) = category::resolve_save_dir(
            Some(&req.save_dir.to_string_lossy()),
            &fallback_name,
            &categories,
            &req.save_dir.to_string_lossy(),
        );

        let is_torrent_req = crate::engine::torrent::is_torrent_url(&req.url);
        let is_media_req = split_internal_media_opts(&req.headers).1.is_some()
            || crate::media::ytdlp::is_media_download_url(&req.url);
        let category_id = req.category_id.or_else(|| {
            if is_torrent_req {
                Some("torrent".to_string())
            } else if is_media_req {
                Some("video".to_string())
            } else {
                Some(detected_cat)
            }
        });
        let queue_id = req.queue_id.or_else(|| Some("default".to_string()));
        let headers_json = if req.headers.is_empty() {
            None
        } else {
            serde_json::to_string(&req.headers).ok()
        };

        let record = DownloadRecord {
            id: id.clone(),
            url: req.url.clone(),
            final_url: None,
            file_name: fallback_name,
            save_dir: final_save_dir,
            total_bytes: None,
            downloaded: 0,
            status: DownloadStatus::Queued,
            error: None,
            resumable: true,
            etag: None,
            last_modified: None,
            category_id,
            queue_id: queue_id.clone(),
            priority: 0,
            segments_cfg: req.segments_cfg.unwrap_or(8),
            speed_limit: req.speed_limit,
            headers_json,
            checksum_kind: None,
            checksum_value: None,
            created_at: now,
            started_at: None,
            completed_at: None,
        };

        {
            let conn = self.db_conn.lock().await;
            insert_download(&conn, &record)?;
        }

        if req.start_now {
            if let Some(ref q_id) = queue_id {
                let _ = self.start_queue(q_id).await;
            } else {
                self.start(&id).await?;
            }
        }

        Ok(id)
    }

    /// Add a video/media grabber download (HLS/DASH/yt-dlp) to DownloadManager and start it asynchronously.
    pub async fn add_media_download(&self, req: MediaDownloadManagerReq) -> Result<String> {
        let headers = pack_media_headers(
            &req.headers,
            req.format_id.as_deref(),
            req.audio_only,
        );
        let ext = if req.audio_only { "mp3" } else { "mp4" };
        let display_name = req
            .file_name
            .map(|n| {
                let trimmed = n.trim().to_string();
                if trimmed.is_empty() {
                    format!("video.{}", ext)
                } else if trimmed.to_ascii_lowercase().ends_with(&format!(".{}", ext)) {
                    trimmed
                } else {
                    format!("{}.{}", crate::engine::filename::sanitize_filename(&trimmed), ext)
                }
            })
            .unwrap_or_else(|| format!("video.{}", ext));

        let category_id = Some(if req.audio_only {
            "music".to_string()
        } else {
            "video".to_string()
        });

        self.add_download(DownloadRequest {
            url: req.url,
            save_dir: req.save_dir,
            file_name: Some(display_name),
            category_id,
            queue_id: req.queue_id,
            segments_cfg: Some(req.concurrency.unwrap_or(6) as u32),
            speed_limit: None,
            headers,
            start_now: req.start_now,
        })
        .await
    }

    /// Start a specific queue, launching pending downloads up to max_concurrent
    pub async fn start_queue(&self, queue_id: &str) -> Result<()> {
        let (max_concurrent, enabled) = {
            let conn = self.db_conn.lock().await;
            if let Some(q) = get_queue(&conn, queue_id)? {
                (q.max_concurrent as usize, q.enabled)
            } else {
                (3, true)
            }
        };

        if !enabled {
            return Ok(());
        }

        let (downloads, active_ids) = {
            let conn = self.db_conn.lock().await;
            let dls = list_downloads(&conn, None)?;
            let tasks = self.active_tasks.lock().await;
            let ids: Vec<String> = tasks.keys().cloned().collect();
            (dls, ids)
        };

        let active_count = queue::QueueManager::count_active_in_queue(queue_id, &active_ids, &downloads);
        let slots = max_concurrent.saturating_sub(active_count);
        let to_start = queue::QueueManager::select_next_to_start(queue_id, &downloads, slots);

        for dl_id in to_start {
            let mgr = self.clone();
            tokio::spawn(async move {
                let _ = mgr.start(&dl_id).await;
            });
        }

        Ok(())
    }

    /// Pause all downloads active in a given queue
    pub async fn pause_queue(&self, queue_id: &str) -> Result<()> {
        let downloads = {
            let conn = self.db_conn.lock().await;
            list_downloads(&conn, None)?
        };

        for d in downloads {
            let belongs = match d.queue_id.as_deref() {
                Some(q) => q == queue_id,
                None => queue_id == "default",
            };
            if belongs && (d.status == DownloadStatus::Downloading || d.status == DownloadStatus::Probing) {
                let _ = self.pause(&d.id).await;
            }
        }
        Ok(())
    }

    /// Start or resume an individual download
    pub fn start(&self, id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'static>> {
        let mgr = self.clone();
        let id_str = id.to_string();
        Box::pin(async move {
            mgr.start_impl(&id_str).await
        })
    }

    async fn start_impl(&self, id: &str) -> Result<()> {
        let record = {
            let conn = self.db_conn.lock().await;
            get_download(&conn, id)?.ok_or_else(|| anyhow!("Download {} not found", id))?
        };

        if !record.status.can_transition_to(DownloadStatus::Probing) {
            return Err(anyhow!(
                "Cannot transition download {} from {} to probing",
                id,
                record.status
            ));
        }

        // Set status to probing
        {
            let conn = self.db_conn.lock().await;
            update_download_status(&conn, id, DownloadStatus::Probing, None)?;
        }

        let cancel_token = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel_token.clone();
        let mgr = self.clone();
        let download_id = id.to_string();

        let handle = tokio::spawn(async move {
            if let Err(e) = mgr.run_download(&download_id, cancel_clone).await {
                let conn = mgr.db_conn.lock().await;
                let _ = update_download_status(
                    &conn,
                    &download_id,
                    DownloadStatus::Failed,
                    Some(&e.to_string()),
                );
            }
        });

        {
            let mut tasks = self.active_tasks.lock().await;
            tasks.insert(
                id.to_string(),
                ActiveTask {
                    cancel_token,
                    handle,
                },
            );
        }

        Ok(())
    }

    async fn run_download(&self, id: &str, cancel_token: Arc<AtomicBool>) -> Result<()> {
        let (record, saved_segments) = {
            let conn = self.db_conn.lock().await;
            let rec = get_download(&conn, id)?.ok_or_else(|| anyhow!("Download {} not found", id))?;
            let segs = get_segments(&conn, id)?;
            (rec, segs)
        };

        let raw_headers: Vec<(String, String)> = record
            .headers_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let (custom_headers, media_opts) = split_internal_media_opts(&raw_headers);

        let net_settings = {
            let conn = self.db_conn.lock().await;
            crate::engine::network::load_network_settings(&conn)
        };
        let http_client = crate::engine::network::build_http_client(&net_settings, false, None)
            .unwrap_or_else(|_| self.client.clone());

        // 1. Probe URL (Media pipeline, BitTorrent magnet/.torrent, FTP/FTPS, or HTTP/HTTPS)
        let is_torrent = crate::engine::torrent::is_torrent_url(&record.url);
        let is_ftp = crate::engine::ftp::is_ftp_url(&record.url);
        let is_media_pipeline =
            media_opts.is_some() || crate::media::ytdlp::is_media_download_url(&record.url);

        let probe_res = if is_media_pipeline {
            Ok(crate::engine::probe::ProbeInfo {
                url: record.url.clone(),
                final_url: record.url.clone(),
                file_name: record.file_name.clone(),
                total_bytes: record.total_bytes,
                resumable: false,
                etag: None,
                last_modified: None,
                content_type: None,
            })
        } else if is_torrent {
            crate::engine::torrent::probe_torrent(&http_client, &record.url).await
        } else if is_ftp {
            crate::engine::ftp::probe_ftp(&record.url).await
        } else {
            probe_url(&http_client, &record.url, &custom_headers).await
        };

        let probe = match probe_res {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string();
                let status = if err_str.contains("403") || err_str.contains("410") {
                    DownloadStatus::WaitingRefresh
                } else {
                    DownloadStatus::Failed
                };
                let conn = self.db_conn.lock().await;
                update_download_status(&conn, id, status, Some(&err_str))?;
                return Err(e);
            }
        };

        // Update metadata in DB
        {
            let conn = self.db_conn.lock().await;
            update_download_meta(
                &conn,
                id,
                &DownloadMetadataUpdate {
                    final_url: &probe.final_url,
                    file_name: &probe.file_name,
                    total_bytes: probe.total_bytes,
                    resumable: probe.resumable,
                    etag: probe.etag.as_deref(),
                    last_modified: probe.last_modified.as_deref(),
                },
            )?;
            update_download_status(&conn, id, DownloadStatus::Downloading, None)?;
        }

        // 2. Setup Downloader (MultiSegment HTTP vs FTP) with rate limiter
        let save_dir = PathBuf::from(&record.save_dir);
        let global_lim = self.global_limiter.limit().await;
        let effective_limit = match (record.speed_limit, global_lim) {
            (Some(dl), 0) => Some(dl),
            (None, gl) if gl > 0 => Some(gl),
            (Some(dl), gl) if gl > 0 => Some(dl.min(gl)),
            _ => None,
        };
        let limiter_arc = effective_limit.map(|lim| Arc::new(RateLimiter::new(lim)));

        // 3. Periodic Checkpoint Loop
        let db_for_cp = self.db_conn.clone();
        let id_for_cp = id.to_string();
        let cancel_cp_loop = cancel_token.clone();
        let cp_downloaded = Arc::new(std::sync::atomic::AtomicU64::new(record.downloaded));
        let cp_dl_clone = cp_downloaded.clone();
        let cp_segments = Arc::new(std::sync::Mutex::new(Vec::<SegmentRecord>::new()));
        let cp_segs_clone = cp_segments.clone();

        let checkpoint_task = tokio::spawn(async move {
            while !cancel_cp_loop.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let current = cp_dl_clone.load(Ordering::Relaxed);
                let segs = {
                    let guard = cp_segs_clone.lock().unwrap();
                    guard.clone()
                };
                if current > 0 {
                    let mut conn = db_for_cp.lock().await;
                    let _ = checkpoint_download(&mut conn, &id_for_cp, current, &segs);
                }
            }
        });

        // 4. Execute download
        let id_str = id.to_string();
        let mgr_for_torrent = self.clone();
        let id_for_torrent = id.to_string();
        let mgr_for_ftp = self.clone();
        let id_for_ftp = id.to_string();
        let mgr_for_http = self.clone();
        let id_for_http = id.to_string();
        let mut resolved_name_override: Option<String> = None;
        let outcome = if is_media_pipeline {
            let opts = media_opts.unwrap_or(MediaOpts {
                format_id: None,
                audio_only: false,
            });
            let media_req = crate::media::ytdlp::MediaDownloadRequest {
                url: record.url.clone(),
                format_id: opts.format_id,
                audio_only: opts.audio_only,
                save_dir: save_dir.clone(),
                file_name: if record.file_name == "video.mp4" || record.file_name == "video.mp3" || record.file_name == "download.bin" {
                    None
                } else {
                    Some(record.file_name.clone())
                },
                headers: custom_headers.clone(),
                concurrency: Some((record.segments_cfg.clamp(1, 16)) as u8),
            };
            let cancel_media = cancel_token.clone();
            let mgr_for_media = self.clone();
            let id_for_media = id.to_string();
            let cp_dl_media = cp_downloaded.clone();

            match crate::media::ytdlp::download_media_with_progress(
                &http_client,
                media_req,
                cancel_media.clone(),
                move |p| {
                    cp_dl_media.store(p.downloaded_bytes, Ordering::Relaxed);
                    mgr_for_media.record_live_progress(
                        &id_for_media,
                        p.downloaded_bytes,
                        p.total_bytes,
                        p.speed_bps,
                    );
                },
            )
            .await
            {
                Ok(out_path) => {
                    if let Some(fname) = out_path.file_name().and_then(|s| s.to_str()) {
                        resolved_name_override = Some(fname.to_string());
                    }
                    match compute_file_sha256_and_size(&out_path) {
                        Ok((total_bytes, sha256_hex)) => Ok(crate::engine::single::DownloadOutcome::Completed {
                            final_path: out_path,
                            total_bytes,
                            sha256_hex,
                        }),
                        Err(e) => Err(e),
                    }
                }
                Err(e) => {
                    if cancel_media.load(Ordering::Relaxed) {
                        Ok(crate::engine::single::DownloadOutcome::Paused {
                            part_path: save_dir.join(&probe.file_name),
                            downloaded_bytes: cp_downloaded.load(Ordering::Relaxed),
                        })
                    } else {
                        Err(e)
                    }
                }
            }
        } else if is_torrent {
            match self
                .torrent_engine
                .download(
                    id,
                    &http_client,
                    &record.url,
                    &save_dir,
                    &probe.file_name,
                    effective_limit,
                    cancel_token.clone(),
                    move |p| {
                        cp_downloaded.store(p.downloaded_bytes, Ordering::Relaxed);
                        mgr_for_torrent.record_live_progress(
                            &id_for_torrent,
                            p.downloaded_bytes,
                            p.total_bytes,
                            p.speed_bps,
                        );
                    },
                )
                .await
            {
                Ok((out, name_opt)) => {
                    resolved_name_override = name_opt;
                    Ok(out)
                }
                Err(e) => Err(e),
            }
        } else if is_ftp {
            let mut ftp_dl = crate::engine::ftp::FtpDownloader::new(
                record.url.clone(),
                save_dir,
                probe.file_name.clone(),
                probe.total_bytes,
            );
            if let Some(ref lim) = limiter_arc {
                ftp_dl = ftp_dl.with_rate_limiter(lim.clone());
            }
            ftp_dl
                .execute(cancel_token.clone(), move |p| {
                    cp_downloaded.store(p.downloaded_bytes, Ordering::Relaxed);
                    mgr_for_ftp.record_live_progress(
                        &id_for_ftp,
                        p.downloaded_bytes,
                        p.total_bytes,
                        p.speed_bps,
                    );
                })
                .await
        } else {
            let mut downloader = MultiSegmentDownloader::new(
                http_client,
                probe.clone(),
                save_dir,
                record.segments_cfg,
            )
            .with_headers(custom_headers.clone())
            .with_initial_segments(saved_segments);

            if let Some(ref lim) = limiter_arc {
                downloader = downloader.with_rate_limiter(lim.clone());
            }

            downloader
                .execute(cancel_token.clone(), move |p| {
                    cp_downloaded.store(p.downloaded_bytes, Ordering::Relaxed);
                    mgr_for_http.record_live_progress(
                        &id_for_http,
                        p.downloaded_bytes,
                        p.total_bytes,
                        p.speed_bps,
                    );
                    if !p.segments.is_empty() {
                        let mut guard = cp_segments.lock().unwrap();
                        *guard = p
                            .segments
                            .into_iter()
                            .map(|mut s| {
                                s.download_id = id_str.clone();
                                s
                            })
                            .collect();
                    }
                })
                .await
        };

        checkpoint_task.abort();
        if let Ok(mut guard) = self.live_stats.lock() {
            guard.remove(id);
        }

        // Remove active task handle
        {
            let mut tasks = self.active_tasks.lock().await;
            tasks.remove(id);
        }

        match outcome {
            Ok(crate::engine::single::DownloadOutcome::Completed { final_path, total_bytes, sha256_hex }) => {
                // Apply Windows Mark-of-the-Web (Zone.Identifier) for downloaded security hygiene
                if final_path.is_file() {
                    let referrer = custom_headers
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("referer"))
                        .map(|(_, v)| v.as_str());
                    let _ = crate::system::security::apply_mark_of_the_web(
                        &final_path,
                        &probe.final_url,
                        referrer,
                    );
                }

                let final_display_name = resolved_name_override
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(&probe.file_name);

                let mut conn = self.db_conn.lock().await;
                let _ = update_download_meta(
                    &conn,
                    id,
                    &DownloadMetadataUpdate {
                        final_url: &probe.final_url,
                        file_name: final_display_name,
                        total_bytes: Some(total_bytes),
                        resumable: probe.resumable,
                        etag: probe.etag.as_deref(),
                        last_modified: probe.last_modified.as_deref(),
                    },
                );
                let _ = checkpoint_download(&mut conn, id, total_bytes, &[]);
                update_download_status(&conn, id, DownloadStatus::Completed, None)?;
                let _ = conn.execute(
                    "UPDATE downloads SET checksum_kind = 'sha256', checksum_value = ?1, completed_at = ?2 WHERE id = ?3",
                    rusqlite::params![sha256_hex, chrono::Utc::now().timestamp_millis(), id],
                );
                let _ = crate::storage::db::log_event(
                    &conn,
                    "INFO",
                    Some(id),
                    &format!("Completed '{}' ({} bytes)", final_display_name, total_bytes),
                );
                drop(conn);

                if let Ok(guard) = self.completion_notifier.lock() {
                    if let Some(ref cb) = *guard {
                        cb(CompletionEvent {
                            success: true,
                            file_name: final_display_name.to_string(),
                            final_path: final_path.clone(),
                            total_bytes,
                            error: None,
                        });
                    }
                }
            }
            Ok(crate::engine::single::DownloadOutcome::Paused { downloaded_bytes, .. }) => {
                let mut conn = self.db_conn.lock().await;
                let _ = checkpoint_download(&mut conn, id, downloaded_bytes, &[]);
                update_download_status(&conn, id, DownloadStatus::Paused, None)?;
            }
            Err(e) => {
                let err_str = e.to_string();
                let status = if err_str.contains("403") || err_str.contains("410") {
                    DownloadStatus::WaitingRefresh
                } else {
                    DownloadStatus::Failed
                };
                let conn = self.db_conn.lock().await;
                update_download_status(&conn, id, status, Some(&err_str))?;
                let _ = crate::storage::db::log_event(
                    &conn,
                    "ERROR",
                    Some(id),
                    &format!("Download failed: {}", err_str),
                );
                drop(conn);

                if let Ok(guard) = self.completion_notifier.lock() {
                    if let Some(ref cb) = *guard {
                        cb(CompletionEvent {
                            success: false,
                            file_name: probe.file_name.clone(),
                            final_path: PathBuf::from(&record.save_dir).join(&probe.file_name),
                            total_bytes: probe.total_bytes.unwrap_or(0),
                            error: Some(err_str.clone()),
                        });
                    }
                }
                return Err(e);
            }
        }

        // Trigger queue auto-promotion and schedule check
        let q_id = record.queue_id.clone().unwrap_or_else(|| "default".to_string());
        let mgr_clone = self.clone();
        tokio::spawn(async move {
            let _ = mgr_clone.on_download_finished(&q_id).await;
        });

        Ok(())
    }

    /// Hook invoked when a download completes, fails, or is paused
    pub async fn on_download_finished(&self, queue_id: &str) -> Result<()> {
        // 1. Promote next queued downloads
        let _ = self.start_queue(queue_id).await;

        // 2. Check if schedule post-actions are configured for this queue
        let schedules = {
            let conn = self.db_conn.lock().await;
            list_schedules(&conn)?
        };

        for s in schedules {
            if s.queue_id == queue_id && s.enabled && s.after_action != "none" {
                let downloads = {
                    let conn = self.db_conn.lock().await;
                    list_downloads(&conn, None)?
                };
                let q_downloads: Vec<DownloadRecord> = downloads
                    .into_iter()
                    .filter(|d| match d.queue_id.as_deref() {
                        Some(q) => q == queue_id,
                        None => queue_id == "default",
                    })
                    .collect();

                if let Some(scheduler::SchedulerAction::ExecuteAfterAction { action, command, .. }) =
                    scheduler::Scheduler::evaluate_queue_completion(&s, &q_downloads)
                {
                    let _ = crate::system::power::execute_power_action(&action, command.as_deref());
                }
            }
        }

        Ok(())
    }

    /// Pause an active download
    pub async fn pause(&self, id: &str) -> Result<()> {
        let record = {
            let conn = self.db_conn.lock().await;
            get_download(&conn, id)?.ok_or_else(|| anyhow!("Download {} not found", id))?
        };

        if !record.status.can_transition_to(DownloadStatus::Paused) {
            return Err(anyhow!("Cannot pause download in status {}", record.status));
        }

        let tasks = self.active_tasks.lock().await;
        if let Some(task) = tasks.get(id) {
            task.cancel_token.store(true, Ordering::Relaxed);
        } else {
            let conn = self.db_conn.lock().await;
            update_download_status(&conn, id, DownloadStatus::Paused, None)?;
        }

        let q_id = record.queue_id.clone().unwrap_or_else(|| "default".to_string());
        let mgr_clone = self.clone();
        tokio::spawn(async move {
            let _ = mgr_clone.on_download_finished(&q_id).await;
        });

        Ok(())
    }

    /// Cancel a download, optionally deleting partially downloaded file
    pub async fn cancel(&self, id: &str, delete_file: bool) -> Result<()> {
        let record = {
            let conn = self.db_conn.lock().await;
            get_download(&conn, id)?.ok_or_else(|| anyhow!("Download {} not found", id))?
        };

        let tasks = self.active_tasks.lock().await;
        if let Some(task) = tasks.get(id) {
            task.cancel_token.store(true, Ordering::Relaxed);
        }

        {
            let conn = self.db_conn.lock().await;
            update_download_status(&conn, id, DownloadStatus::Canceled, None)?;
        }

        self.torrent_engine.remove_torrent(id, delete_file).await;

        if delete_file {
            let part_path = PathBuf::from(&record.save_dir).join(format!("{}.part", record.file_name));
            let final_path = PathBuf::from(&record.save_dir).join(&record.file_name);
            let _ = std::fs::remove_file(part_path);
            let _ = std::fs::remove_file(final_path);
        }

        let q_id = record.queue_id.clone().unwrap_or_else(|| "default".to_string());
        let mgr_clone = self.clone();
        tokio::spawn(async move {
            let _ = mgr_clone.on_download_finished(&q_id).await;
        });

        Ok(())
    }

    /// Refresh expired URL (e.g. 403 / 410)
    pub async fn refresh_url(&self, id: &str, new_url: &str) -> Result<()> {
        let record = {
            let conn = self.db_conn.lock().await;
            get_download(&conn, id)?.ok_or_else(|| anyhow!("Download {} not found", id))?
        };

        if record.status != DownloadStatus::WaitingRefresh && record.status != DownloadStatus::Failed {
            return Err(anyhow!(
                "Cannot refresh URL when download status is {}",
                record.status
            ));
        }

        {
            let conn = self.db_conn.lock().await;
            update_download_url(&conn, id, new_url)?;
            update_download_status(&conn, id, DownloadStatus::Paused, None)?;
        }

        Ok(())
    }

    /// Recover any downloads interrupted by crash or sudden kill on app startup
    pub async fn recover_interrupted_downloads(&self) -> Result<Vec<String>> {
        let interrupted = {
            let conn = self.db_conn.lock().await;
            let mut stmt = conn.prepare(
                "SELECT id FROM downloads WHERE status IN ('downloading', 'probing', 'verifying')",
            )?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            let mut ids = Vec::new();
            for r in rows {
                ids.push(r?);
            }
            ids
        };

        for id in &interrupted {
            let conn = self.db_conn.lock().await;
            update_download_status(
                &conn,
                id,
                DownloadStatus::Paused,
                Some("Recovered from previous session after abnormal termination"),
            )?;
        }

        Ok(interrupted)
    }

    /// Launch background scheduler loop checking every 1 second
    pub fn start_scheduler_loop(&self) -> tokio::task::JoinHandle<()> {
        let mgr = self.clone();
        tokio::spawn(async move {
            let mut last_checked_minute = String::new();
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let now = chrono::Local::now();
                let current_time = now.format("%H:%M").to_string();
                if current_time == last_checked_minute {
                    continue;
                }
                last_checked_minute = current_time.clone();

                let weekday = now.weekday().num_days_from_monday() as u8;

                let schedules = {
                    let conn = mgr.db_conn.lock().await;
                    list_schedules(&conn).unwrap_or_default()
                };

                let actions = scheduler::Scheduler::evaluate_time_triggers(&schedules, weekday, &current_time);
                for action in actions {
                    match action {
                        scheduler::SchedulerAction::StartQueue(q_id) => {
                            let _ = mgr.start_queue(&q_id).await;
                        }
                        scheduler::SchedulerAction::StopQueue(q_id) => {
                            let _ = mgr.pause_queue(&q_id).await;
                        }
                        _ => {}
                    }
                }
            }
        })
    }

    pub async fn get_db_conn(&self) -> tokio::sync::MutexGuard<'_, rusqlite::Connection> {
        self.db_conn.lock().await
    }

    pub async fn get_download(&self, id: &str) -> Result<Option<DownloadRecord>> {
        let conn = self.db_conn.lock().await;
        Ok(get_download(&conn, id)?)
    }

    pub async fn list_downloads(&self, filter: Option<DownloadStatus>) -> Result<Vec<DownloadRecord>> {
        let conn = self.db_conn.lock().await;
        Ok(list_downloads(&conn, filter)?)
    }

    pub fn set_ui_waker(&self, waker: UiWaker) {
        if let Ok(mut guard) = self.ui_waker.lock() {
            *guard = Some(waker);
        }
    }

    pub fn set_completion_notifier(&self, notifier: CompletionNotifier) {
        if let Ok(mut guard) = self.completion_notifier.lock() {
            *guard = Some(notifier);
        }
    }

    pub fn record_live_progress(
        &self,
        id: &str,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        raw_speed_bps: u64,
    ) {
        if let Ok(mut guard) = self.live_stats.lock() {
            let prev_speed = guard.get(id).map(|s| s.speed_bps).unwrap_or(0);
            let smoothed = if prev_speed == 0 {
                raw_speed_bps
            } else if raw_speed_bps == 0 {
                (prev_speed * 2) / 3
            } else {
                (prev_speed + raw_speed_bps * 2) / 3
            };
            guard.insert(
                id.to_string(),
                LiveProgressStats {
                    downloaded_bytes,
                    total_bytes,
                    speed_bps: smoothed,
                },
            );
        }
    }

    pub fn get_live_stats(&self, id: &str) -> Option<LiveProgressStats> {
        self.live_stats
            .lock()
            .ok()
            .and_then(|guard| guard.get(id).cloned())
    }

    pub fn push_pending_prompt(&self, prompt: PendingDownloadPrompt) {
        let now = std::time::Instant::now();
        if let Ok(mut recent) = self.recent_prompt_urls.lock() {
            recent.retain(|_, ts| now.duration_since(*ts) < Duration::from_secs(3));
            if recent.contains_key(&prompt.url) {
                return;
            }
            recent.insert(prompt.url.clone(), now);
        }
        if let Ok(mut guard) = self.pending_prompts.lock() {
            if guard.iter().any(|p| p.url == prompt.url) {
                return;
            }
            guard.push(prompt);
        }
        if let Ok(guard) = self.ui_waker.lock() {
            if let Some(ref waker) = *guard {
                waker();
            }
        }
    }

    pub fn pop_pending_prompt(&self) -> Option<PendingDownloadPrompt> {
        self.pending_prompts
            .lock()
            .ok()
            .and_then(|mut guard| {
                if guard.is_empty() {
                    None
                } else {
                    Some(guard.remove(0))
                }
            })
    }

    pub async fn get_segments(&self, id: &str) -> Result<Vec<SegmentRecord>> {
        let conn = self.db_conn.lock().await;
        Ok(get_segments(&conn, id)?)
    }
}
