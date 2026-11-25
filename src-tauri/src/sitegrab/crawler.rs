use anyhow::{anyhow, Result};
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::engine::filename::sanitize_filename;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlerConfig {
    pub start_url: String,
    pub max_depth: u32,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub include_subdomains: bool,
    #[serde(default)]
    pub file_extensions: Vec<String>,
    #[serde(default)]
    pub include_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    pub max_pages: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrawledItem {
    pub url: String,
    pub file_name: String,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    pub depth: u32,
    pub source_page: String,
}

fn is_domain_allowed(url: &Url, allowed_domains: &[String], include_subdomains: bool) -> bool {
    let Some(host) = url.host_str().map(|h| h.to_lowercase()) else {
        return false;
    };

    allowed_domains.iter().any(|dom| {
        let d = dom.trim().to_lowercase();
        if d.is_empty() {
            return false;
        }
        host == d || (include_subdomains && host.ends_with(&format!(".{}", d)))
    })
}

fn extract_extension(url: &Url) -> String {
    let path = url.path();
    let base = path.rsplit('/').next().unwrap_or("");
    if let Some(dot) = base.rfind('.') {
        if dot + 1 < base.len() {
            return base[dot + 1..].to_lowercase();
        }
    }
    String::new()
}

fn matches_wildcard(text: &str, pattern: &str) -> bool {
    let t = text.to_lowercase();
    let p = pattern.trim().to_lowercase();
    if p.is_empty() {
        return true;
    }
    if !p.contains('*') {
        return t.contains(&p);
    }
    let parts: Vec<&str> = p.split('*').collect();
    let mut pos = 0usize;
    for (idx, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if idx == 0 && !p.starts_with('*') {
            if !t.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if idx == parts.len() - 1 && !p.ends_with('*') {
            return t[pos..].ends_with(part);
        } else if let Some(found) = t[pos..].find(part) {
            pos += found + part.len();
        } else {
            return false;
        }
    }
    true
}

fn passes_filters(url_str: &str, url: &Url, cfg: &CrawlerConfig) -> bool {
    let ext = extract_extension(url);

    // Exclude patterns take precedence
    if cfg
        .exclude_patterns
        .iter()
        .any(|pat| matches_wildcard(url_str, pat))
    {
        return false;
    }

    // Include patterns (if provided, at least one must match)
    if !cfg.include_patterns.is_empty()
        && !cfg
            .include_patterns
            .iter()
            .any(|pat| matches_wildcard(url_str, pat))
    {
        return false;
    }

    // File extension filter
    if !cfg.file_extensions.is_empty() {
        return cfg
            .file_extensions
            .iter()
            .any(|e| e.trim().trim_start_matches('.').eq_ignore_ascii_case(&ext));
    }

    // When no extension filter is specified, consider URLs with non-web extensions
    !ext.is_empty() && !matches!(ext.as_str(), "html" | "htm" | "php" | "asp" | "aspx" | "jsp")
}

/// Extract normalized HTTP/HTTPS links from HTML attributes (`href`, `src`).
pub fn extract_links(html: &str, page_url: &Url) -> Vec<String> {
    let lower = html.to_lowercase();
    let bytes = html.as_bytes();
    let mut links = Vec::new();
    let mut seen = HashSet::new();

    for attr in ["href=", "src="] {
        let mut cursor = 0usize;
        while let Some(rel) = lower[cursor..].find(attr) {
            let attr_start = cursor + rel;
            // Verify boundary before attr name
            if attr_start > 0 {
                let prev = bytes[attr_start - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-' {
                    cursor = attr_start + attr.len();
                    continue;
                }
            }

            let mut val_start = attr_start + attr.len();
            while val_start < bytes.len() && bytes[val_start].is_ascii_whitespace() {
                val_start += 1;
            }
            if val_start >= bytes.len() {
                break;
            }

            let raw_val = if bytes[val_start] == b'"' || bytes[val_start] == b'\'' {
                let quote = bytes[val_start];
                val_start += 1;
                let mut val_end = val_start;
                while val_end < bytes.len() && bytes[val_end] != quote {
                    val_end += 1;
                }
                cursor = (val_end + 1).min(bytes.len());
                &html[val_start..val_end]
            } else {
                let mut val_end = val_start;
                while val_end < bytes.len()
                    && !bytes[val_end].is_ascii_whitespace()
                    && bytes[val_end] != b'>'
                {
                    val_end += 1;
                }
                cursor = val_end;
                &html[val_start..val_end]
            };

            let unescaped = raw_val.trim().replace("&amp;", "&");
            if unescaped.is_empty()
                || unescaped.starts_with('#')
                || unescaped.to_lowercase().starts_with("javascript:")
                || unescaped.to_lowercase().starts_with("mailto:")
                || unescaped.to_lowercase().starts_with("data:")
            {
                continue;
            }

            if let Ok(mut joined) = page_url.join(&unescaped) {
                if joined.scheme() == "http" || joined.scheme() == "https" {
                    joined.set_fragment(None);
                    let normalized = joined.to_string();
                    if seen.insert(normalized.clone()) {
                        links.push(normalized);
                    }
                }
            }
        }
    }

    links
}

fn infer_file_name(url: &Url) -> String {
    if let Some(mut segs) = url.path_segments() {
        if let Some(last) = segs.rfind(|s| !s.is_empty()) {
            let clean = sanitize_filename(last);
            if !clean.is_empty() {
                return clean;
            }
        }
    }
    "download.bin".to_string()
}

/// Crawl site up to `cfg.max_depth`, strictly respecting domain boundary and file filters.
pub async fn crawl_site(
    client: &Client,
    mut cfg: CrawlerConfig,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<CrawledItem>> {
    let start_parsed = Url::parse(cfg.start_url.trim())?;
    if start_parsed.scheme() != "http" && start_parsed.scheme() != "https" {
        return Err(anyhow!("Crawler only supports http:// and https:// URLs"));
    }

    if cfg.allowed_domains.is_empty() {
        if let Some(host) = start_parsed.host_str() {
            cfg.allowed_domains.push(host.to_string());
        }
    }

    let max_pages = cfg.max_pages.unwrap_or(200);
    let mut visited_pages: HashSet<String> = HashSet::new();
    let mut collected_urls: HashSet<String> = HashSet::new();
    let mut results: Vec<CrawledItem> = Vec::new();
    let mut queue: VecDeque<(String, u32)> = VecDeque::new();

    let mut clean_start = start_parsed;
    clean_start.set_fragment(None);
    queue.push_back((clean_start.to_string(), 0));
    visited_pages.insert(clean_start.to_string());

    let mut pages_fetched = 0usize;

    while let Some((page_url_str, depth)) = queue.pop_front() {
        if cancel.load(Ordering::Relaxed) || pages_fetched >= max_pages || depth > cfg.max_depth {
            break;
        }

        let Ok(page_url) = Url::parse(&page_url_str) else {
            continue;
        };
        if !is_domain_allowed(&page_url, &cfg.allowed_domains, cfg.include_subdomains) {
            continue;
        }

        let resp = match client.get(&page_url_str).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => continue,
        };

        pages_fetched += 1;

        let ct = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let cl = resp.content_length();
        let is_html = ct
            .as_deref()
            .map(|c| c.to_lowercase().contains("text/html") || c.to_lowercase().contains("application/xhtml"))
            .unwrap_or(false);

        if !is_html {
            if passes_filters(&page_url_str, &page_url, &cfg)
                && collected_urls.insert(page_url_str.clone())
            {
                results.push(CrawledItem {
                    url: page_url_str,
                    file_name: infer_file_name(&page_url),
                    content_type: ct,
                    content_length: cl,
                    depth,
                    source_page: page_url.to_string(),
                });
            }
            continue;
        }

        let Ok(body) = resp.text().await else {
            continue;
        };

        let links = extract_links(&body, &page_url);
        for link_str in links {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let Ok(link_url) = Url::parse(&link_str) else {
                continue;
            };
            // Respect domain boundary for both crawled pages and collected links
            if !is_domain_allowed(&link_url, &cfg.allowed_domains, cfg.include_subdomains) {
                continue;
            }

            if passes_filters(&link_str, &link_url, &cfg) {
                if collected_urls.insert(link_str.clone()) {
                    results.push(CrawledItem {
                        url: link_str,
                        file_name: infer_file_name(&link_url),
                        content_type: None,
                        content_length: None,
                        depth,
                        source_page: page_url_str.clone(),
                    });
                }
                continue;
            }

            // Enqueue sub-page only if depth + 1 <= max_depth
            let next_depth = depth + 1;
            if next_depth <= cfg.max_depth && visited_pages.insert(link_str.clone()) {
                queue.push_back((link_str, next_depth));
            }
        }
    }

    Ok(results)
}
