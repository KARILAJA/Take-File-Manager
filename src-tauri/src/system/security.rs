use anyhow::Result;
use std::path::Path;

fn sanitize_ini_value(val: &str) -> String {
    val.chars()
        .filter(|c| !c.is_control() && *c != '\r' && *c != '\n')
        .take(2048)
        .collect()
}

/// Apply Windows Mark-of-the-Web (`Zone.Identifier` NTFS Alternate Data Stream)
/// so Windows Defender SmartScreen treats downloaded files as Internet-origin (`ZoneId=3`).
/// On non-Windows OSes or non-NTFS filesystems where ADS isn't supported, degrades gracefully.
pub fn apply_mark_of_the_web(
    file_path: &Path,
    host_url: &str,
    referrer_url: Option<&str>,
) -> Result<()> {
    #[cfg(windows)]
    {
        let ads_path = format!("{}:Zone.Identifier", file_path.to_string_lossy());
        let mut content = format!(
            "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl={}\r\n",
            sanitize_ini_value(host_url)
        );
        if let Some(ref_url) = referrer_url {
            if !ref_url.trim().is_empty() {
                content.push_str(&format!("ReferrerUrl={}\r\n", sanitize_ini_value(ref_url)));
            }
        }
        // Ignore non-NTFS unsupported errors gracefully, propagate only unexpected permission issues?
        // Actually even FAT32 / RamDisks reject ':Zone.Identifier' — log and return Ok if Unsupported/InvalidInput
        if let Err(e) = std::fs::write(&ads_path, content.as_bytes()) {
            let kind = e.kind();
            if kind != std::io::ErrorKind::Unsupported
                && kind != std::io::ErrorKind::InvalidInput
            {
                return Err(e.into());
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = (file_path, host_url, referrer_url);
    }

    Ok(())
}
