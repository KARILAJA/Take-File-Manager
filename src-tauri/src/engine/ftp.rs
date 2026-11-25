use anyhow::{anyhow, Result};
use reqwest::Url;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};
use suppaftp::tokio_rustls::rustls::{ClientConfig, RootCertStore};
use suppaftp::tokio_rustls::TlsConnector;
use suppaftp::types::FileType;
use tokio::io::AsyncReadExt;

use crate::engine::filename::sanitize_filename;
use crate::engine::limiter::RateLimiter;
use crate::engine::probe::ProbeInfo;
use crate::engine::single::{DownloadOutcome, ProgressUpdate};

#[derive(Debug, Clone)]
pub struct FtpTarget {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub remote_path: String,
    pub file_name: String,
    pub is_ftps: bool,
}

pub fn is_ftp_url(url: &str) -> bool {
    let lower = url.trim().to_lowercase();
    lower.starts_with("ftp://") || lower.starts_with("ftps://")
}

pub fn parse_ftp_url(url_str: &str) -> Result<FtpTarget> {
    let url = Url::parse(url_str)?;
    let is_ftps = match url.scheme().to_lowercase().as_str() {
        "ftp" => false,
        "ftps" => true,
        other => return Err(anyhow!("Unsupported scheme '{}' for FTP", other)),
    };

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("Missing FTP host in URL"))?
        .to_string();
    let port = url.port().unwrap_or(21);

    let raw_user = url.username();
    let username = if raw_user.is_empty() {
        "anonymous".to_string()
    } else {
        percent_encoding::percent_decode_str(raw_user)
            .decode_utf8_lossy()
            .to_string()
    };

    let password = url
        .password()
        .map(|p| {
            percent_encoding::percent_decode_str(p)
                .decode_utf8_lossy()
                .to_string()
        })
        .unwrap_or_else(|| "turboget@example.com".to_string());

    let decoded_path = percent_encoding::percent_decode_str(url.path())
        .decode_utf8_lossy()
        .to_string();
    let remote_path = if decoded_path.is_empty() {
        "/".to_string()
    } else {
        decoded_path
    };

    let file_name = remote_path
        .rsplit('/')
        .find(|s| !s.is_empty())
        .map(sanitize_filename)
        .unwrap_or_else(|| "download.bin".to_string());

    Ok(FtpTarget {
        host,
        port,
        username,
        password,
        remote_path,
        file_name,
        is_ftps,
    })
}

async fn connect_ftp(target: &FtpTarget) -> Result<AsyncRustlsFtpStream> {
    let addr = format!("{}:{}", target.host, target.port);
    let mut stream = AsyncRustlsFtpStream::connect(&addr)
        .await
        .map_err(|e| anyhow!("FTP connect failed to {}: {}", addr, e))?;

    if target.is_ftps {
        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls_cfg = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let connector = AsyncRustlsConnector::from(TlsConnector::from(Arc::new(tls_cfg)));
        stream = stream
            .into_secure(connector, &target.host)
            .await
            .map_err(|e| anyhow!("FTPS TLS handshake failed: {}", e))?;
    }

    stream
        .login(&target.username, &target.password)
        .await
        .map_err(|e| anyhow!("FTP login failed: {}", e))?;

    stream
        .transfer_type(FileType::Binary)
        .await
        .map_err(|e| anyhow!("FTP TYPE I failed: {}", e))?;

    Ok(stream)
}

/// Probe FTP/FTPS file size and resume support (`REST STREAM`)
pub async fn probe_ftp(url: &str) -> Result<ProbeInfo> {
    let target = parse_ftp_url(url)?;
    let mut ftp = connect_ftp(&target).await?;

    let size = ftp.size(&target.remote_path).await.ok().map(|s| s as u64);
    let resumable = match ftp.feat().await {
        Ok(feats) => feats.contains_key("REST") || size.is_some(),
        Err(_) => true,
    };

    let _ = ftp.quit().await;

    Ok(ProbeInfo {
        url: url.to_string(),
        final_url: url.to_string(),
        file_name: target.file_name,
        total_bytes: size,
        resumable,
        etag: None,
        last_modified: None,
        content_type: Some("application/octet-stream".to_string()),
    })
}

pub struct FtpDownloader {
    pub url: String,
    pub save_dir: PathBuf,
    pub file_name: String,
    pub total_bytes: Option<u64>,
    pub rate_limiter: Option<Arc<RateLimiter>>,
}

