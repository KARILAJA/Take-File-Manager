use anyhow::Result;

#[cfg(target_os = "windows")]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const VALUE_NAME: &str = "Take File Manager";
#[cfg(target_os = "windows")]
const LEGACY_VALUE_NAME: &str = "TurboGet";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Returns whether Take File Manager is currently registered to run at Windows startup.
pub fn is_autostart_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        for name in [VALUE_NAME, LEGACY_VALUE_NAME] {
            if let Ok(out) = Command::new("reg")
                .args(["query", RUN_KEY, "/v", name])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
            {
                if out.status.success() {
                    return true;
                }
            }
        }
        false
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Registers or removes Take File Manager auto-start on Windows logon (`--minimized` in System Tray).
pub fn set_autostart(enabled: bool) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        let exe_path = std::env::current_exe()?;
        let path_str = exe_path.to_string_lossy();

        if enabled {
            Command::new("reg")
                .args([
                    "add",
                    RUN_KEY,
                    "/v",
                    VALUE_NAME,
                    "/t",
                    "REG_SZ",
                    "/d",
                    &format!("\"{}\" --minimized", path_str),
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output()?;

            let _ = Command::new("reg")
                .args(["delete", RUN_KEY, "/v", LEGACY_VALUE_NAME, "/f"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        } else {
            for name in [VALUE_NAME, LEGACY_VALUE_NAME] {
                let _ = Command::new("reg")
                    .args(["delete", RUN_KEY, "/v", name, "/f"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output();
            }
        }
    }
    let _ = enabled;
    Ok(())
}
