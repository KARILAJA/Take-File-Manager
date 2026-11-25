use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub notes: Option<String>,
    pub download_url: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub notes: Option<String>,
    pub download_url: Option<String>,
}

pub fn parse_semver_tuple(v: &str) -> (u64, u64, u64) {
    let clean = v.trim().trim_start_matches('v');
    let core = clean.split('-').next().unwrap_or(clean);
    let mut parts = core.split('.');
    let major = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    (major, minor, patch)
}

pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    parse_semver_tuple(candidate) > parse_semver_tuple(current)
}

pub async fn check_for_updates(
    client: &reqwest::Client,
    current_version: &str,
    feed_url: &str,
) -> Result<UpdateCheckResult> {
    let resp = client.get(feed_url).send().await?;
    let manifest: UpdateManifest = resp.error_for_status()?.json().await?;
    let update_available = is_newer_version(current_version, &manifest.version);
    Ok(UpdateCheckResult {
        current_version: current_version.to_string(),
        latest_version: manifest.version,
        update_available,
        notes: manifest.notes,
        download_url: manifest.download_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_comparison() {
        assert!(is_newer_version("0.1.0", "0.2.0"));
        assert!(is_newer_version("0.1.0", "v0.1.1"));
        assert!(is_newer_version("0.9.5", "1.0.0"));
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(!is_newer_version("0.2.0", "0.1.9"));
    }
}
