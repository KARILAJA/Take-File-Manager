use anyhow::{anyhow, Result};
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH,
    CONTENT_RANGE, CONTENT_TYPE, ETAG, LAST_MODIFIED, RANGE,
};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::filename::extract_filename;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeInfo {
    pub url: String,
    pub final_url: String,
    pub file_name: String,
    pub total_bytes: Option<u64>,
    pub resumable: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
}

pub async fn probe_url(client: &Client, url: &str, custom_headers: &[(String, String)]) -> Result<ProbeInfo> {
    let mut headers = HeaderMap::new();
    for (k, v) in custom_headers {
        if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
            headers.insert(name, val);
        }
    }

    // First try HEAD
    let resp = client.head(url).headers(headers.clone()).send().await;

    let response = match resp {
        Ok(res) if res.status().is_success() || res.status() == StatusCode::PARTIAL_CONTENT => res,
        _ => {
            // Fallback to GET with Range: bytes=0-0
            let mut get_headers = headers.clone();
            get_headers.insert(RANGE, HeaderValue::from_static("bytes=0-0"));
            client
                .get(url)
                .headers(get_headers)
                .send()
                .await
                .map_err(|e| anyhow!("Failed to probe URL {}: {}", url, e))?
        }
    };

    let final_url = response.url().to_string();
    let resp_headers = response.headers();

    let etag = resp_headers
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim_matches('"').to_string());

    let last_modified = resp_headers
        .get(LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let content_type = resp_headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let cd = resp_headers
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok());

    let file_name = extract_filename(cd, &final_url);

    let mut total_bytes = response.content_length();
    if total_bytes.is_none() || total_bytes == Some(0) {
        if let Some(cl) = resp_headers.get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()) {
            if let Ok(b) = cl.parse::<u64>() {
                if b > 0 {
                    total_bytes = Some(b);
                }
            }
        }
    }

    let mut resumable = false;

    if let Some(cr) = resp_headers.get(CONTENT_RANGE).and_then(|v| v.to_str().ok()) {
        // Format: "bytes 0-0/12345"
        if let Some(slash_idx) = cr.rfind('/') {
            let total_str = &cr[slash_idx + 1..];
            if let Ok(bytes) = total_str.parse::<u64>() {
                total_bytes = Some(bytes);
                resumable = true;
            }
        }
    }

    if let Some(ar) = resp_headers.get(ACCEPT_RANGES).and_then(|v| v.to_str().ok()) {
        if ar.to_lowercase().contains("bytes") {
            resumable = true;
        }
    }

    if response.status() == StatusCode::PARTIAL_CONTENT {
        resumable = true;
    }

    Ok(ProbeInfo {
        url: url.to_string(),
        final_url,
        file_name,
        total_bytes,
        resumable,
        etag,
        last_modified,
        content_type,
    })
}
