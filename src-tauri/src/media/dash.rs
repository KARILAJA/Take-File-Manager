use anyhow::{anyhow, Result};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::hls::download_urls_parallel_concat;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashRepresentation {
    pub id: String,
    pub content_type: String, // "video" | "audio" | "unknown"
    pub mime_type: String,
    pub bandwidth: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub codecs: Option<String>,
    pub init_url: Option<String>,
    pub segment_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashManifest {
    pub representations: Vec<DashRepresentation>,
}

fn extract_xml_attr(tag: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{}={}", name, quote);
        if let Some(pos) = tag.find(&needle) {
            let rest = &tag[pos + needle.len()..];
            if let Some(end) = rest.find(quote) {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

fn extract_tag_text(block: &str, tag_name: &str) -> Option<String> {
    let open = format!("<{}", tag_name);
    let close = format!("</{}>", tag_name);
    let start_pos = block.find(&open)?;
    let after_open = &block[start_pos..];
    let gt_offset = after_open.find('>')?;
    let content_start = start_pos + gt_offset + 1;
    let end_pos = block[content_start..].find(&close)? + content_start;
    Some(block[content_start..end_pos].trim().to_string())
}

fn split_elements<'a>(xml: &'a str, tag_name: &str) -> Vec<&'a str> {
    let mut blocks = Vec::new();
    let open = format!("<{}", tag_name);
    let close = format!("</{}>", tag_name);
    let mut cursor = 0usize;

    while let Some(rel_start) = xml[cursor..].find(&open) {
        let start = cursor + rel_start;
        // Make sure tag name ends at whitespace or '>'
        let after_name = start + open.len();
        if let Some(ch) = xml[after_name..].chars().next() {
            if ch != ' ' && ch != '>' && ch != '\n' && ch != '\r' && ch != '\t' {
                cursor = after_name;
                continue;
            }
        }
        if let Some(rel_end) = xml[start..].find(&close) {
            let end = start + rel_end + close.len();
            blocks.push(&xml[start..end]);
            cursor = end;
        } else {
            break;
        }
    }
    blocks
}

fn join_url(base: &Url, rel: &str) -> Result<String> {
    let trimmed = rel.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed.to_string())
    } else {
        Ok(base.join(trimmed)?.to_string())
    }
}

fn expand_template(tpl: &str, rep_id: &str, number: Option<u64>, time: Option<u64>) -> String {
    let mut out = tpl.replace("$RepresentationID$", rep_id);
    if let Some(n) = number {
        out = out.replace("$Number$", &n.to_string());
    }
    if let Some(t) = time {
        out = out.replace("$Time$", &t.to_string());
    }
    out
}

fn parse_segment_template(
    tpl_block: &str,
    base: &Url,
    rep_id: &str,
) -> Result<(Option<String>, Vec<String>)> {
    let init_url = extract_xml_attr(tpl_block, "initialization")
        .map(|s| join_url(base, &expand_template(&s, rep_id, None, None)))
        .transpose()?;

    let media_tpl = match extract_xml_attr(tpl_block, "media") {
        Some(m) => m,
        None => return Ok((init_url, Vec::new())),
    };

    let start_number = extract_xml_attr(tpl_block, "startNumber")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1);

    let mut seg_urls = Vec::new();
    // Handle <SegmentTimeline> with <S t="..." d="..." r="..."/>
    let mut cursor = 0usize;
    let mut current_time = 0u64;
    let mut current_num = start_number;

    while let Some(rel_s) = tpl_block[cursor..].find("<S ") {
        let s_start = cursor + rel_s;
        let s_end = match tpl_block[s_start..].find('>') {
            Some(e) => s_start + e + 1,
            None => break,
        };
        let s_tag = &tpl_block[s_start..s_end];
        if let Some(t) = extract_xml_attr(s_tag, "t").and_then(|v| v.parse::<u64>().ok()) {
            current_time = t;
        }
        let d = extract_xml_attr(s_tag, "d")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let repeat = extract_xml_attr(s_tag, "r")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        for _ in 0..=repeat {
            let expanded = expand_template(&media_tpl, rep_id, Some(current_num), Some(current_time));
            seg_urls.push(join_url(base, &expanded)?);
            current_num += 1;
            current_time += d;
        }
        cursor = s_end;
    }

    Ok((init_url, seg_urls))
}

fn parse_segment_list(list_block: &str, base: &Url) -> Result<(Option<String>, Vec<String>)> {
    let mut init_url = None;
    if let Some(init_pos) = list_block.find("<Initialization") {
        if let Some(end) = list_block[init_pos..].find('>') {
            let init_tag = &list_block[init_pos..init_pos + end + 1];
            if let Some(src) = extract_xml_attr(init_tag, "sourceURL") {
                init_url = Some(join_url(base, &src)?);
            }
        }
    }

    let mut seg_urls = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel_pos) = list_block[cursor..].find("<SegmentURL") {
        let s_start = cursor + rel_pos;
        let s_end = match list_block[s_start..].find('>') {
            Some(e) => s_start + e + 1,
            None => break,
        };
        let tag = &list_block[s_start..s_end];
        if let Some(media) = extract_xml_attr(tag, "media") {
            seg_urls.push(join_url(base, &media)?);
        }
        cursor = s_end;
    }

    Ok((init_url, seg_urls))
}

