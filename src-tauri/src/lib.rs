pub mod api;
pub mod commands;
pub mod engine;
pub mod events;
pub mod manager;
pub mod media;
pub mod sitegrab;
pub mod storage;
pub mod system;

use manager::DownloadManager;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Builder, Manager, WindowEvent,
};

fn bring_main_window_to_front(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_always_on_top(true);
        let _ = win.set_focus();
        let _ = win.set_always_on_top(false);
    }
}

fn try_forward_url_to_running_instance(api_json_path: &std::path::Path, target_url: &str) -> bool {
    let content = match std::fs::read_to_string(api_json_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let info: crate::api::auth::ApiConnectionInfo = match serde_json::from_str(&content) {
        Ok(i) => i,
        Err(_) => return false,
    };
    let addr = format!("127.0.0.1:{}", info.port);
    let mut stream = match std::net::TcpStream::connect_timeout(
        &match addr.parse() {
            Ok(a) => a,
            Err(_) => return false,
        },
        std::time::Duration::from_millis(800),
    ) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let body = serde_json::json!({
        "url": target_url,
        "prompt_user": true
    })
    .to_string();
    let req = format!(
        "POST /v1/downloads HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        info.port,
        info.token,
        body.len(),
        body
    );
    use std::io::{Read, Write};
    if stream.write_all(req.as_bytes()).is_err() || stream.flush().is_err() {
        return false;
    }
    let mut resp = [0u8; 64];
    let n = stream.read(&mut resp).unwrap_or(0);
    let head = String::from_utf8_lossy(&resp[..n]);
    head.contains("202") || head.contains("201") || head.contains("200")
}

pub fn run() {
    let app_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("turboget");
    std::fs::create_dir_all(&app_dir).ok();
    let db_path = app_dir.join("turboget.db");
    let api_json_path = app_dir.join("api.json");

    let start_minimized = std::env::args()
        .skip(1)
        .any(|a| a.eq_ignore_ascii_case("--minimized") || a.eq_ignore_ascii_case("--background"));

    let launch_arg_url = std::env::args()
        .skip(1)
        .find(|a| {
            let l = a.trim().to_ascii_lowercase();
            l.starts_with("magnet:")
                || l.starts_with("http://")
                || l.starts_with("https://")
                || l.starts_with("ftp://")
                || l.ends_with(".torrent")
        });

    if let Some(ref url) = launch_arg_url {
        if try_forward_url_to_running_instance(&api_json_path, url) {
            return;
        }
    }

    let has_launch_url = launch_arg_url.is_some();

    let manager = DownloadManager::new(db_path).expect("failed to initialize DownloadManager");
    if let Some(url) = launch_arg_url {
        let save_dir = commands::default_download_dir().to_string_lossy().to_string();
        let file_name = if crate::engine::torrent::is_torrent_url(&url) {
            crate::engine::torrent::parse_magnet_display_name(&url)
        } else {
            None
        };
        manager.push_pending_prompt(manager::PendingDownloadPrompt {
            prompt_id: format!("cli-{}", uuid::Uuid::new_v4()),
            url,
            file_name,
            save_dir,
            headers: Vec::new(),
            segments_cfg: None,
            speed_limit: None,
            is_media: false,
            format_id: None,
            audio_only: false,
        });
    }

    Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(manager)
        .manage(commands::CrawlCancelState(std::sync::Arc::new(
            tokio::sync::Mutex::new(None),
        )))
        .setup(move |app| {
            let show_item = MenuItem::with_id(app, "tray_show", "Tampilkan Take File Manager", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "tray_quit", "Keluar Take File Manager", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&tray_menu)
                .tooltip("Take File Manager (Berjalan di Latar Belakang)")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "tray_show" => bring_main_window_to_front(app),
                    "tray_quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        bring_main_window_to_front(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            let _tray = tray_builder.build(app)?;

            let mgr = app.state::<DownloadManager>();
            let m = mgr.inner().clone();
            let app_handle = app.handle().clone();
            m.set_ui_waker(std::sync::Arc::new(move || {
                bring_main_window_to_front(&app_handle);
            }));

            let notif_handle = app.handle().clone();
            let notif_dir = app_dir.clone();
            std::thread::spawn(move || {
                crate::system::notify::init_desktop_notifier(&notif_dir);
            });
            m.set_completion_notifier(std::sync::Arc::new(move |ev| {
                let win = notif_handle.get_webview_window("main");
                let visible_and_focused = win
                    .as_ref()
                    .map(|w| {
                        w.is_visible().unwrap_or(false)
                            && w.is_focused().unwrap_or(false)
                            && !w.is_minimized().unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !visible_and_focused {
                    if ev.success {
                        let size_str = crate::system::notify::format_human_size(ev.total_bytes);
                        crate::system::notify::send_notification(
                            "Unduhan Selesai!",
                            &format!("{} ({})", ev.file_name, size_str),
                        );
                    } else {
                        let msg = ev.error.as_deref().unwrap_or("Koneksi terputus");
                        crate::system::notify::send_notification(
                            "Unduhan Gagal",
                            &format!("{}: {}", ev.file_name, msg),
                        );
                    }
                }
            }));
            let api_path = api_json_path.clone();
            tauri::async_runtime::spawn(async move {
                let speed_limit_kbps = {
                    let conn = m.get_db_conn().await;
                    crate::engine::network::load_network_settings(&conn).speed_limit_kbps
                };
                if speed_limit_kbps > 0 {
                    m.set_global_speed_limit(speed_limit_kbps.saturating_mul(1024))
                        .await;
                }
                let _ = m.recover_interrupted_downloads().await;
                if let Err(e) = crate::api::server::start_local_api(m, Some(api_path)).await {
                    eprintln!("Failed to start LocalApi: {}", e);
                }
            });
            let m2 = mgr.inner().clone();
            tauri::async_runtime::spawn(async move {
                drop(m2.start_scheduler_loop());
            });
            let m_auto = mgr.inner().clone();
            tauri::async_runtime::spawn(async move {
                let conn = m_auto.get_db_conn().await;
                let configured = crate::storage::db::get_setting(&conn, "autostart_configured")
                    .ok()
                    .flatten();
                let enabled_pref = crate::storage::db::get_setting(&conn, "autostart_enabled")
                    .ok()
                    .flatten();
                let should_enable = match configured.as_deref() {
                    None => {
                        let _ = crate::storage::db::set_setting(&conn, "autostart_configured", "1");
                        let _ = crate::storage::db::set_setting(&conn, "autostart_enabled", "true");
                        true
                    }
                    Some(_) => enabled_pref.as_deref() != Some("false"),
                };
                drop(conn);
                if should_enable {
                    let _ = crate::system::autostart::set_autostart(true);
                }
            });
            let nh_dir = app_dir.clone();
            std::thread::spawn(move || {
                crate::system::native_host::auto_register_native_host(&nh_dir);
            });
            if start_minimized && !has_launch_url {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
            } else {
                bring_main_window_to_front(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_version,
            commands::probe_url,
            commands::add_download,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::delete_download,
            commands::list_downloads,
            commands::get_download,
            commands::get_segments,
            commands::open_file,
            commands::open_folder,
            commands::set_speed_limit,
            commands::queue_list,
            commands::queue_create,
            commands::queue_update,
            commands::queue_delete,
            commands::queue_start,
            commands::queue_pause,
            commands::schedule_list,
            commands::schedule_create,
            commands::schedule_update,
            commands::schedule_delete,
            commands::category_list,
            commands::category_update,
            commands::set_download_priority,
            commands::execute_power_action,
            commands::media_probe,
            commands::media_download,
            commands::media_tools_status,
            commands::expand_url_pattern,
            commands::add_batch,
            commands::add_from_pattern,
            commands::export_list,
            commands::import_list,
            commands::grab_site_start,
            commands::grab_site_stop,
            commands::agent_access_get,
            commands::agent_access_set,
            commands::get_audit_logs,
            commands::settings_get,
            commands::settings_set,
            commands::check_app_update,
            commands::get_default_download_dir,
            commands::pick_folder,
            commands::pick_torrent_file,
            commands::pop_pending_download_prompt,
            commands::verify_file_checksum,
            commands::pause_all_downloads,
            commands::window_minimize,
            commands::window_toggle_maximize,
            commands::window_close_to_tray,
            commands::autostart_get,
            commands::autostart_set,
            commands::browsers_detect,
            commands::browser_integrate,
            commands::get_extension_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TurboGet tauri application");
}
