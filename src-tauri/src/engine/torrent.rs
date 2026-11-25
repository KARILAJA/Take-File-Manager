use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use librqbit::{
    limits::LimitsConfig,
    torrent_from_bytes, AddTorrent, AddTorrentOptions, DhtSessionConfig, ListenerMode,
    ListenerOptions, Magnet, ManagedTorrent, Session, SessionOptions,
};
use percent_encoding::percent_decode_str;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::engine::probe::ProbeInfo;
use crate::engine::single::{DownloadOutcome, ProgressUpdate};

/// Returns true if the URL or local path refers to a BitTorrent magnet link or `.torrent` file.
pub fn is_torrent_url(target: &str) -> bool {
    let trimmed = target.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("magnet:?") || lower.starts_with("magnet:") {
        return true;
    }
    let without_frag = lower.split('#').next().unwrap_or(&lower);
    let without_query = without_frag.split('?').next().unwrap_or(without_frag);
    without_query.ends_with(".torrent")
}

/// Extract display name (`dn=`) or fallback infohash from a magnet URI.
pub fn parse_magnet_display_name(magnet_url: &str) -> Option<String> {
    let query = magnet_url.split_once('?')?.1;
    let mut xt_hash: Option<String> = None;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k.eq_ignore_ascii_case("dn") && !v.is_empty() {
                let plus_fixed = v.replace('+', " ");
                let decoded = percent_decode_str(&plus_fixed)
                    .decode_utf8_lossy()
                    .trim()
                    .to_string();
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            } else if k.eq_ignore_ascii_case("xt") && xt_hash.is_none() {
                if let Some(hash) = v.rsplit(':').next() {
                    if !hash.is_empty() {
                        xt_hash = Some(format!("torrent_{}", &hash[..hash.len().min(12)]));
                    }
                }
            }
        }
    }
    xt_hash
}

/// Extract optional exact length (`xl=`) from a magnet URI.
pub fn parse_magnet_exact_length(magnet_url: &str) -> Option<u64> {
    let query = magnet_url.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k.eq_ignore_ascii_case("xl") {
                if let Ok(len) = v.parse::<u64>() {
                    return Some(len);
                }
            }
        }
    }
    None
}

async fn load_torrent_bytes(client: &reqwest::Client, target: &str) -> Result<Vec<u8>> {
    let trimmed = target.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        let resp = client.get(trimmed).send().await?.error_for_status()?;
        let mut bytes = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let c = chunk?;
            if bytes.len() + c.len() > 20 * 1024 * 1024 {
                return Err(anyhow!(".torrent file exceeds 20 MiB safety limit"));
            }
            bytes.extend_from_slice(&c);
        }
        Ok(bytes)
    } else {
        Ok(tokio::fs::read(trimmed).await?)
    }
}

fn normalize_magnet_scheme(target: &str) -> String {
    let trimmed = target.trim();
    if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("magnet:") {
        format!("magnet:{}", &trimmed[7..])
    } else {
        trimmed.to_string()
    }
}

/// Probe a magnet URI or `.torrent` file (HTTP URL or local path) without starting peers.
pub async fn probe_torrent(client: &reqwest::Client, target: &str) -> Result<ProbeInfo> {
    let trimmed = target.trim();
    let lower = trimmed.to_ascii_lowercase();

    if lower.starts_with("magnet:") {
        let normalized = normalize_magnet_scheme(trimmed);
        Magnet::parse(&normalized).map_err(|e| anyhow!("Invalid magnet URI: {}", e))?;
        let name =
            parse_magnet_display_name(&normalized).unwrap_or_else(|| "torrent_download".to_string());
        let total_bytes = parse_magnet_exact_length(&normalized);
        return Ok(ProbeInfo {
            url: normalized,
            final_url: trimmed.to_string(),
            file_name: name,
            total_bytes,
            resumable: true,
            etag: None,
            last_modified: None,
            content_type: Some("application/x-bittorrent".to_string()),
        });
    }

    let raw_bytes = load_torrent_bytes(client, trimmed).await?;
    let parsed = torrent_from_bytes(&raw_bytes)
        .map_err(|e| anyhow!("Invalid .torrent metadata: {}", e))?;
    let validated = parsed
        .info
        .data
        .validate()
        .map_err(|e| anyhow!("Invalid .torrent info dictionary: {}", e))?;

    let name = validated
        .name()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            Path::new(trimmed)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("torrent_download")
                .to_string()
        });
    let total_len = validated.lengths().total_length();

    Ok(ProbeInfo {
        url: trimmed.to_string(),
        final_url: trimmed.to_string(),
        file_name: name,
        total_bytes: Some(total_len),
        resumable: true,
        etag: None,
        last_modified: None,
        content_type: Some("application/x-bittorrent".to_string()),
    })
}

