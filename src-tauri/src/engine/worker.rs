use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, IF_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::storage::db::SegmentRecord;
use crate::storage::part_file::{create_part_file, write_all_at};
use super::probe::ProbeInfo;
use super::segmenter::SegmentManager;
use super::single::{DownloadOutcome, ProgressUpdate, SingleStreamDownloader};

pub struct MultiSegmentDownloader {
    pub client: Client,
    pub probe: ProbeInfo,
    pub save_dir: PathBuf,
    pub num_segments: u32,
    pub min_segment_size: u64,
    pub headers: Vec<(String, String)>,
    pub initial_segments: Option<Vec<SegmentRecord>>,
    pub rate_limiter: Option<Arc<crate::engine::limiter::RateLimiter>>,
}

impl MultiSegmentDownloader {
    pub fn new(
        client: Client,
        probe: ProbeInfo,
        save_dir: PathBuf,
        num_segments: u32,
    ) -> Self {
        Self {
            client,
            probe,
            save_dir,
            num_segments: num_segments.clamp(1, 32),
            min_segment_size: 512 * 1024, // 512 KB
            headers: Vec::new(),
            initial_segments: None,
            rate_limiter: None,
        }
    }

    pub fn with_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.headers = headers;
        self
    }

    pub fn with_rate_limiter(mut self, limiter: Arc<crate::engine::limiter::RateLimiter>) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    pub fn with_min_segment_size(mut self, min_size: u64) -> Self {
        self.min_segment_size = min_size;
        self
    }

    pub fn with_initial_segments(mut self, segs: Vec<SegmentRecord>) -> Self {
        if !segs.is_empty() {
            self.initial_segments = Some(segs);
        }
        self
    }

    /// Execute multi-segment download, falling back to single-stream if range is not supported
    pub async fn execute<F>(
        &self,
        cancel_token: Arc<AtomicBool>,
        on_progress: F,
    ) -> Result<DownloadOutcome>
    where
        F: FnMut(ProgressUpdate) + Send + 'static,
    {
        // Fallback to single stream if server does not support Range or file size is unknown/small
        let total_size = match self.probe.total_bytes {
            Some(size) if size >= 1024 * 1024 && self.probe.resumable => size,
            _ => {
                let mut single = SingleStreamDownloader::new(
                    self.client.clone(),
                    self.probe.url.clone(),
                    self.save_dir.clone(),
                    self.probe.file_name.clone(),
                    self.probe.etag.clone(),
                    self.probe.total_bytes,
                )
                .with_headers(self.headers.clone());
                if let Some(ref lim) = self.rate_limiter {
                    single = single.with_rate_limiter(lim.clone());
                }
                return single.execute(cancel_token, on_progress).await;
            }
        };

        std::fs::create_dir_all(&self.save_dir)?;
        let part_path = self.save_dir.join(format!("{}.part", self.probe.file_name));
        let final_path = self.save_dir.join(&self.probe.file_name);

        // Pre-allocate or open existing part file without truncation
        let file = Arc::new(create_part_file(&part_path, total_size)?);

        let initial_mgr = if let Some(ref saved) = self.initial_segments {
            SegmentManager::from_saved_segments(total_size, self.min_segment_size, saved.clone())
        } else {
            SegmentManager::new(total_size, self.num_segments, self.min_segment_size)
        };

        let segment_mgr = Arc::new(Mutex::new(initial_mgr));

        let mut worker_handles = Vec::with_capacity(self.num_segments as usize);

        for worker_id in 0..self.num_segments as usize {
            let ctx = WorkerContext {
                worker_id,
                client: self.client.clone(),
                url: self.probe.url.clone(),
                etag: self.probe.etag.clone(),
                headers: self.headers.clone(),
                file: file.clone(),
                segment_mgr: segment_mgr.clone(),
                cancel_token: cancel_token.clone(),
                min_segment_size: self.min_segment_size,
                limiter: self.rate_limiter.clone(),
            };

            let handle = tokio::spawn(async move { worker_loop(ctx).await });
            worker_handles.push(handle);
        }

        // Progress aggregation loop
        let seg_mgr_progress = segment_mgr.clone();
        let cancel_progress = cancel_token.clone();
        let mut last_progress_time = Instant::now();
        let mut last_bytes = 0u64;

        let progress_task = tokio::spawn(async move {
            let mut progress_cb = on_progress;
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let (downloaded, seg_records) = {
                    let mgr = seg_mgr_progress.lock().await;
                    let dl = mgr.total_downloaded();
                    let records: Vec<SegmentRecord> = mgr
                        .segments
                        .iter()
                        .map(|s| SegmentRecord {
                            download_id: String::new(),
                            idx: s.idx,
                            start_byte: s.start_byte,
                            end_byte: s.end_byte,
                            written: s.written,
                        })
                        .collect();
                    (dl, records)
                };

                let elapsed = last_progress_time.elapsed();
                let speed = if elapsed.as_secs_f64() > 0.0 {
                    ((downloaded.saturating_sub(last_bytes)) as f64 / elapsed.as_secs_f64()) as u64
                } else {
                    0
                };

                progress_cb(ProgressUpdate {
                    downloaded_bytes: downloaded,
                    total_bytes: Some(total_size),
                    speed_bps: speed,
                    segments: seg_records,
                });

                last_progress_time = Instant::now();
                last_bytes = downloaded;

                if downloaded >= total_size || cancel_progress.load(Ordering::Relaxed) {
                    break;
                }
            }
        });

        // Wait for all workers
        for handle in worker_handles {
            if let Err(e) = handle.await? {
                return Err(anyhow!("Worker failed: {}", e));
            }
        }

        let _ = progress_task.await;

        if cancel_token.load(Ordering::Relaxed) {
            let downloaded = {
                let mgr = segment_mgr.lock().await;
                mgr.total_downloaded()
            };
            return Ok(DownloadOutcome::Paused {
                part_path,
                downloaded_bytes: downloaded,
            });
        }

        let is_completed = {
            let mgr = segment_mgr.lock().await;
            mgr.is_all_completed()
        };

        if !is_completed {
            return Err(anyhow!("Download incomplete after all workers exited"));
        }

        file.sync_all()?;
        drop(file);

        // Finalize: Calculate hash and rename
        finalize_download(&part_path, &final_path)
    }
}

