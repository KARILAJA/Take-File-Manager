use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::storage::db::DownloadRecord;

const MAX_EXPANDED_URLS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportEntry {
    pub url: String,
    pub file_name: Option<String>,
    pub save_dir: Option<String>,
    pub category_id: Option<String>,
    pub queue_id: Option<String>,
    pub segments_cfg: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportEntry {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub save_dir: String,
    pub status: String,
    pub total_bytes: Option<u64>,
    pub downloaded: u64,
    pub category_id: Option<String>,
    pub queue_id: Option<String>,
    pub segments_cfg: u32,
}

pub fn is_supported_url(url: &str) -> bool {
    let lower = url.trim().to_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("ftp://")
        || lower.starts_with("ftps://")
}

/// Expand batch URL patterns supporting `[001-100]`, `[1-10:2]`, `[a-z]`, and `{a,b,c}`.
pub fn expand_url_pattern(pattern: &str) -> Result<Vec<String>> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut acc = Vec::new();
    expand_recursive(trimmed, &mut acc)?;
    if !acc.iter().all(|u| is_supported_url(u)) {
        return Err(anyhow!(
            "Expanded URL must start with http://, https://, ftp://, or ftps://"
        ));
    }
    Ok(acc)
}

fn expand_recursive(input: &str, out: &mut Vec<String>) -> Result<()> {
    if out.len() >= MAX_EXPANDED_URLS {
        return Err(anyhow!(
            "Pattern expansion exceeded limit of {} URLs",
            MAX_EXPANDED_URLS
        ));
    }

    // Find earliest '[' or '{' token that has a valid closing bracket
    let next_bracket = find_next_token(input);
    let Some((start, end, choices)) = next_bracket else {
        out.push(input.to_string());
        return Ok(());
    };

    let prefix = &input[..start];
    let suffix = &input[end + 1..];

    if out.len() + choices.len() > MAX_EXPANDED_URLS {
        return Err(anyhow!(
            "Pattern expansion exceeded limit of {} URLs",
            MAX_EXPANDED_URLS
        ));
    }

    let mut suffix_expanded = Vec::new();
    expand_recursive(suffix, &mut suffix_expanded)?;

    for choice in choices {
        for s in &suffix_expanded {
            if out.len() >= MAX_EXPANDED_URLS {
                return Err(anyhow!(
                    "Pattern expansion exceeded limit of {} URLs",
                    MAX_EXPANDED_URLS
                ));
            }
            out.push(format!("{}{}{}", prefix, choice, s));
        }
    }

    Ok(())
}

fn find_next_token(s: &str) -> Option<(usize, usize, Vec<String>)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(rel_end) = s[i + 1..].find(']') {
                let end = i + 1 + rel_end;
                let inner = &s[i + 1..end];
                if let Some(choices) = parse_range_token(inner) {
                    return Some((i, end, choices));
                }
            }
        } else if bytes[i] == b'{' {
            if let Some(rel_end) = s[i + 1..].find('}') {
                let end = i + 1 + rel_end;
                let inner = &s[i + 1..end];
                if inner.contains(',') || !inner.is_empty() {
                    let choices: Vec<String> = inner.split(',').map(|x| x.trim().to_string()).collect();
                    return Some((i, end, choices));
                }
            }
        }
        i += 1;
    }
    None
}

fn parse_range_token(inner: &str) -> Option<Vec<String>> {
    // Support optional :step suffix e.g. "001-010:2"
    let (range_part, step) = if let Some((r, st)) = inner.split_once(':') {
        let parsed_step = st.trim().parse::<usize>().ok()?;
        if parsed_step == 0 {
            return None;
        }
        (r, parsed_step)
    } else {
        (inner, 1usize)
    };

    let (start_str, end_str) = range_part.split_once('-')?;
    let start_trim = start_str.trim();
    let end_trim = end_str.trim();

    // Try numeric range first
    if let (Ok(start_num), Ok(end_num)) = (start_trim.parse::<i64>(), end_trim.parse::<i64>()) {
        if start_num < 0 || end_num < 0 {
            return None;
        }
        let width = if (start_trim.starts_with('0') && start_trim.len() > 1)
            || (end_trim.starts_with('0') && end_trim.len() > 1)
        {
            start_trim.len().max(end_trim.len())
        } else {
            0
        };

        let mut res = Vec::new();
        if start_num <= end_num {
            let mut cur = start_num;
            while cur <= end_num {
                if res.len() >= MAX_EXPANDED_URLS {
                    break;
                }
                if width > 0 {
                    res.push(format!("{:0width$}", cur, width = width));
                } else {
                    res.push(cur.to_string());
                }
                cur += step as i64;
            }
        } else {
            let mut cur = start_num;
            while cur >= end_num {
                if res.len() >= MAX_EXPANDED_URLS {
                    break;
                }
                if width > 0 {
                    res.push(format!("{:0width$}", cur, width = width));
                } else {
                    res.push(cur.to_string());
                }
                cur -= step as i64;
            }
        }
        return Some(res);
    }

    // Try single ASCII character range e.g. a-z or A-F
    if start_trim.len() == 1 && end_trim.len() == 1 {
        let s_ch = start_trim.as_bytes()[0];
        let e_ch = end_trim.as_bytes()[0];
        if (s_ch.is_ascii_lowercase() && e_ch.is_ascii_lowercase())
            || (s_ch.is_ascii_uppercase() && e_ch.is_ascii_uppercase())
        {
            let mut res = Vec::new();
            if s_ch <= e_ch {
                let mut cur = s_ch;
                while cur <= e_ch {
                    res.push((cur as char).to_string());
                    cur = cur.saturating_add(step as u8);
                    if cur == u8::MAX {
                        break;
                    }
                }
            } else {
                let mut cur = s_ch;
                while cur >= e_ch {
                    res.push((cur as char).to_string());
                    if cur < e_ch + step as u8 {
                        break;
                    }
                    cur -= step as u8;
                }
            }
            return Some(res);
        }
    }

    None
}