fn sha256_file_if_small_or_file(path: &Path) -> Result<String> {
    if !path.is_file() {
        return Ok(String::new());
    }
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Manages the embedded BitTorrent session (`librqbit`) and active torrent handles.
#[derive(Clone, Default)]
pub struct TorrentEngine {
    session: Arc<Mutex<Option<Arc<Session>>>>,
    handles: Arc<Mutex<HashMap<String, Arc<ManagedTorrent>>>>,
    session_opts: Arc<Mutex<Option<SessionOptions>>>,
    initial_peers: Arc<Mutex<Option<Vec<std::net::SocketAddr>>>>,
}

impl TorrentEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn configure_session_options(&self, opts: SessionOptions) {
        let mut guard = self.session_opts.lock().await;
        *guard = Some(opts);
    }

    pub async fn configure_initial_peers(&self, peers: Vec<std::net::SocketAddr>) {
        let mut guard = self.initial_peers.lock().await;
        *guard = Some(peers);
    }

    pub async fn get_or_init_session(&self, default_dir: &Path) -> Result<Arc<Session>> {
        let mut guard = self.session.lock().await;
        if let Some(ref s) = *guard {
            return Ok(s.clone());
        }
        std::fs::create_dir_all(default_dir)?;
        let custom_opts = self.session_opts.lock().await.take();
        let sess = if let Some(opts) = custom_opts {
            Session::new_with_opts(default_dir.to_path_buf(), opts).await?
        } else {
            let dual_opts = SessionOptions {
                dht: Some(DhtSessionConfig {
                    persistence: None,
                    ..Default::default()
                }),
                listen: Some(ListenerOptions {
                    mode: ListenerMode::TcpAndUtp,
                    enable_upnp_port_forwarding: true,
                    ..Default::default()
                }),
                fastresume: false,
                ..Default::default()
            };
            match Session::new_with_opts(default_dir.to_path_buf(), dual_opts).await {
                Ok(s) => s,
                Err(_) => {
                    // Fallback to IPv4-only if dual-stack / IPv6 sockets fail on host OS
                    let v4_opts = SessionOptions {
                        dht: Some(DhtSessionConfig {
                            persistence: None,
                            ..Default::default()
                        }),
                        listen: Some(ListenerOptions {
                            mode: ListenerMode::TcpAndUtp,
                            enable_upnp_port_forwarding: true,
                            ipv4_only: true,
                            ..Default::default()
                        }),
                        fastresume: false,
                        ipv4_only: true,
                        ..Default::default()
                    };
                    Session::new_with_opts(default_dir.to_path_buf(), v4_opts).await?
                }
            }
        };
        *guard = Some(sess.clone());
        Ok(sess)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn download<F>(
        &self,
        download_id: &str,
        client: &reqwest::Client,
        target: &str,
        save_dir: &Path,
        fallback_name: &str,
        speed_limit_bps: Option<u64>,
        cancel_token: Arc<AtomicBool>,
        mut on_progress: F,
    ) -> Result<(DownloadOutcome, Option<String>)>
    where
        F: FnMut(ProgressUpdate) + Send,
    {
        std::fs::create_dir_all(save_dir)?;
        let session = self.get_or_init_session(save_dir).await?;

        let normalized_magnet;
        let add_req = if target.trim().to_ascii_lowercase().starts_with("magnet:") {
            normalized_magnet = normalize_magnet_scheme(target);
            AddTorrent::from_url(&normalized_magnet)
        } else {
            let bytes = load_torrent_bytes(client, target).await?;
            AddTorrent::from_bytes(bytes)
        };

        let download_bps = speed_limit_bps
            .filter(|&b| b > 0)
            .and_then(|b| NonZeroU32::new(b.min(u32::MAX as u64) as u32));

        let initial_peers = self.initial_peers.lock().await.clone();

        let opts = AddTorrentOptions {
            overwrite: true,
            paused: false,
            output_folder: Some(save_dir.to_string_lossy().to_string()),
            initial_peers,
            ratelimits: LimitsConfig {
                download_bps,
                upload_bps: None,
            },
            ..Default::default()
        };

        // Allow cooperative pause/cancel while magnet metadata is resolving via DHT/trackers
        let resp = {
            let add_fut = session.add_torrent(add_req, Some(opts));
            tokio::pin!(add_fut);
            loop {
                if cancel_token.load(Ordering::Relaxed) {
                    return Ok((
                        DownloadOutcome::Paused {
                            part_path: save_dir.to_path_buf(),
                            downloaded_bytes: 0,
                        },
                        None,
                    ));
                }
                tokio::select! {
                    res = &mut add_fut => {
                        break res?;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {}
                }
            }
        };
        let handle = resp
            .into_handle()
            .ok_or_else(|| anyhow!("Failed to create managed torrent handle"))?;

        {
            let mut h_guard = self.handles.lock().await;
            h_guard.insert(download_id.to_string(), handle.clone());
        }

        loop {
            if cancel_token.load(Ordering::Relaxed) {
                let _ = session.pause(&handle).await;
                let stats = handle.stats();
                let mut h_guard = self.handles.lock().await;
                h_guard.remove(download_id);
                return Ok((
                    DownloadOutcome::Paused {
                        part_path: save_dir.to_path_buf(),
                        downloaded_bytes: stats.progress_bytes,
                    },
                    handle.name(),
                ));
            }

            let stats = handle.stats();
            let speed_bps = stats
                .live
                .as_ref()
                .map(|l| l.download_speed.as_bytes())
                .unwrap_or(0);
            let total_opt = if stats.total_bytes > 0 {
                Some(stats.total_bytes)
            } else {
                None
            };

            on_progress(ProgressUpdate {
                downloaded_bytes: stats.progress_bytes,
                total_bytes: total_opt,
                speed_bps,
                segments: Vec::new(),
            });

            if let Some(err) = stats.error {
                let mut h_guard = self.handles.lock().await;
                h_guard.remove(download_id);
                return Err(anyhow!("Torrent error: {}", err));
            }

            if stats.finished {
                let _ = session.pause(&handle).await;
                let resolved_name = handle.name();
                let final_name = resolved_name.as_deref().unwrap_or(fallback_name);
                let final_path = save_dir.join(final_name);
                let sha256_hex = sha256_file_if_small_or_file(&final_path).unwrap_or_default();
                let total_bytes = stats.total_bytes.max(stats.progress_bytes);

                let mut h_guard = self.handles.lock().await;
                h_guard.remove(download_id);

                return Ok((
                    DownloadOutcome::Completed {
                        final_path,
                        total_bytes,
                        sha256_hex,
                    },
                    resolved_name,
                ));
            }

            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    pub async fn remove_torrent(&self, download_id: &str, delete_files: bool) {
        let handle = {
            let mut h_guard = self.handles.lock().await;
            h_guard.remove(download_id)
        };
        if let Some(h) = handle {
            if let Some(sess) = self.session.lock().await.clone() {
                let _ = sess.delete(h.id().into(), delete_files).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_torrent_url_and_magnet_parsing() {
        assert!(is_torrent_url("magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Ubuntu+24.04+LTS&xl=1024"));
        assert!(is_torrent_url("https://releases.ubuntu.com/ubuntu.torrent?mirror=1"));
        assert!(is_torrent_url(r"C:\Users\Test\Downloads\linux.torrent"));
        assert!(!is_torrent_url("https://example.com/archive.zip"));

        let mag = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Ubuntu+24.04+LTS&xl=123456";
        assert_eq!(parse_magnet_display_name(mag).as_deref(), Some("Ubuntu 24.04 LTS"));
        assert_eq!(parse_magnet_exact_length(mag), Some(123456));
    }
}