struct WorkerContext {
    worker_id: usize,
    client: Client,
    url: String,
    etag: Option<String>,
    headers: Vec<(String, String)>,
    file: Arc<File>,
    segment_mgr: Arc<Mutex<SegmentManager>>,
    cancel_token: Arc<AtomicBool>,
    min_segment_size: u64,
    limiter: Option<Arc<crate::engine::limiter::RateLimiter>>,
}

async fn worker_loop(ctx: WorkerContext) -> Result<()> {
    loop {
        if ctx.cancel_token.load(Ordering::Relaxed) {
            break;
        }

        // Claim existing unassigned segment or dynamically split an active segment
        let segment = {
            let mut mgr = ctx.segment_mgr.lock().await;
            if let Some(seg) = mgr.claim_next_segment(ctx.worker_id) {
                Some(seg)
            } else {
                mgr.try_dynamic_split(ctx.worker_id, ctx.min_segment_size)
            }
        };

        let seg = match segment {
            Some(s) => s,
            None => {
                // No more segments available to download or split
                break;
            }
        };

        let mut current_offset = seg.start_byte + seg.written;
        if current_offset > seg.end_byte {
            let mut mgr = ctx.segment_mgr.lock().await;
            mgr.mark_completed(seg.idx);
            continue;
        }

        // Build Range request
        let mut req_headers = HeaderMap::new();
        for (k, v) in &ctx.headers {
            if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                req_headers.insert(name, val);
            }
        }

        let range_val = format!("bytes={}-{}", current_offset, seg.end_byte);
        if let Ok(hv) = HeaderValue::from_str(&range_val) {
            req_headers.insert(RANGE, hv);
        }
        if let Some(ref tag) = ctx.etag {
            if let Ok(hv) = HeaderValue::from_str(tag) {
                req_headers.insert(IF_RANGE, hv);
            }
        }

        let resp = match ctx.client.get(&ctx.url).headers(req_headers).send().await {
            Ok(r) if r.status() == StatusCode::PARTIAL_CONTENT || r.status() == StatusCode::OK => r,
            Ok(r) => {
                return Err(anyhow!(
                    "Worker {} received unexpected HTTP status {}",
                    ctx.worker_id,
                    r.status()
                ));
            }
            Err(e) => {
                return Err(anyhow!("Worker {} network error: {}", ctx.worker_id, e));
            }
        };

        let mut stream = resp.bytes_stream();

        while let Some(chunk_res) = stream.next().await {
            if ctx.cancel_token.load(Ordering::Relaxed) {
                break;
            }

            let chunk = chunk_res?;
            let current_seg_end = {
                let mgr = ctx.segment_mgr.lock().await;
                mgr.get_segment(seg.idx).map(|s| s.end_byte).unwrap_or(seg.end_byte)
            };

            if current_offset > current_seg_end {
                // Segment was dynamically truncated
                break;
            }

            let max_write = (current_seg_end - current_offset + 1) as usize;
            let to_write = chunk.len().min(max_write);

            if to_write > 0 {
                if let Some(ref limiter) = ctx.limiter {
                    limiter.acquire(to_write).await;
                }
                write_all_at(&ctx.file, current_offset, &chunk[..to_write])?;
                current_offset += to_write as u64;

                let mut mgr = ctx.segment_mgr.lock().await;
                mgr.record_progress(seg.idx, to_write as u64);
            }

            if current_offset > current_seg_end {
                break;
            }
        }

        let is_done = {
            let mgr = ctx.segment_mgr.lock().await;
            mgr.get_segment(seg.idx).map(|s| s.is_done()).unwrap_or(true)
        };

        if is_done {
            let mut mgr = ctx.segment_mgr.lock().await;
            mgr.mark_completed(seg.idx);
        }
    }

    Ok(())
}

fn finalize_download(part_path: &Path, final_path: &Path) -> Result<DownloadOutcome> {
    let mut hasher = Sha256::new();
    let mut f = File::open(part_path)?;
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;

    loop {
        let n = f.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        total += n as u64;
    }

    let hash_bytes = hasher.finalize();
    let sha256_hex = format!("{:x}", hash_bytes);

    if final_path.exists() {
        std::fs::remove_file(final_path)?;
    }
    std::fs::rename(part_path, final_path)?;

    Ok(DownloadOutcome::Completed {
        final_path: final_path.to_path_buf(),
        total_bytes: total,
        sha256_hex,
    })
}
