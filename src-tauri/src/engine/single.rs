use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, IF_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: u64,
    pub segments: Vec<crate::storage::db::SegmentRecord>,
}

#[derive(Debug)]
pub enum DownloadOutcome {
    Completed {
        final_path: PathBuf,
        total_bytes: u64,
        sha256_hex: String,
    },
    Paused {
        part_path: PathBuf,
        downloaded_bytes: u64,
    },
}

pub struct SingleStreamDownloader {
    pub client: Client,
    pub url: String,
    pub save_dir: PathBuf,
    pub file_name: String,
    pub etag: Option<String>,
    pub total_bytes: Option<u64>,
    pub headers: Vec<(String, String)>,
    pub max_retries: u32,
    pub rate_limiter: Option<Arc<crate::engine::limiter::RateLimiter>>,
}

impl SingleStreamDownloader {
    pub fn new(
        client: Client,
        url: String,
        save_dir: PathBuf,
        file_name: String,
        etag: Option<String>,
        total_bytes: Option<u64>,
    ) -> Self {
        Self {
            client,
            url,
            save_dir,
            file_name,
            etag,
            total_bytes,
            headers: Vec::new(),
            max_retries: 5,
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

    /// Execute single stream download with resume, retry, and cancellation support
    pub async fn execute<F>(
        &self,
        cancel_token: Arc<AtomicBool>,
        mut on_progress: F,
    ) -> Result<DownloadOutcome>
    where
        F: FnMut(ProgressUpdate),
    {
        std::fs::create_dir_all(&self.save_dir)?;
        let part_path = self.save_dir.join(format!("{}.part", self.file_name));

        let mut retries = 0;
        let mut backoff = Duration::from_millis(500);

        loop {
            if cancel_token.load(Ordering::Relaxed) {
                let downloaded = std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
                return Ok(DownloadOutcome::Paused {
                    part_path,
                    downloaded_bytes: downloaded,
                });
            }

            match self.download_attempt(&part_path, cancel_token.clone(), &mut on_progress).await {
                Ok(outcome) => return Ok(outcome),
                Err(e) => {
                    retries += 1;
                    if retries > self.max_retries {
                        return Err(anyhow!(
                            "Failed to download after {} retries: {}",
                            self.max_retries,
                            e
                        ));
                    }

                    // Exponential backoff
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(8));
                }
            }
        }
    }

    async fn download_attempt<F>(
        &self,
        part_path: &Path,
        cancel_token: Arc<AtomicBool>,
        on_progress: &mut F,
    ) -> Result<DownloadOutcome>
    where
        F: FnMut(ProgressUpdate),
    {
        let mut downloaded_bytes = 0u64;
        let mut resume = false;

        if part_path.exists() {
            downloaded_bytes = std::fs::metadata(part_path)?.len();
            if let Some(total) = self.total_bytes {
                if downloaded_bytes >= total && total > 0 {
                    // Already completely downloaded, compute hash and finalize
                    return self.finalize(part_path);
                }
            }
            if downloaded_bytes > 0 {
                resume = true;
            }
        }

        let mut req_headers = HeaderMap::new();
        for (k, v) in &self.headers {
            if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                req_headers.insert(name, val);
            }
        }

        if resume {
            let range_val = format!("bytes={}-", downloaded_bytes);
            if let Ok(hv) = HeaderValue::from_str(&range_val) {
                req_headers.insert(RANGE, hv);
            }
            if let Some(ref etag) = self.etag {
                if let Ok(hv) = HeaderValue::from_str(etag) {
                    req_headers.insert(IF_RANGE, hv);
                }
            }
        }

        let response = self
            .client
            .get(&self.url)
            .headers(req_headers)
            .send()
            .await?;

        let status = response.status();

        let (mut file, mut current_written) = if resume && status == StatusCode::PARTIAL_CONTENT {
            let mut f = OpenOptions::new()
                .create(true)
                .write(true)
                .read(true)
                .truncate(false)
                .open(part_path)?;
            f.seek(SeekFrom::End(0))?;
            (f, downloaded_bytes)
        } else {
            // Server ignored range or fresh download
            let f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .read(true)
                .open(part_path)?;
            (f, 0u64)
        };

        let mut stream = response.bytes_stream();
        let mut last_progress_time = Instant::now();
        let mut bytes_since_last_progress = 0u64;

        while let Some(chunk_res) = stream.next().await {
            if cancel_token.load(Ordering::Relaxed) {
                file.flush()?;
                return Ok(DownloadOutcome::Paused {
                    part_path: part_path.to_path_buf(),
                    downloaded_bytes: current_written,
                });
            }

            let chunk = chunk_res?;
            if let Some(ref limiter) = self.rate_limiter {
                limiter.acquire(chunk.len()).await;
            }
            file.write_all(&chunk)?;
            let len = chunk.len() as u64;
            current_written += len;
            bytes_since_last_progress += len;

            let elapsed = last_progress_time.elapsed();
            if elapsed >= Duration::from_millis(50) {
                let current_speed = (bytes_since_last_progress as f64 / elapsed.as_secs_f64()) as u64;
                on_progress(ProgressUpdate {
                    downloaded_bytes: current_written,
                    total_bytes: self.total_bytes,
                    speed_bps: current_speed,
                    segments: Vec::new(),
                });
                last_progress_time = Instant::now();
                bytes_since_last_progress = 0;
            }

            if cancel_token.load(Ordering::Relaxed) {
                file.flush()?;
                return Ok(DownloadOutcome::Paused {
                    part_path: part_path.to_path_buf(),
                    downloaded_bytes: current_written,
                });
            }
        }

        file.flush()?;

        // Send final 100% progress
        on_progress(ProgressUpdate {
            downloaded_bytes: current_written,
            total_bytes: self.total_bytes.or(Some(current_written)),
            speed_bps: 0,
            segments: Vec::new(),
        });

        self.finalize(part_path)
    }

    fn finalize(&self, part_path: &Path) -> Result<DownloadOutcome> {
        let final_path = self.save_dir.join(&self.file_name);

        // Compute SHA-256
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

        // Rename .part -> final
        if final_path.exists() {
            std::fs::remove_file(&final_path)?;
        }
        std::fs::rename(part_path, &final_path)?;

        Ok(DownloadOutcome::Completed {
            final_path,
            total_bytes: total,
            sha256_hex,
        })
    }
}
