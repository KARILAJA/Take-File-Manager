use anyhow::Result;
use reqwest::header::{HeaderValue, AUTHORIZATION};
use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    System,
    Direct,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    pub proxy_mode: ProxyMode,
    pub proxy_url: Option<String>,
    pub proxy_username: Option<String>,
    pub proxy_password: Option<String>,
    pub max_connections_per_host: usize,
    pub auto_update_enabled: bool,
    pub update_feed_url: Option<String>,
    #[serde(default)]
    pub speed_limit_kbps: u64,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            proxy_mode: ProxyMode::System,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            max_connections_per_host: 6,
            auto_update_enabled: true,
            update_feed_url: None,
            speed_limit_kbps: 0,
        }
    }
}

pub const NETWORK_SETTINGS_KEY: &str = "network_settings";

pub fn load_network_settings(conn: &rusqlite::Connection) -> NetworkSettings {
    match crate::storage::db::get_setting(conn, NETWORK_SETTINGS_KEY) {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        _ => NetworkSettings::default(),
    }
}

pub fn save_network_settings(
    conn: &rusqlite::Connection,
    settings: &NetworkSettings,
) -> Result<()> {
    let json = serde_json::to_string(settings)?;
    crate::storage::db::set_setting(conn, NETWORK_SETTINGS_KEY, &json)?;
    Ok(())
}

/// Base64 encode arbitrary bytes (RFC 4648) without external crate dependencies.
pub fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Build an HTTP Basic Authorization header value (`Basic <base64>`).
pub fn encode_basic_auth(username: &str, password: &str) -> String {
    let raw = format!("{}:{}", username, password);
    format!("Basic {}", base64_encode(raw.as_bytes()))
}

/// Mask sensitive credential fields for safe logging / UI display.
pub fn mask_secret(secret: &str) -> String {
    if secret.is_empty() {
        return String::new();
    }
    "********".to_string()
}

/// Construct a reqwest `Client` applying proxy, connection-per-host limits, and optional per-download TLS setting.
pub fn build_http_client(
    settings: &NetworkSettings,
    allow_insecure_tls: bool,
    basic_auth: Option<(&str, &str)>,
) -> Result<Client> {
    let mut builder = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .pool_max_idle_per_host(settings.max_connections_per_host.clamp(1, 32))
        .connect_timeout(Duration::from_secs(20));

    match settings.proxy_mode {
        ProxyMode::Direct => {
            builder = builder.no_proxy();
        }
        ProxyMode::Manual => {
            if let Some(ref p_url) = settings.proxy_url {
                if !p_url.trim().is_empty() {
                    let mut proxy = Proxy::all(p_url.trim())?;
                    if let Some(ref user) = settings.proxy_username {
                        if !user.is_empty() {
                            let pass = settings.proxy_password.as_deref().unwrap_or("");
                            proxy = proxy.basic_auth(user, pass);
                        }
                    }
                    builder = builder.proxy(proxy);
                }
            }
        }
        ProxyMode::System => {}
    }

    if allow_insecure_tls {
        builder = builder.danger_accept_invalid_certs(true);
    }

    if let Some((user, pass)) = basic_auth {
        let mut default_hdrs = reqwest::header::HeaderMap::new();
        let val = encode_basic_auth(user, pass);
        if let Ok(hv) = HeaderValue::from_str(&val) {
            default_hdrs.insert(AUTHORIZATION, hv);
            builder = builder.default_headers(default_hdrs);
        }
    }

    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_auth_encoding_rfc7617() {
        // Known RFC 7617 test vector: Aladdin:open sesame -> QWxhZGRpbjpvcGVuIHNlc2FtZQ==
        assert_eq!(
            encode_basic_auth("Aladdin", "open sesame"),
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
    }

    #[test]
    fn test_mask_secret() {
        assert_eq!(mask_secret(""), "");
        assert_eq!(mask_secret("my-secret-password"), "********");
    }
}