impl FtpDownloader {
    pub fn new(
        url: String,
        save_dir: PathBuf,
        file_name: String,
        total_bytes: Option<u64>,
    ) -> Self {
        Self {
            url,
            save_dir,
            file_name,
            total_bytes,
            rate_limiter: None,
        }
    }

    pub fn with_rate_limiter(mut self, limiter: Arc<RateLimiter>) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

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
        let final_path = self.save_dir.join(&self.file_name);

        let existing_bytes = if part_path.exists() {
            std::fs::metadata(&part_path)?.len()
        } else {
            0
        };

        if let Some(total) = self.total_bytes {
            if existing_bytes >= total && total > 0 {
                return finalize_ftp(&part_path, &final_path);
            }
        }

        if cancel_token.load(Ordering::Relaxed) {
            return Ok(DownloadOutcome::Paused {
                part_path,
                downloaded_bytes: existing_bytes,
            });
        }

        let target = parse_ftp_url(&self.url)?;
        let mut ftp = connect_ftp(&target).await?;

        let (mut file, mut current_written) = if existing_bytes > 0 {
            if ftp.resume_transfer(existing_bytes as usize).await.is_ok() {
                let mut f = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .read(true)
                    .truncate(false)
                    .open(&part_path)?;
                f.seek(SeekFrom::End(0))?;
                (f, existing_bytes)
            } else {
                let f = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .read(true)
                    .open(&part_path)?;
                (f, 0u64)
            }
        } else {
            let f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .read(true)
                .open(&part_path)?;
            (f, 0u64)
        };

        let mut data_stream = ftp
            .retr_as_stream(&target.remote_path)
            .await
            .map_err(|e| anyhow!("FTP RETR failed for {}: {}", target.remote_path, e))?;

        let mut buf = vec![0u8; 64 * 1024];
        let mut last_progress = Instant::now();
        let mut bytes_since_last = 0u64;
        let mut paused = false;

        loop {
            if cancel_token.load(Ordering::Relaxed) {
                paused = true;
                break;
            }

            let n = data_stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }

            if let Some(ref lim) = self.rate_limiter {
                lim.acquire(n).await;
            }

            file.write_all(&buf[..n])?;
            current_written += n as u64;
            bytes_since_last += n as u64;

            let elapsed = last_progress.elapsed();
            if elapsed >= Duration::from_millis(50) {
                let speed = (bytes_since_last as f64 / elapsed.as_secs_f64()) as u64;
                on_progress(ProgressUpdate {
                    downloaded_bytes: current_written,
                    total_bytes: self.total_bytes,
                    speed_bps: speed,
                    segments: Vec::new(),
                });
                last_progress = Instant::now();
                bytes_since_last = 0;
            }
        }

        file.flush()?;
        drop(file);

        if paused {
            drop(data_stream);
            return Ok(DownloadOutcome::Paused {
                part_path,
                downloaded_bytes: current_written,
            });
        }

        data_stream
            .finish()
            .await
            .map_err(|e| anyhow!("FTP finalize stream failed: {}", e))?;
        let _ = ftp.quit().await;

        on_progress(ProgressUpdate {
            downloaded_bytes: current_written,
            total_bytes: self.total_bytes.or(Some(current_written)),
            speed_bps: 0,
            segments: Vec::new(),
        });

        finalize_ftp(&part_path, &final_path)
    }
}

fn finalize_ftp(part_path: &Path, final_path: &Path) -> Result<DownloadOutcome> {
    let mut file = File::open(part_path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    drop(file);

    let sha256_hex = format!("{:x}", hasher.finalize());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ftp_urls() {
        let t1 = parse_ftp_url("ftp://ftp.example.com/pub/release_v1.tar.gz").unwrap();
        assert_eq!(t1.host, "ftp.example.com");
        assert_eq!(t1.port, 21);
        assert_eq!(t1.username, "anonymous");
        assert!(!t1.is_ftps);
        assert_eq!(t1.file_name, "release_v1.tar.gz");

        let t2 = parse_ftp_url("ftps://alice:s3cr3t@secure.example.org:2121/dir/archive.zip").unwrap();
        assert_eq!(t2.host, "secure.example.org");
        assert_eq!(t2.port, 2121);
        assert_eq!(t2.username, "alice");
        assert_eq!(t2.password, "s3cr3t");
        assert!(t2.is_ftps);
        assert_eq!(t2.file_name, "archive.zip");
    }
}