/// Parse newline-separated pasted URLs, deduplicated and validated.
pub fn parse_pasted_urls(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut urls = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if is_supported_url(trimmed) && seen.insert(trimmed.to_string()) {
            urls.push(trimmed.to_string());
        }
    }
    urls
}

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn parse_csv_row(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(c);
            }
        } else if c == '"' && current.is_empty() {
            in_quotes = true;
        } else if c == ',' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    fields.push(current);
    fields
}

/// Export download records into `txt`, `csv`, or `json`.
pub fn export_downloads(records: &[DownloadRecord], format: &str) -> Result<String> {
    match format.trim().to_lowercase().as_str() {
        "txt" | "text" => Ok(records
            .iter()
            .map(|r| r.url.as_str())
            .collect::<Vec<_>>()
            .join("\n")),
        "csv" => {
            let mut out = String::from("id,url,file_name,save_dir,status,total_bytes,downloaded,category_id\n");
            for r in records {
                let row = [
                    csv_escape(&r.id),
                    csv_escape(&r.url),
                    csv_escape(&r.file_name),
                    csv_escape(&r.save_dir),
                    csv_escape(&r.status.to_string()),
                    r.total_bytes.map(|v| v.to_string()).unwrap_or_default(),
                    r.downloaded.to_string(),
                    csv_escape(r.category_id.as_deref().unwrap_or("")),
                ]
                .join(",");
                out.push_str(&row);
                out.push('\n');
            }
            Ok(out)
        }
        "json" => {
            let entries: Vec<ExportEntry> = records
                .iter()
                .map(|r| ExportEntry {
                    id: r.id.clone(),
                    url: r.url.clone(),
                    file_name: r.file_name.clone(),
                    save_dir: r.save_dir.clone(),
                    status: r.status.to_string(),
                    total_bytes: r.total_bytes,
                    downloaded: r.downloaded,
                    category_id: r.category_id.clone(),
                    queue_id: r.queue_id.clone(),
                    segments_cfg: r.segments_cfg,
                })
                .collect();
            Ok(serde_json::to_string_pretty(&entries)?)
        }
        other => Err(anyhow!("Unsupported export format '{}'", other)),
    }
}

