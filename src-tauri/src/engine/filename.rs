use std::path::Path;

/// Sanitize a filename to prevent path traversal and invalid OS characters
pub fn sanitize_filename(name: &str) -> String {
    let mut cleaned = name.trim().to_string();

    // Replace invalid Windows characters: <>:"/\|?*
    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    cleaned = cleaned
        .chars()
        .map(|c| {
            if invalid_chars.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();

    // Remove leading and trailing dots or spaces
    cleaned = cleaned.trim_matches(|c| c == '.' || c == ' ').to_string();

    // Check Windows reserved device names
    let reserved_names = [
        "CON", "PRN", "AUX", "NUL",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];

    let stem = cleaned.split('.').next().unwrap_or("").to_uppercase();
    if reserved_names.contains(&stem.as_str()) {
        cleaned = format!("_{cleaned}");
    }

    if cleaned.is_empty() {
        "download.bin".to_string()
    } else {
        cleaned
    }
}

/// Extract filename from Content-Disposition header, or URL path, or fallback
pub fn extract_filename(content_disposition: Option<&str>, final_url: &str) -> String {
    if let Some(cd) = content_disposition {
        if let Some(name) = parse_content_disposition(cd) {
            let sanitized = sanitize_filename(&name);
            if !sanitized.is_empty() && sanitized != "download.bin" {
                return sanitized;
            }
        }
    }

    // Fallback to URL path
    if let Ok(url) = reqwest::Url::parse(final_url) {
        if let Some(mut segments) = url.path_segments() {
            if let Some(last) = segments.rfind(|s| !s.is_empty()) {
                let decoded = percent_encoding::percent_decode_str(last)
                    .decode_utf8_lossy()
                    .to_string();
                let sanitized = sanitize_filename(&decoded);
                if !sanitized.is_empty() && sanitized != "download.bin" {
                    return sanitized;
                }
            }
        }
    }

    "download.bin".to_string()
}

fn parse_content_disposition(header: &str) -> Option<String> {
    // Check filename*=UTF-8''... (RFC 5987 / RFC 6266)
    for part in header.split(';') {
        let trimmed = part.trim();
        if let Some(encoded) = trimmed.strip_prefix("filename*=") {
            let encoded = encoded.trim().trim_matches('"');
            if let Some(rest) = encoded.strip_prefix("UTF-8''").or_else(|| encoded.strip_prefix("utf-8''")) {
                if let Ok(decoded) = percent_encoding::percent_decode_str(rest).decode_utf8() {
                    return Some(
                        Path::new(&decoded.to_string())
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| decoded.to_string()),
                    );
                }
            }
        }
    }

    // Check filename="..." or filename=...
    for part in header.split(';') {
        let trimmed = part.trim();
        if let Some(name) = trimmed.strip_prefix("filename=") {
            let name = name.trim().trim_matches('"');
            if !name.is_empty() {
                return Some(
                    Path::new(name)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| name.to_string()),
                );
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("valid_name.zip"), "valid_name.zip");
        assert_eq!(sanitize_filename("folder/traversal/name.txt"), "folder_traversal_name.txt");
        assert_eq!(sanitize_filename("bad:char*<?>|.pdf"), "bad_char_____.pdf");
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename(""), "download.bin");
        assert_eq!(sanitize_filename("..."), "download.bin");
    }

    #[test]
    fn test_extract_filename() {
        let cd1 = r#"attachment; filename="setup_v1.0.exe""#;
        assert_eq!(extract_filename(Some(cd1), "https://example.com/download"), "setup_v1.0.exe");

        let cd2 = "attachment; filename*=UTF-8''data%202026.csv";
        assert_eq!(extract_filename(Some(cd2), "https://example.com/download"), "data 2026.csv");

        let url = "https://example.com/files/archive-v2.tar.gz?auth=123";
        assert_eq!(extract_filename(None, url), "archive-v2.tar.gz");
    }
}
