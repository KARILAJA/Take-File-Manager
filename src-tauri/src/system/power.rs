use anyhow::{anyhow, Result};
use std::process::Command;

/// Execute post-download power actions: none, exit, shutdown, sleep, hibernate, run
pub fn execute_power_action(action: &str, command: Option<&str>) -> Result<()> {
    match action.to_lowercase().as_str() {
        "none" => Ok(()),
        "exit" => {
            std::process::exit(0);
        }
        "shutdown" => {
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                // Graceful 60s shutdown with cancel possibility
                Command::new("shutdown")
                    .args(["/s", "/t", "60", "/c", "Take File Manager: Unduhan selesai, mematikan komputer..."])
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()?;
            }
            #[cfg(not(target_os = "windows"))]
            {
                Command::new("shutdown").arg("-h").arg("+1").spawn()?;
            }
            Ok(())
        }
        "sleep" => {
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                Command::new("rundll32.exe")
                    .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()?;
            }
            #[cfg(target_os = "macos")]
            {
                Command::new("pmset").args(["sleepnow"]).spawn()?;
            }
            #[cfg(target_os = "linux")]
            {
                Command::new("systemctl").args(["suspend"]).spawn()?;
            }
            Ok(())
        }
        "hibernate" => {
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                Command::new("shutdown")
                    .args(["/h"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()?;
            }
            #[cfg(not(target_os = "windows"))]
            {
                Command::new("systemctl").args(["hibernate"]).spawn()?;
            }
            Ok(())
        }
        "run" => {
            if let Some(cmd_str) = command {
                if !cmd_str.trim().is_empty() {
                    #[cfg(target_os = "windows")]
                    {
                        Command::new("cmd").args(["/C", cmd_str]).spawn()?;
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        Command::new("sh").args(["-c", cmd_str]).spawn()?;
                    }
                    return Ok(());
                }
            }
            Err(anyhow!("No command specified for 'run' action"))
        }
        other => Err(anyhow!("Unknown power action '{}'", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_action_none() {
        assert!(execute_power_action("none", None).is_ok());
    }

    #[test]
    fn test_power_action_invalid() {
        assert!(execute_power_action("invalid_action_xyz", None).is_err());
    }
}
