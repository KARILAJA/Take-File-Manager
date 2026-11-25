use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const FIXED_TFM_EXTENSION_ID: &str = "mbikddgphpokomgjlnkgipejppbjileo";
pub const FIREFOX_TFM_EXTENSION_ID: &str = "takefilemanager@turboget.app";

pub fn build_firefox_native_manifest_json(host_exe: &Path) -> Value {
    serde_json::json!({
        "name": "com.turboget.host",
        "description": "Take File Manager Native Messaging Host",
        "path": host_exe.to_string_lossy().to_string(),
        "type": "stdio",
        "allowed_extensions": [FIREFOX_TFM_EXTENSION_ID]
    })
}

pub fn extract_matching_ids_from_preferences_json(raw: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let Ok(root) = serde_json::from_str::<Value>(raw) else {
        return ids;
    };
    let Some(settings) = root
        .get("extensions")
        .and_then(|e| e.get("settings"))
        .and_then(|s| s.as_object())
    else {
        return ids;
    };

    for (id, entry) in settings {
        let name = entry
            .get("manifest")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("");
        let path = entry.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let path_low = path.to_ascii_lowercase().replace('/', "\\");
        let name_low = name.to_ascii_lowercase();

        let matches = name_low.contains("take file manager")
            || name_low.contains("turboget")
            || path_low.ends_with("\\idm\\extension")
            || (path_low.contains("take file manager") && path_low.ends_with("\\extension"));

        if matches && id.len() == 32 && id.bytes().all(|b| b.is_ascii_lowercase()) {
            ids.push(id.clone());
        }
    }
    ids
}

#[cfg(windows)]
fn resolve_host_exe_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("turboget-host.exe");
            if sibling.exists() {
                return Some(sibling);
            }
            // Dev/repo layout fallback: <repo>\src-tauri\target\{debug,release}\turboget.exe
            if let Some(repo) = dir
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
            {
                let dev_rel = repo.join("native-host\\target\\release\\turboget-host.exe");
                if dev_rel.exists() {
                    return Some(dev_rel);
                }
                let dev_dbg = repo.join("native-host\\target\\debug\\turboget-host.exe");
                if dev_dbg.exists() {
                    return Some(dev_dbg);
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn collect_browser_extension_ids() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    ids.insert(FIXED_TFM_EXTENSION_ID.to_string());

    let mut roots = Vec::new();
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        roots.push(Path::new(&local_app_data).join("Google\\Chrome\\User Data"));
        roots.push(Path::new(&local_app_data).join("Microsoft\\Edge\\User Data"));
        roots.push(Path::new(&local_app_data).join("BraveSoftware\\Brave-Browser\\User Data"));
    }
    if let Ok(app_data) = std::env::var("APPDATA") {
        // Opera stores Secure Preferences / Preferences directly inside the profile dir
        roots.push(Path::new(&app_data).join("Opera Software"));
    }

    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let prof_dir = entry.path();
            if !prof_dir.is_dir() {
                continue;
            }
            for fname in ["Secure Preferences", "Preferences"] {
                let pref = prof_dir.join(fname);
                if let Ok(raw) = std::fs::read_to_string(&pref) {
                    for found in extract_matching_ids_from_preferences_json(&raw) {
                        ids.insert(found);
                    }
                }
            }
        }
    }
    ids
}

/// Resolves the unpacked extension folder path (`extension/`) for browser Developer Mode loading.
pub fn resolve_extension_dir() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("extension");
            if sibling.join("manifest.json").exists() {
                return Some(sibling);
            }
            if let Some(repo) = dir
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
            {
                let dev_ext = repo.join("extension");
                if dev_ext.join("manifest.json").exists() {
                    return Some(dev_ext);
                }
            }
        }
    }
    let cwd_ext = PathBuf::from("extension");
    if cwd_ext.join("manifest.json").exists() {
        return std::fs::canonicalize(&cwd_ext).ok().or(Some(cwd_ext));
    }
    None
}

#[cfg(windows)]
fn write_hkcu_default_value(reg_path: &str, value: &str) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("reg.exe")
        .args(["add", reg_path, "/ve", "/t", "REG_SZ", "/d", value, "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// Automatically detects loaded TFM/TurboGet browser extension IDs from Chrome/Edge/Brave
/// and keeps Native Messaging host manifest + HKCU registration synchronized on Windows.
pub fn auto_register_native_host(app_dir: &Path) {
    #[cfg(windows)]
    {
        let Some(host_exe) = resolve_host_exe_path() else {
            return;
        };
        let ids = collect_browser_extension_ids();
        let origins: Vec<String> = ids
            .into_iter()
            .map(|id| format!("chrome-extension://{}/", id))
            .collect();

        let manifest = serde_json::json!({
            "name": "com.turboget.host",
            "description": "Take File Manager Native Messaging Host",
            "path": host_exe.to_string_lossy().to_string(),
            "type": "stdio",
            "allowed_origins": origins
        });

        let manifest_path = app_dir.join("com.turboget.host.json");
        if let Ok(pretty) = serde_json::to_string_pretty(&manifest) {
            if std::fs::write(&manifest_path, pretty).is_ok() {
                let m_str = manifest_path.to_string_lossy().to_string();
                for key in [
                    r"HKCU\Software\Google\Chrome\NativeMessagingHosts\com.turboget.host",
                    r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.turboget.host",
                    r"HKCU\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.turboget.host",
                ] {
                    write_hkcu_default_value(key, &m_str);
                }
            }
        }

        let ff_manifest = build_firefox_native_manifest_json(&host_exe);
        let ff_manifest_path = app_dir.join("com.turboget.host.firefox.json");
        if let Ok(pretty) = serde_json::to_string_pretty(&ff_manifest) {
            if std::fs::write(&ff_manifest_path, pretty).is_ok() {
                let ff_str = ff_manifest_path.to_string_lossy().to_string();
                write_hkcu_default_value(
                    r"HKCU\Software\Mozilla\NativeMessagingHosts\com.turboget.host",
                    &ff_str,
                );
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app_dir;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_matching_extension_ids_from_preferences() {
        let sample = r#"{
            "extensions": {
                "settings": {
                    "nmgolfmhnmdnimfakegjfkcappnepolo": {
                        "path": "E:\\Project\\idm\\extension"
                    },
                    "abcdefghijklmnopabcdefghijklmnop": {
                        "manifest": { "name": "Take File Manager" },
                        "path": "C:\\SomeOtherFolder"
                    },
                    "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz": {
                        "manifest": { "name": "Unrelated AdBlocker" },
                        "path": "C:\\Users\\Test\\Extensions\\abc"
                    }
                }
            }
        }"#;

        let ids = extract_matching_ids_from_preferences_json(sample);
        assert!(ids.contains(&"nmgolfmhnmdnimfakegjfkcappnepolo".to_string()));
        assert!(ids.contains(&"abcdefghijklmnopabcdefghijklmnop".to_string()));
        assert!(!ids.contains(&"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".to_string()));
    }

    #[test]
    fn test_firefox_native_manifest_uses_allowed_extensions() {
        let ff = build_firefox_native_manifest_json(Path::new("C:\\App\\turboget-host.exe"));
        assert!(ff.get("allowed_origins").is_none());
        let allowed = ff.get("allowed_extensions").and_then(|v| v.as_array()).unwrap();
        assert_eq!(allowed[0].as_str(), Some(FIREFOX_TFM_EXTENSION_ID));
    }
}
