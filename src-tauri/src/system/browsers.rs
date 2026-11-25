use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::system::native_host::{
    auto_register_native_host, extract_matching_ids_from_preferences_json, resolve_extension_dir,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserIntegrationInfo {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub running: bool,
    pub executable_path: Option<String>,
    pub extension_installed: bool,
    pub supports_developer_mode: bool,
    pub supports_cli_autoload: bool,
    pub coming_soon: bool,
    pub status_label: String,
    pub extensions_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserIntegrateResult {
    pub browser_id: String,
    pub browser_name: String,
    pub extension_dir: String,
    pub extensions_url: String,
    pub launched: bool,
    pub auto_loaded: bool,
    pub needs_relaunch: bool,
    pub message: String,
}

#[cfg(windows)]
fn is_process_running(exe_name: &str) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let filter = format!("IMAGENAME eq {}", exe_name);
    if let Ok(out) = std::process::Command::new("tasklist")
        .args(["/FI", &filter, "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        let stdout = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
        return stdout.contains(&exe_name.to_ascii_lowercase());
    }
    false
}

#[cfg(not(windows))]
fn is_process_running(_exe_name: &str) -> bool {
    false
}

#[cfg(windows)]
fn kill_process_by_name(exe_name: &str) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("taskkill")
        .args(["/IM", exe_name, "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(windows)]
fn query_app_path_registry(exe_name: &str) -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    for hive in ["HKCU", "HKLM"] {
        let key = format!(
            r"{}\Software\Microsoft\Windows\CurrentVersion\App Paths\{}",
            hive, exe_name
        );
        if let Ok(out) = std::process::Command::new("reg")
            .args(["query", &key, "/ve"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    if let Some(idx) = line.find("REG_SZ") {
                        let raw = line[idx + 6..].trim().trim_matches('"');
                        let p = PathBuf::from(raw);
                        if p.exists() {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_first_existing(candidates: Vec<PathBuf>, reg_exe: &str) -> Option<PathBuf> {
    for p in candidates {
        if p.exists() {
            return Some(p);
        }
    }
    #[cfg(windows)]
    if let Some(reg_p) = query_app_path_registry(reg_exe) {
        return Some(reg_p);
    }
    #[cfg(not(windows))]
    let _ = reg_exe;
    None
}

fn check_extension_in_user_data(roots: &[PathBuf]) -> bool {
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
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
                    if !extract_matching_ids_from_preferences_json(&raw).is_empty() {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Generates an interactive browser-side setup page so Chrome/Edge/Opera open a clear
/// step-by-step installer tab (since Chromium blocks external `chrome://extensions` CLI launches).
fn write_browser_setup_page(
    app_dir: &Path,
    browser_name: &str,
    extensions_url: &str,
    extension_dir: &str,
    auto_loaded: bool,
) -> Option<PathBuf> {
    let page_path = app_dir.join("browser-setup.html");
    let bn = html_escape(browser_name);
    let eu = html_escape(extensions_url);
    let ed = html_escape(extension_dir);
    let ed_js = serde_json::to_string(extension_dir).unwrap_or_else(|_| "\"\"".to_string());
    let eu_js = serde_json::to_string(extensions_url).unwrap_or_else(|_| "\"\"".to_string());

    let banner = if auto_loaded {
        "<div class='ok'>Ekstensi Take File Manager dimuat otomatis ke browser ini! Jika belum muncul di toolbar, klik ikon Puzzle di pojok kanan atas lalu sematkan (Pin) <b>Take File Manager</b>.</div>"
    } else {
        "<div class='warn'><b>Wajib Aktifkan Developer Mode:</b> Karena browser Chromium memblokir pemasangan otomatis dari luar tanpa Developer Mode / Web Store, selesaikan 3 langkah cepat (10 detik) di bawah ini.</div>"
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="id">
<head>
<meta charset="UTF-8" />
<title>Integrasi Take File Manager — {bn}</title>
<style>
  body {{ margin: 0; font-family: system-ui, -apple-system, sans-serif; background: #0b1326; color: #dae2fd; display: flex; justify-content: center; padding: 40px 16px; }}
  .card {{ max-width: 660px; width: 100%; background: #171f33; border: 1px solid rgba(141,144,160,0.35); border-radius: 16px; padding: 26px; box-shadow: 0 20px 50px rgba(0,0,0,0.45); }}
  h1 {{ margin: 0 0 6px; font-size: 20px; }}
  .sub {{ color: #93a1be; font-size: 13px; margin-bottom: 18px; }}
  .warn {{ background: rgba(245,158,11,0.14); border: 1px solid rgba(245,158,11,0.45); color: #fcd34d; border-radius: 10px; padding: 12px 14px; font-size: 13px; line-height: 1.5; margin-bottom: 18px; }}
  .ok {{ background: rgba(16,185,129,0.15); border: 1px solid rgba(16,185,129,0.45); color: #68dba9; border-radius: 10px; padding: 12px 14px; font-size: 13px; line-height: 1.5; margin-bottom: 18px; }}
  .step {{ background: #0b1326; border: 1px solid rgba(141,144,160,0.28); border-radius: 10px; padding: 12px 14px; margin-bottom: 12px; display: flex; justify-content: space-between; align-items: center; gap: 12px; }}
  .step b {{ display: block; margin-bottom: 3px; font-size: 13.5px; }}
  .step span {{ font-size: 12px; color: #93a1be; }}
  code {{ background: #222a3d; padding: 2px 6px; border-radius: 5px; font-family: ui-monospace, monospace; color: #93c5fd; }}
  button {{ background: #2563eb; color: #fff; border: none; border-radius: 8px; padding: 8px 13px; font-weight: 600; font-size: 12px; cursor: pointer; flex-shrink: 0; }}
  button:hover {{ filter: brightness(1.1); }}
</style>
</head>
<body>
  <div class="card">
    <h1>Integrasi Ekstensi {bn}</h1>
    <div class="sub">Take File Manager Native Host sudah otomatis terdaftar untuk {bn}.</div>
    {banner}
    <div class="step">
      <div>
        <b>1. Buka halaman Ekstensi {bn}</b>
        <span>Salin URL <code>{eu}</code> lalu tempel di address bar tab baru &amp; tekan Enter.</span>
      </div>
      <button onclick="copyTxt({eu_js}, this)">Salin URL</button>
    </div>
    <div class="step">
      <div>
        <b>2. Aktifkan "Developer mode / Mode pengembang"</b>
        <span>Nyalakan sakelar Developer mode di sudut halaman ekstensi {bn}.</span>
      </div>
    </div>
    <div class="step">
      <div>
        <b>3. Klik "Load unpacked" (atau Drag &amp; Drop folder)</b>
        <span>Tempel path <code>{ed}</code> (atau seret folder <b>extension</b> yang terbuka di File Explorer langsung ke halaman ekstensi).</span>
      </div>
      <button onclick="copyTxt({ed_js}, this)">Salin Path</button>
    </div>
  </div>
  <script>
    function copyTxt(val, btn) {{
      navigator.clipboard.writeText(val).then(() => {{
        const prev = btn.textContent;
        btn.textContent = "Tersalin ✓";
        setTimeout(() => btn.textContent = prev, 1500);
      }});
    }}
  </script>
</body>
</html>"#
    );

    if std::fs::write(&page_path, html).is_ok() {
        Some(page_path)
    } else {
        None
    }
}

pub fn detect_browsers() -> Vec<BrowserIntegrationInfo> {
    let local_app = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let app_data = std::env::var("APPDATA").unwrap_or_default();

    let chrome_exe = find_first_existing(
        vec![
            PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
            Path::new(&local_app).join(r"Google\Chrome\Application\chrome.exe"),
        ],
        "chrome.exe",
    );
    let chrome_ext = if local_app.is_empty() {
        false
    } else {
        check_extension_in_user_data(&[Path::new(&local_app).join(r"Google\Chrome\User Data")])
    };

    let edge_exe = find_first_existing(
        vec![
            PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
            PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
            Path::new(&local_app).join(r"Microsoft\Edge\Application\msedge.exe"),
        ],
        "msedge.exe",
    );
    let edge_ext = if local_app.is_empty() {
        false
    } else {
        check_extension_in_user_data(&[Path::new(&local_app).join(r"Microsoft\Edge\User Data")])
    };

    let opera_exe = find_first_existing(
        vec![
            Path::new(&local_app).join(r"Programs\Opera\opera.exe"),
            Path::new(&local_app).join(r"Programs\Opera GX\opera.exe"),
            PathBuf::from(r"C:\Program Files\Opera\opera.exe"),
            PathBuf::from(r"C:\Program Files (x86)\Opera\opera.exe"),
        ],
        "opera.exe",
    );
    let opera_ext = if app_data.is_empty() {
        false
    } else {
        check_extension_in_user_data(&[Path::new(&app_data).join("Opera Software")])
    };

    let firefox_exe = find_first_existing(
        vec![
            PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"),
            PathBuf::from(r"C:\Program Files (x86)\Mozilla Firefox\firefox.exe"),
            Path::new(&local_app).join(r"Mozilla Firefox\firefox.exe"),
        ],
        "firefox.exe",
    );

    vec![
        BrowserIntegrationInfo {
            id: "chrome".to_string(),
            name: "Google Chrome".to_string(),
            installed: chrome_exe.is_some(),
            running: is_process_running("chrome.exe"),
            executable_path: chrome_exe.map(|p| p.to_string_lossy().to_string()),
            extension_installed: chrome_ext,
            supports_developer_mode: true,
            supports_cli_autoload: false,
            coming_soon: false,
            status_label: if chrome_ext {
                "Terpasang".to_string()
            } else {
                "Siap Integrasi".to_string()
            },
            extensions_url: "chrome://extensions".to_string(),
        },
        BrowserIntegrationInfo {
            id: "edge".to_string(),
            name: "Microsoft Edge".to_string(),
            installed: edge_exe.is_some(),
            running: is_process_running("msedge.exe"),
            executable_path: edge_exe.map(|p| p.to_string_lossy().to_string()),
            extension_installed: edge_ext,
            supports_developer_mode: true,
            supports_cli_autoload: true,
            coming_soon: false,
            status_label: if edge_ext {
                "Terpasang".to_string()
            } else {
                "Auto-Load Siap".to_string()
            },
            extensions_url: "edge://extensions".to_string(),
        },
        BrowserIntegrationInfo {
            id: "opera".to_string(),
            name: "Opera Browser".to_string(),
            installed: opera_exe.is_some(),
            running: is_process_running("opera.exe"),
            executable_path: opera_exe.map(|p| p.to_string_lossy().to_string()),
            extension_installed: opera_ext,
            supports_developer_mode: true,
            supports_cli_autoload: true,
            coming_soon: false,
            status_label: if opera_ext {
                "Terpasang".to_string()
            } else {
                "Auto-Load Siap".to_string()
            },
            extensions_url: "opera://extensions".to_string(),
        },
        BrowserIntegrationInfo {
            id: "firefox".to_string(),
            name: "Mozilla Firefox".to_string(),
            installed: firefox_exe.is_some(),
            running: is_process_running("firefox.exe"),
            executable_path: firefox_exe.map(|p| p.to_string_lossy().to_string()),
            extension_installed: false,
            supports_developer_mode: false,
            supports_cli_autoload: false,
            coming_soon: true,
            status_label: "Soon on Store Addon".to_string(),
            extensions_url: "about:addons".to_string(),
        },
    ]
}

pub fn integrate_browser(
    app_dir: &Path,
    browser_id: &str,
    relaunch: bool,
) -> Result<BrowserIntegrateResult, String> {
    let target_id = browser_id.trim().to_ascii_lowercase();
    if target_id == "firefox" {
        return Err("Mozilla Firefox ditunda sementara: Soon on Store Addon.".to_string());
    }

    auto_register_native_host(app_dir);

    let ext_dir = resolve_extension_dir()
        .ok_or_else(|| "Folder ekstensi (extension/manifest.json) tidak ditemukan.".to_string())?;
    let ext_dir_str = ext_dir.to_string_lossy().to_string();

    let browsers = detect_browsers();
    let info = browsers
        .into_iter()
        .find(|b| b.id == target_id)
        .ok_or_else(|| format!("Browser '{}' tidak dikenali.", browser_id))?;

    let mut launched = false;
    let mut auto_loaded = false;
    let mut needs_relaunch = false;

    if let Some(ref exe) = info.executable_path {
        let exe_name = Path::new(exe)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        if info.supports_cli_autoload {
            #[cfg(windows)]
            if relaunch && info.running && !exe_name.is_empty() {
                kill_process_by_name(exe_name);
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
            let still_running = !relaunch && info.running;
            let setup_url = write_browser_setup_page(
                app_dir,
                &info.name,
                &info.extensions_url,
                &ext_dir_str,
                !still_running,
            )
            .map(|p| format!("file:///{}", p.to_string_lossy().replace('\\', "/")))
            .unwrap_or_else(|| "about:blank".to_string());

            let mut cmd = std::process::Command::new(exe);
            if !still_running {
                cmd.arg(format!("--load-extension={}", ext_dir_str));
                auto_loaded = true;
            } else {
                needs_relaunch = true;
            }
            cmd.arg(&setup_url);
            if cmd.spawn().is_ok() {
                launched = true;
            }
        } else {
            // Chrome blocks --load-extension and external chrome:// URLs:
            // Open an interactive local guide page inside Chrome + reveal extension folder in Explorer for 1-drag Load Unpacked.
            let setup_url = write_browser_setup_page(
                app_dir,
                &info.name,
                &info.extensions_url,
                &ext_dir_str,
                false,
            )
            .map(|p| format!("file:///{}", p.to_string_lossy().replace('\\', "/")))
            .unwrap_or_else(|| "about:blank".to_string());

            let mut cmd = std::process::Command::new(exe);
            cmd.arg(&setup_url);
            if cmd.spawn().is_ok() {
                launched = true;
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let _ = std::process::Command::new("explorer.exe")
                    .raw_arg(format!("/select,\"{}\"", ext_dir_str))
                    .spawn();
            }
        }
    }

    let message = if auto_loaded {
        format!(
            "{} dijalankan dengan --load-extension dan Native Host otomatis terpasang!",
            info.name
        )
    } else if needs_relaunch {
        format!(
            "{} sedang berjalan. Klik 'Tutup & Muat Otomatis' untuk restart {} dengan ekstensi langsung terpasang, atau aktifkan Developer Mode dan seret folder 'extension' ke {}.",
            info.name, info.name, info.extensions_url
        )
    } else if launched {
        format!(
            "Panduan dibuka di {} & File Explorer diarahkan ke folder ekstensi. Buka {}, aktifkan Developer Mode, lalu Load unpacked / seret folder ekstensi.",
            info.name, info.extensions_url
        )
    } else {
        format!(
            "Browser {} tidak ditemukan. Buka {}, aktifkan Developer Mode, dan Load unpacked folder: {}",
            info.name, info.extensions_url, ext_dir_str
        )
    };

    Ok(BrowserIntegrateResult {
        browser_id: info.id,
        browser_name: info.name,
        extension_dir: ext_dir_str,
        extensions_url: info.extensions_url,
        launched,
        auto_loaded,
        needs_relaunch,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_browsers_contains_expected_entries_and_firefox_soon() {
        let list = detect_browsers();
        let ids: Vec<&str> = list.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["chrome", "edge", "opera", "firefox"]);
        let ff = list.iter().find(|b| b.id == "firefox").unwrap();
        assert!(ff.coming_soon);
        assert_eq!(ff.status_label, "Soon on Store Addon");
    }
}
