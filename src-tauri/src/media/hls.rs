use anyhow::{anyhow, Result};
use futures_util::future::try_join_all;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HlsVariant {
    pub bandwidth: u64,
    pub resolution: Option<String>,
    pub codecs: Option<String>,
    pub uri: String,
    pub audio_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HlsAudioTrack {
    pub group_id: String,
    pub name: String,
    pub language: Option<String>,
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlsSegment {
    pub index: usize,
    pub duration: f64,
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlsMediaPlaylist {
    pub target_duration: Option<f64>,
    pub init_map_uri: Option<String>,
    pub segments: Vec<HlsSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HlsPlaylist {
    Master {
        variants: Vec<HlsVariant>,
        audio_tracks: Vec<HlsAudioTrack>,
    },
    Media(HlsMediaPlaylist),
}

fn resolve_url(base: &Url, relative: &str) -> Result<String> {
    let trimmed = relative.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed.to_string())
    } else {
        Ok(base.join(trimmed)?.to_string())
    }
}

/// Extract quoted or unquoted KEY=VALUE attributes from an M3U8 tag parameter string
fn parse_m3u8_attrs(attr_str: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let bytes = attr_str.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key = attr_str[key_start..i].trim().to_uppercase();
        i += 1; // skip '='

        if i >= bytes.len() {
            break;
        }

        let val = if bytes[i] == b'"' {
            i += 1;
            let val_start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            let v = attr_str[val_start..i].to_string();
            if i < bytes.len() {
                i += 1;
            }
            v
        } else {
            let val_start = i;
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            attr_str[val_start..i].trim().to_string()
        };

        result.push((key, val));
    }

    result
}

pub fn parse_m3u8(content: &str, base_url: &str) -> Result<HlsPlaylist> {
    let base = Url::parse(base_url)?;
    let lines: Vec<&str> = content.lines().map(|l| l.trim()).collect();

    let is_master = lines.iter().any(|l| l.starts_with("#EXT-X-STREAM-INF:"));

    if is_master {
        let mut variants = Vec::new();
        let mut audio_tracks = Vec::new();
        let mut pending_stream_inf: Option<Vec<(String, String)>> = None;

        for line in lines {
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
                let attrs = parse_m3u8_attrs(rest);
                let media_type = attrs.iter().find(|(k, _)| k == "TYPE").map(|(_, v)| v.as_str());
                if media_type == Some("AUDIO") {
                    let group_id = attrs
                        .iter()
                        .find(|(k, _)| k == "GROUP-ID")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default();
                    let name = attrs
                        .iter()
                        .find(|(k, _)| k == "NAME")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| "audio".to_string());
                    let language = attrs.iter().find(|(k, _)| k == "LANGUAGE").map(|(_, v)| v.clone());
                    if let Some(raw_uri) = attrs.iter().find(|(k, _)| k == "URI").map(|(_, v)| v) {
                        audio_tracks.push(HlsAudioTrack {
                            group_id,
                            name,
                            language,
                            uri: resolve_url(&base, raw_uri)?,
                        });
                    }
                }
            } else if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
                pending_stream_inf = Some(parse_m3u8_attrs(rest));
            } else if !line.starts_with('#') {
                if let Some(attrs) = pending_stream_inf.take() {
                    let bandwidth = attrs
                        .iter()
                        .find(|(k, _)| k == "BANDWIDTH")
                        .and_then(|(_, v)| v.parse::<u64>().ok())
                        .unwrap_or(0);
                    let resolution = attrs
                        .iter()
                        .find(|(k, _)| k == "RESOLUTION")
                        .map(|(_, v)| v.clone());
                    let codecs = attrs
                        .iter()
                        .find(|(k, _)| k == "CODECS")
                        .map(|(_, v)| v.clone());
                    let audio_group = attrs
                        .iter()
                        .find(|(k, _)| k == "AUDIO")
                        .map(|(_, v)| v.clone());

                    variants.push(HlsVariant {
                        bandwidth,
                        resolution,
                        codecs,
                        uri: resolve_url(&base, line)?,
                        audio_group,
                    });
                }
            }
        }

        // Sort highest bandwidth first
        variants.sort_by_key(|b| std::cmp::Reverse(b.bandwidth));
        return Ok(HlsPlaylist::Master {
            variants,
            audio_tracks,
        });
    }

    // Media playlist
    let mut target_duration = None;
    let mut init_map_uri = None;
    let mut segments = Vec::new();
    let mut pending_duration: Option<f64> = None;

    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration = rest.trim().parse::<f64>().ok();
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let attrs = parse_m3u8_attrs(rest);
            if let Some((_, raw_uri)) = attrs.iter().find(|(k, _)| k == "URI") {
                init_map_uri = Some(resolve_url(&base, raw_uri)?);
            }
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let dur_part = rest.split(',').next().unwrap_or("0").trim();
            pending_duration = Some(dur_part.parse::<f64>().unwrap_or(0.0));
        } else if !line.starts_with('#') {
            let duration = pending_duration.take().unwrap_or(0.0);
            segments.push(HlsSegment {
                index: segments.len(),
                duration,
                uri: resolve_url(&base, line)?,
            });
        }
    }

    Ok(HlsPlaylist::Media(HlsMediaPlaylist {
        target_duration,
        init_map_uri,
        segments,
    }))
}

