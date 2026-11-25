use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConnectionInfo {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

#[derive(Clone)]
pub struct AuthConfig {
    pub token: String,
    pub port: u16,
}

pub fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn write_api_json(path: &Path, info: &ApiConnectionInfo) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(info)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Validates Host header, rejects web Origin headers (CSRF / DNS rebinding protection),
/// and verifies Bearer token authentication.
pub async fn auth_and_origin_guard(
    State(auth): State<AuthConfig>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. Validate Host header is strictly loopback
    if let Some(host_val) = req.headers().get(header::HOST) {
        let host = host_val.to_str().map_err(|_| StatusCode::BAD_REQUEST)?;
        let expected_ip = format!("127.0.0.1:{}", auth.port);
        let expected_lh = format!("localhost:{}", auth.port);
        if host != expected_ip && host != expected_lh && host != "127.0.0.1" && host != "localhost" {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // 2. Reject requests with standard web Origin headers (prevent CSRF / DNS rebinding)
    if let Some(origin_val) = req.headers().get(header::ORIGIN) {
        let origin = origin_val.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
        if !origin.starts_with("chrome-extension://") && !origin.starts_with("moz-extension://") {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // 3. Verify Bearer token in Authorization header (or ?token= for WebSocket clients)
    let auth_header_ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|t| constant_time_eq(t.as_bytes(), auth.token.as_bytes()))
        .unwrap_or(false);

    let ws_query_ok = if !auth_header_ok && req.uri().path() == "/v1/events" {
        req.uri()
            .query()
            .and_then(|q| {
                q.split('&').find_map(|pair| {
                    let (k, v) = pair.split_once('=')?;
                    if k == "token" {
                        Some(v)
                    } else {
                        None
                    }
                })
            })
            .map(|t| constant_time_eq(t.as_bytes(), auth.token.as_bytes()))
            .unwrap_or(false)
    } else {
        false
    };

    if !auth_header_ok && !ws_query_ok {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
