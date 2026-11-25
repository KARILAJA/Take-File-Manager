use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const TFM_ICON_BYTES: &[u8] = include_bytes!("../../icons/64x64.png");
static ICON_PATH_CELL: OnceLock<PathBuf> = OnceLock::new();

#[cfg(windows)]
const AUMID: &str = "TakeFileManager.Desktop";

fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(any(windows, test))]
fn encode_utf16le_base64(text: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        bytes.push((unit & 0xff) as u8);
        bytes.push((unit >> 8) as u8);
    }
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub fn format_human_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut idx = 0usize;
    let mut val = bytes as f64;
    while val >= 1024.0 && idx < UNITS.len() - 1 {
        val /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", val, UNITS[idx])
    }
}

/// Persists the TFM icon PNG to disk and registers a Windows AppUserModelId so native
/// desktop toast notifications display "Take File Manager" and our TFM logo.
pub fn init_desktop_notifier(app_dir: &Path) {
    let icon_path = app_dir.join("tfm-icon.png");
    if std::fs::write(&icon_path, TFM_ICON_BYTES).is_ok() {
        let _ = ICON_PATH_CELL.set(icon_path.clone());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let key = format!(r"HKCU\Software\Classes\AppUserModelId\{}", AUMID);
            let icon_str = icon_path.to_string_lossy().to_string();
            let _ = std::process::Command::new("reg.exe")
                .args(["add", &key, "/v", "DisplayName", "/t", "REG_SZ", "/d", "Take File Manager", "/f"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            let _ = std::process::Command::new("reg.exe")
                .args(["add", &key, "/v", "IconUri", "/t", "REG_SZ", "/d", &icon_str, "/f"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }
    }
}

/// Desktop notification dispatcher (Windows native WinRT Toast + stdout fallback).
pub fn send_notification(title: &str, body: &str) {
    println!("[Desktop Notification] {}: {}", title, body);

    #[cfg(windows)]
    {
        let title_esc = xml_escape(title);
        let body_esc = xml_escape(body);
        let img_tag = ICON_PATH_CELL
            .get()
            .filter(|p| p.exists())
            .map(|p| {
                let uri = p.to_string_lossy().replace('\\', "/");
                format!(
                    "<image placement=\"appLogoOverride\" src=\"file:///{}\"/>",
                    xml_escape(&uri)
                )
            })
            .unwrap_or_default();

        let toast_xml = format!(
            "<toast><visual><binding template=\"ToastGeneric\">{}<text>{}</text><text>{}</text></binding></visual></toast>",
            img_tag, title_esc, body_esc
        );
        let toast_ps = toast_xml.replace('\'', "''");
        let ps_script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null\n\
             [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null\n\
             $xml = New-Object Windows.Data.Xml.Dom.XmlDocument\n\
             $xml.LoadXml('{}')\n\
             $toast = [Windows.UI.Notifications.ToastNotification]::new($xml)\n\
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{}').Show($toast)",
            toast_ps, AUMID
        );

        let encoded = encode_utf16le_base64(&ps_script);
        std::thread::spawn(move || {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let _ = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_escape_and_utf16_base64() {
        assert_eq!(xml_escape("A & <B> \"C\""), "A &amp; &lt;B&gt; &quot;C&quot;");
        assert_eq!(encode_utf16le_base64("hi"), "aABpAA==");
        assert_eq!(format_human_size(1536), "1.5 KB");
    }
}