pub fn build_header_map(headers: &[(String, String)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in headers {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            map.insert(name, val);
        }
    }
    map
}

pub async fn fetch_text(
    client: &Client,
    url: &str,
    headers: &[(String, String)],
) -> Result<String> {
    let hdrs = build_header_map(headers);
    let resp = client.get(url).headers(hdrs).send().await?;
    if !resp.status().is_success() {
        return Err(anyhow!("HTTP {} fetching {}", resp.status(), url));
    }
    Ok(resp.text().await?)
}

/// Download all segments of an HLS media playlist in parallel and concatenate into `output_path`.
pub async fn download_hls_media_playlist(
    client: &Client,
    media_playlist_url: &str,
    output_path: &Path,
    concurrency: usize,
    headers: &[(String, String)],
) -> Result<PathBuf> {
    let text = fetch_text(client, media_playlist_url, headers).await?;
    let media = match parse_m3u8(&text, media_playlist_url)? {
        HlsPlaylist::Media(m) => m,
        HlsPlaylist::Master { mut variants, .. } => {
            let best = variants
                .drain(..)
                .next()
                .ok_or_else(|| anyhow!("Empty HLS master playlist"))?;
            let sub_text = fetch_text(client, &best.uri, headers).await?;
            match parse_m3u8(&sub_text, &best.uri)? {
                HlsPlaylist::Media(m) => m,
                _ => return Err(anyhow!("Nested HLS master playlist unsupported")),
            }
        }
    };

    if media.segments.is_empty() {
        return Err(anyhow!("HLS media playlist has 0 segments"));
    }

    let mut urls: Vec<String> = Vec::with_capacity(media.segments.len() + 1);
    if let Some(init_uri) = media.init_map_uri {
        urls.push(init_uri);
    }
    for seg in media.segments {
        urls.push(seg.uri);
    }

    download_urls_parallel_concat(client, &urls, output_path, concurrency, headers).await?;
    Ok(output_path.to_path_buf())
}

/// Shared bounded-parallel segment downloader & ordered concatenator (reused by HLS & DASH).
pub async fn download_urls_parallel_concat(
    client: &Client,
    urls: &[String],
    output_path: &Path,
    concurrency: usize,
    headers: &[(String, String)],
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let sem = Arc::new(Semaphore::new(concurrency.clamp(1, 16)));
    let hdrs = Arc::new(build_header_map(headers));

    let tasks = urls.iter().enumerate().map(|(idx, seg_url)| {
        let client = client.clone();
        let sem = sem.clone();
        let hdrs = hdrs.clone();
        let url = seg_url.clone();
        async move {
            let _permit = sem.acquire().await.map_err(|e| anyhow!(e.to_string()))?;
            let resp = client.get(&url).headers((*hdrs).clone()).send().await?;
            if !resp.status().is_success() {
                return Err(anyhow!("Segment {} HTTP {} for {}", idx, resp.status(), url));
            }
            let bytes = resp.bytes().await?;
            Ok::<(usize, Vec<u8>), anyhow::Error>((idx, bytes.to_vec()))
        }
    });

    let mut results = try_join_all(tasks).await?;
    results.sort_by_key(|(idx, _)| *idx);

    use std::io::Write;
    let mut out_file = std::fs::File::create(output_path)?;
    for (_, chunk) in results {
        out_file.write_all(&chunk)?;
    }
    out_file.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hls_master_and_media_playlists() {
        let master = r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aac",LANGUAGE="en",NAME="English",URI="audio/eng.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=1500000,RESOLUTION=1280x720,CODECS="avc1.4d401f,mp4a.40.2",AUDIO="aac"
720p/stream.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=3000000,RESOLUTION=1920x1080,CODECS="avc1.640028,mp4a.40.2",AUDIO="aac"
1080p/stream.m3u8
"#;

        match parse_m3u8(master, "https://cdn.example.com/live/master.m3u8").unwrap() {
            HlsPlaylist::Master {
                variants,
                audio_tracks,
            } => {
                assert_eq!(variants.len(), 2);
                assert_eq!(variants[0].resolution.as_deref(), Some("1920x1080"));
                assert_eq!(
                    variants[0].uri,
                    "https://cdn.example.com/live/1080p/stream.m3u8"
                );
                assert_eq!(audio_tracks.len(), 1);
                assert_eq!(
                    audio_tracks[0].uri,
                    "https://cdn.example.com/live/audio/eng.m3u8"
                );
            }
            _ => panic!("expected Master playlist"),
        }

        let media = r#"#EXTM3U
#EXT-X-TARGETDURATION:4
#EXT-X-MAP:URI="init.mp4"
#EXTINF:4.0,
seg0.m4s
#EXTINF:3.5,
seg1.m4s
#EXT-X-ENDLIST
"#;

        match parse_m3u8(media, "https://cdn.example.com/live/1080p/stream.m3u8").unwrap() {
            HlsPlaylist::Media(m) => {
                assert_eq!(m.target_duration, Some(4.0));
                assert_eq!(
                    m.init_map_uri.as_deref(),
                    Some("https://cdn.example.com/live/1080p/init.mp4")
                );
                assert_eq!(m.segments.len(), 2);
                assert_eq!(
                    m.segments[1].uri,
                    "https://cdn.example.com/live/1080p/seg1.m4s"
                );
            }
            _ => panic!("expected Media playlist"),
        }
    }
}