/// Import download list from `txt`, `csv`, or `json` string.
pub fn import_downloads(content: &str, format: &str) -> Result<Vec<ImportEntry>> {
    match format.trim().to_lowercase().as_str() {
        "txt" | "text" => Ok(parse_pasted_urls(content)
            .into_iter()
            .map(|url| ImportEntry {
                url,
                file_name: None,
                save_dir: None,
                category_id: None,
                queue_id: None,
                segments_cfg: None,
            })
            .collect()),
        "csv" => {
            let mut lines = content.lines().filter(|l| !l.trim().is_empty());
            let header_line = match lines.next() {
                Some(h) => h,
                None => return Ok(Vec::new()),
            };
            let headers: Vec<String> = parse_csv_row(header_line)
                .into_iter()
                .map(|h| h.trim().to_lowercase())
                .collect();

            let url_idx = headers.iter().position(|h| h == "url");
            let name_idx = headers.iter().position(|h| h == "file_name" || h == "filename");
            let dir_idx = headers.iter().position(|h| h == "save_dir" || h == "dir");
            let cat_idx = headers.iter().position(|h| h == "category_id" || h == "category");

            let mut entries = Vec::new();
            // If no 'url' header column, treat header_line itself as raw row col 0 if supported URL
            if url_idx.is_none() {
                let first_cols = parse_csv_row(header_line);
                if let Some(u) = first_cols.first().filter(|u| is_supported_url(u)) {
                    entries.push(ImportEntry {
                        url: u.trim().to_string(),
                        file_name: first_cols.get(1).filter(|s| !s.is_empty()).cloned(),
                        save_dir: None,
                        category_id: None,
                        queue_id: None,
                        segments_cfg: None,
                    });
                }
                for line in lines {
                    let cols = parse_csv_row(line);
                    if let Some(u) = cols.first().filter(|u| is_supported_url(u)) {
                        entries.push(ImportEntry {
                            url: u.trim().to_string(),
                            file_name: cols.get(1).filter(|s| !s.is_empty()).cloned(),
                            save_dir: None,
                            category_id: None,
                            queue_id: None,
                            segments_cfg: None,
                        });
                    }
                }
                return Ok(entries);
            }

            let u_idx = url_idx.unwrap();
            for line in lines {
                let cols = parse_csv_row(line);
                let Some(url) = cols.get(u_idx).map(|s| s.trim().to_string()) else {
                    continue;
                };
                if !is_supported_url(&url) {
                    continue;
                }
                entries.push(ImportEntry {
                    url,
                    file_name: name_idx
                        .and_then(|i| cols.get(i))
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    save_dir: dir_idx
                        .and_then(|i| cols.get(i))
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    category_id: cat_idx
                        .and_then(|i| cols.get(i))
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    queue_id: None,
                    segments_cfg: None,
                });
            }
            Ok(entries)
        }
        "json" => {
            let parsed: serde_json::Value = serde_json::from_str(content)?;
            let arr = parsed
                .as_array()
                .ok_or_else(|| anyhow!("JSON import must be an array"))?;
            let mut entries = Vec::new();
            for item in arr {
                if let Some(url_str) = item.as_str() {
                    if is_supported_url(url_str) {
                        entries.push(ImportEntry {
                            url: url_str.to_string(),
                            file_name: None,
                            save_dir: None,
                            category_id: None,
                            queue_id: None,
                            segments_cfg: None,
                        });
                    }
                } else if let Some(obj) = item.as_object() {
                    let Some(url) = obj.get("url").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if !is_supported_url(url) {
                        continue;
                    }
                    entries.push(ImportEntry {
                        url: url.to_string(),
                        file_name: obj
                            .get("file_name")
                            .or_else(|| obj.get("fileName"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        save_dir: obj
                            .get("save_dir")
                            .or_else(|| obj.get("saveDir"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        category_id: obj
                            .get("category_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        queue_id: obj
                            .get("queue_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        segments_cfg: obj
                            .get("segments_cfg")
                            .and_then(|v| v.as_u64())
                            .map(|n| n as u32),
                    });
                }
            }
            Ok(entries)
        }
        other => Err(anyhow!("Unsupported import format '{}'", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_numeric_zero_padded_range() {
        let urls = expand_url_pattern("http://x.com/img[001-005].jpg").unwrap();
        assert_eq!(
            urls,
            vec![
                "http://x.com/img001.jpg",
                "http://x.com/img002.jpg",
                "http://x.com/img003.jpg",
                "http://x.com/img004.jpg",
                "http://x.com/img005.jpg",
            ]
        );
    }

    #[test]
    fn test_expand_braces_and_stepped_range() {
        let urls = expand_url_pattern("https://cdn.example.com/{a,b,c}/part_[02-06:2].zip").unwrap();
        assert_eq!(
            urls,
            vec![
                "https://cdn.example.com/a/part_02.zip",
                "https://cdn.example.com/a/part_04.zip",
                "https://cdn.example.com/a/part_06.zip",
                "https://cdn.example.com/b/part_02.zip",
                "https://cdn.example.com/b/part_04.zip",
                "https://cdn.example.com/b/part_06.zip",
                "https://cdn.example.com/c/part_02.zip",
                "https://cdn.example.com/c/part_04.zip",
                "https://cdn.example.com/c/part_06.zip",
            ]
        );
    }

    #[test]
    fn test_expand_alpha_range() {
        let urls = expand_url_pattern("https://x.com/vol_[a-c].iso").unwrap();
        assert_eq!(
            urls,
            vec![
                "https://x.com/vol_a.iso",
                "https://x.com/vol_b.iso",
                "https://x.com/vol_c.iso",
            ]
        );
    }

    #[test]
    fn test_paste_multiple_urls_dedup_and_filter() {
        let raw = "\n# comment line\nhttps://example.com/1.zip\nftp://files.example.org/2.tar.gz\njavascript:alert(1)\nhttps://example.com/1.zip\n";
        let urls = parse_pasted_urls(raw);
        assert_eq!(
            urls,
            vec![
                "https://example.com/1.zip",
                "ftp://files.example.org/2.tar.gz",
            ]
        );
    }
}
