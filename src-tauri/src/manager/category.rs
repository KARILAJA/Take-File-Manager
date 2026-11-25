use crate::storage::db::CategoryRecord;

/// Determine category ID based on filename and list of category rules
pub fn detect_category(file_name: &str, categories: &[CategoryRecord]) -> String {
    let lower_name = file_name.to_lowercase();
    let ext = match lower_name.rfind('.') {
        Some(idx) => &lower_name[idx + 1..],
        None => "",
    };

    if ext.is_empty() {
        return "others".to_string();
    }

    // Check custom and default categories
    for cat in categories {
        if cat.id == "others" || cat.extensions == "*" {
            continue;
        }

        let exts: Vec<&str> = cat
            .extensions
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if exts.iter().any(|&e| e.eq_ignore_ascii_case(ext)) {
            return cat.id.clone();
        }
    }

    // Default hardcoded fallback checks if category list is empty
    match ext {
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "tgz" => "compressed".to_string(),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt" | "epub" => {
            "documents".to_string()
        }
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" => "music".to_string(),
        "exe" | "msi" | "apk" | "dmg" | "pkg" | "deb" | "rpm" | "appimage" => "programs".to_string(),
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "ts" => "video".to_string(),
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "tiff" | "ico" => "images".to_string(),
        "torrent" => "torrent".to_string(),
        _ => "others".to_string(),
    }
}

/// Resolve destination save directory for a download:
/// If caller specified save_dir, use it. Otherwise, use matched category's save_dir.
pub fn resolve_save_dir(
    specified_save_dir: Option<&str>,
    file_name: &str,
    categories: &[CategoryRecord],
    fallback_dir: &str,
) -> (String, String) {
    let cat_id = detect_category(file_name, categories);

    if let Some(dir) = specified_save_dir {
        if !dir.trim().is_empty() {
            return (dir.to_string(), cat_id);
        }
    }

    if let Some(cat) = categories.iter().find(|c| c.id == cat_id) {
        if !cat.save_dir.trim().is_empty() {
            return (cat.save_dir.clone(), cat_id);
        }
    }

    (fallback_dir.to_string(), cat_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_category() {
        assert_eq!(detect_category("ubuntu-22.04.iso", &[]), "compressed");
        assert_eq!(detect_category("archive.tar.gz", &[]), "compressed");
        assert_eq!(detect_category("song.mp3", &[]), "music");
        assert_eq!(detect_category("report.pdf", &[]), "documents");
        assert_eq!(detect_category("setup.exe", &[]), "programs");
        assert_eq!(detect_category("movie.mkv", &[]), "video");
        assert_eq!(detect_category("photo.png", &[]), "images");
        assert_eq!(detect_category("ubuntu.torrent", &[]), "torrent");
        assert_eq!(detect_category("unknown.dat", &[]), "others");
    }
}