/// Parse an MPEG-DASH `.mpd` manifest into AdaptationSet representations and segment URLs
pub fn parse_mpd(xml: &str, base_url: &str) -> Result<DashManifest> {
    let mpd_base = Url::parse(base_url)?;
    let mut representations = Vec::new();

    for adapt_block in split_elements(xml, "AdaptationSet") {
        let adapt_header = adapt_block.split('>').next().unwrap_or("");
        let adapt_mime = extract_xml_attr(adapt_header, "mimeType").unwrap_or_default();
        let adapt_ctype = extract_xml_attr(adapt_header, "contentType").unwrap_or_default();

        // Check AdaptationSet-level SegmentTemplate
        let adapt_tpl_block = split_elements(adapt_block, "SegmentTemplate")
            .into_iter()
            .next()
            .filter(|tpl| !adapt_block[..adapt_block.find(tpl).unwrap_or(0)].contains("<Representation"));

        for rep_block in split_elements(adapt_block, "Representation") {
            let rep_header = rep_block.split('>').next().unwrap_or("");
            let id = extract_xml_attr(rep_header, "id").unwrap_or_else(|| "0".to_string());
            let mime_type = extract_xml_attr(rep_header, "mimeType").unwrap_or_else(|| adapt_mime.clone());
            let bandwidth = extract_xml_attr(rep_header, "bandwidth")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let width = extract_xml_attr(rep_header, "width").and_then(|v| v.parse::<u32>().ok());
            let height = extract_xml_attr(rep_header, "height").and_then(|v| v.parse::<u32>().ok());
            let codecs = extract_xml_attr(rep_header, "codecs");

            let content_type = if !adapt_ctype.is_empty() {
                adapt_ctype.clone()
            } else if mime_type.starts_with("video/") {
                "video".to_string()
            } else if mime_type.starts_with("audio/") {
                "audio".to_string()
            } else {
                "unknown".to_string()
            };

            let rep_base_url = extract_tag_text(rep_block, "BaseURL")
                .map(|b| join_url(&mpd_base, &b))
                .transpose()?;

            let (init_url, segment_urls) = if let Some(seg_list) = split_elements(rep_block, "SegmentList").into_iter().next() {
                let effective_base = rep_base_url
                    .as_deref()
                    .and_then(|u| Url::parse(u).ok())
                    .unwrap_or_else(|| mpd_base.clone());
                parse_segment_list(seg_list, &effective_base)?
            } else if let Some(rep_tpl) = split_elements(rep_block, "SegmentTemplate").into_iter().next().or(adapt_tpl_block) {
                parse_segment_template(rep_tpl, &mpd_base, &id)?
            } else if let Some(single_base) = rep_base_url {
                (None, vec![single_base])
            } else {
                (None, Vec::new())
            };

            representations.push(DashRepresentation {
                id,
                content_type,
                mime_type,
                bandwidth,
                width,
                height,
                codecs,
                init_url,
                segment_urls,
            });
        }
    }

    Ok(DashManifest { representations })
}

/// Download a DASH representation (init + segments) concurrently and write into `output_path`
pub async fn download_dash_representation(
    client: &Client,
    rep: &DashRepresentation,
    output_path: &Path,
    concurrency: usize,
    headers: &[(String, String)],
) -> Result<PathBuf> {
    let mut urls = Vec::with_capacity(rep.segment_urls.len() + 1);
    if let Some(ref init) = rep.init_url {
        urls.push(init.clone());
    }
    urls.extend(rep.segment_urls.iter().cloned());

    if urls.is_empty() {
        return Err(anyhow!("DASH representation {} has no segment URLs", rep.id));
    }

    download_urls_parallel_concat(client, &urls, output_path, concurrency, headers).await?;
    Ok(output_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mpd_segment_list_and_template() {
        let mpd_list = r#"<?xml version="1.0" encoding="utf-8"?>
<MPD>
  <Period>
    <AdaptationSet mimeType="video/mp4" contentType="video">
      <Representation id="v720" bandwidth="1200000" width="1280" height="720" codecs="avc1.4d401f">
        <SegmentList duration="2">
          <Initialization sourceURL="video/init.mp4"/>
          <SegmentURL media="video/seg1.m4s"/>
          <SegmentURL media="video/seg2.m4s"/>
        </SegmentList>
      </Representation>
    </AdaptationSet>
    <AdaptationSet mimeType="audio/mp4" contentType="audio">
      <SegmentTemplate initialization="$RepresentationID$/init.m4s" media="$RepresentationID$/chunk_$Number$.m4s" startNumber="1">
        <SegmentTimeline>
          <S t="0" d="1000" r="1"/>
        </SegmentTimeline>
      </SegmentTemplate>
      <Representation id="a128" bandwidth="128000" codecs="mp4a.40.2"></Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;

        let manifest = parse_mpd(mpd_list, "https://cdn.example.com/vod/manifest.mpd").unwrap();
        assert_eq!(manifest.representations.len(), 2);

        let v = &manifest.representations[0];
        assert_eq!(v.content_type, "video");
        assert_eq!(v.height, Some(720));
        assert_eq!(
            v.init_url.as_deref(),
            Some("https://cdn.example.com/vod/video/init.mp4")
        );
        assert_eq!(
            v.segment_urls,
            vec![
                "https://cdn.example.com/vod/video/seg1.m4s",
                "https://cdn.example.com/vod/video/seg2.m4s"
            ]
        );

        let a = &manifest.representations[1];
        assert_eq!(a.content_type, "audio");
        assert_eq!(
            a.init_url.as_deref(),
            Some("https://cdn.example.com/vod/a128/init.m4s")
        );
        assert_eq!(
            a.segment_urls,
            vec![
                "https://cdn.example.com/vod/a128/chunk_1.m4s",
                "https://cdn.example.com/vod/a128/chunk_2.m4s"
            ]
        );
    }
}
