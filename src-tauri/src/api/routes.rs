use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::commands::default_download_dir;
use crate::engine::probe::{probe_url, ProbeInfo};
use crate::manager::agent_policy::{
    load_agent_policy, resolve_and_validate_agent_dir, sanitize_untrusted_text, validate_agent_url,
    AgentAccessMode, AgentRateLimiter,
};
use crate::manager::state::DownloadStatus;
use crate::manager::{DownloadManager, DownloadRequest};
use crate::storage::db::{list_queues, log_event, DownloadRecord, QueueRecord, SegmentRecord};

#[derive(Clone)]
pub struct ApiState {
    pub manager: DownloadManager,
    pub events_tx: broadcast::Sender<serde_json::Value>,
    pub agent_limiter: Arc<AgentRateLimiter>,
}

#[derive(Debug, Deserialize)]
pub struct ApiAddDownloadReq {
    pub url: String,
    pub file_name: Option<String>,
    pub save_dir: Option<String>,
    pub cookies: Option<String>,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub category_id: Option<String>,
    pub queue_id: Option<String>,
    pub segments: Option<u32>,
    pub speed_limit: Option<u64>,
    pub start_now: Option<bool>,
    #[serde(default)]
    pub prompt_user: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ApiAddDownloadResp {
    pub id: String,
    #[serde(default)]
    pub prompted: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub q: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct DownloadDetailResp {
    pub download: DownloadRecord,
    pub segments: Vec<SegmentRecord>,
}

#[derive(Debug, Deserialize)]
pub struct CancelReq {
    pub delete_file: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct RefreshUrlReq {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct ProbeReq {
    pub url: String,
    pub cookies: Option<String>,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct SpeedLimitReq {
    pub limit_bps: Option<u64>,
    pub kbps: Option<u64>,
}

fn is_agent_actor(headers: &HeaderMap) -> bool {
    headers
        .get("x-turboget-actor")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("agent"))
        .unwrap_or(false)
}

fn perm_err(msg: impl Into<String>) -> (StatusCode, String) {
    (
        StatusCode::FORBIDDEN,
        serde_json::json!({
            "error": msg.into(),
            "code": "PERMISSION_DENIED"
        })
        .to_string(),
    )
}

async fn enforce_agent_policy(
    state: &ApiState,
    headers: &HeaderMap,
    action: &str,
    url: Option<&str>,
    requested_save_dir: Option<&str>,
    download_id: Option<&str>,
) -> Result<Option<PathBuf>, (StatusCode, String)> {
    if !is_agent_actor(headers) {
        return Ok(requested_save_dir
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from));
    }

    let policy = {
        let conn = state.manager.get_db_conn().await;
        load_agent_policy(&conn)
    };

    // 1. Rate limiting check
    if !state
        .agent_limiter
        .check_and_record(policy.max_requests_per_minute)
    {
        let conn = state.manager.get_db_conn().await;
        let _ = log_event(
            &conn,
            "AGENT_AUDIT",
            download_id,
            &format!("RATE_LIMITED action={}", action),
        );
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            serde_json::json!({
                "error": "Agent request rate limit exceeded",
                "code": "RATE_LIMITED"
            })
            .to_string(),
        ));
    }

    // 2. Access mode enforcement
    match policy.mode {
        AgentAccessMode::Deny => {
            let conn = state.manager.get_db_conn().await;
            let _ = log_event(
                &conn,
                "AGENT_AUDIT",
                download_id,
                &format!("DENIED(mode=deny) action={}", action),
            );
            return Err(perm_err("Agent access is disabled (mode=deny)"));
        }
        AgentAccessMode::Ask => {
            // Mutating/network actions require explicit approval when mode == Ask
            if action != "list_downloads" && action != "get_download" {
                let conn = state.manager.get_db_conn().await;
                let _ = log_event(
                    &conn,
                    "AGENT_AUDIT",
                    download_id,
                    &format!("DENIED(mode=ask) action={}", action),
                );
                return Err(perm_err(
                    "Agent access requires user confirmation or allowlist mode",
                ));
            }
        }
        AgentAccessMode::Allowlist => {}
    }

    // 3. SSRF protection for URLs
    if let Some(u) = url {
        if let Err(e) = validate_agent_url(u, policy.block_private_ips) {
            let conn = state.manager.get_db_conn().await;
            let _ = log_event(
                &conn,
                "AGENT_AUDIT",
                download_id,
                &format!("DENIED(ssrf) action={} err={}", action, e),
            );
            return Err(perm_err(e.to_string()));
        }
    }

    // 4. Folder allowlist verification for downloads
    let resolved_dir = if action == "add_download" || action == "media_download" {
        match resolve_and_validate_agent_dir(requested_save_dir, &policy) {
            Ok(d) => Some(d),
            Err(e) => {
                let conn = state.manager.get_db_conn().await;
                let _ = log_event(
                    &conn,
                    "AGENT_AUDIT",
                    download_id,
                    &format!("DENIED(allowlist) action={} err={}", action, e),
                );
                return Err(perm_err(e.to_string()));
            }
        }
    } else {
        None
    };

    // 5. Audit log success
    {
        let conn = state.manager.get_db_conn().await;
        let _ = log_event(
            &conn,
            "AGENT_AUDIT",
            download_id,
            &format!(
                "ALLOWED action={} url={} dir={}",
                action,
                url.unwrap_or("-"),
                resolved_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
        );
    }

    Ok(resolved_dir)
}

fn sanitize_record_for_agent(mut r: DownloadRecord) -> DownloadRecord {
    r.file_name = sanitize_untrusted_text(&r.file_name);
    r.final_url = r.final_url.map(|s| sanitize_untrusted_text(&s));
    r.etag = r.etag.map(|s| sanitize_untrusted_text(&s));
    r.error = r.error.map(|s| sanitize_untrusted_text(&s));
    r
}

fn build_headers(
    cookies: Option<&str>,
    referrer: Option<&str>,
    user_agent: Option<&str>,
    extra: Option<&HashMap<String, String>>,
) -> Vec<(String, String)> {
    let mut hdrs = Vec::new();
    if let Some(c) = cookies {
        if !c.trim().is_empty() {
            hdrs.push(("Cookie".to_string(), c.to_string()));
        }
    }
    if let Some(r) = referrer {
        if !r.trim().is_empty() {
            hdrs.push(("Referer".to_string(), r.to_string()));
        }
    }
    if let Some(ua) = user_agent {
        if !ua.trim().is_empty() {
            hdrs.push(("User-Agent".to_string(), ua.to_string()));
        }
    }
    if let Some(map) = extra {
        for (k, v) in map {
            hdrs.push((k.clone(), v.clone()));
        }
    }
    hdrs
}

pub async fn add_download_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Json(body): Json<ApiAddDownloadReq>,
) -> Result<(StatusCode, Json<ApiAddDownloadResp>), (StatusCode, String)> {
    let validated_dir = enforce_agent_policy(
        &state,
        &req_headers,
        "add_download",
        Some(&body.url),
        body.save_dir.as_deref(),
        None,
    )
    .await?;

    let save_dir = validated_dir.unwrap_or_else(default_download_dir);

    let headers = build_headers(
        body.cookies.as_deref(),
        body.referrer.as_deref(),
        body.user_agent.as_deref(),
        body.headers.as_ref(),
    );

    if body.prompt_user.unwrap_or(false) && !is_agent_actor(&req_headers) {
        let prompt_id = format!("prompt-{}", uuid::Uuid::new_v4());
        state
            .manager
            .push_pending_prompt(crate::manager::PendingDownloadPrompt {
                prompt_id: prompt_id.clone(),
                url: body.url,
                file_name: body.file_name,
                save_dir: save_dir.to_string_lossy().to_string(),
                headers,
                segments_cfg: body.segments,
                speed_limit: body.speed_limit,
                is_media: false,
                format_id: None,
                audio_only: false,
            });
        return Ok((
            StatusCode::ACCEPTED,
            Json(ApiAddDownloadResp {
                id: prompt_id,
                prompted: true,
            }),
        ));
    }

    let req = DownloadRequest {
        url: body.url,
        save_dir,
        file_name: body.file_name,
        category_id: body.category_id,
        queue_id: body.queue_id,
        segments_cfg: body.segments,
        speed_limit: body.speed_limit,
        headers,
        start_now: body.start_now.unwrap_or(true),
    };

    let id = state
        .manager
        .add_download(req)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let _ = state.events_tx.send(serde_json::json!({
        "type": "download:added",
        "id": id
    }));

    Ok((
        StatusCode::CREATED,
        Json(ApiAddDownloadResp {
            id,
            prompted: false,
        }),
    ))
}

pub async fn list_downloads_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<DownloadRecord>>, (StatusCode, String)> {
    enforce_agent_policy(&state, &req_headers, "list_downloads", None, None, None).await?;
    let agent = is_agent_actor(&req_headers);

    let status_filter = query
        .status
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<DownloadStatus>().ok());

    let mut records = state
        .manager
        .list_downloads(status_filter)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(ref q) = query.q {
        let q_lower = q.to_lowercase();
        if !q_lower.is_empty() {
            records.retain(|r| {
                r.file_name.to_lowercase().contains(&q_lower)
                    || r.url.to_lowercase().contains(&q_lower)
            });
        }
    }

    if let Some(limit) = query.limit {
        records.truncate(limit);
    }

    if agent {
        records = records.into_iter().map(sanitize_record_for_agent).collect();
    }

    Ok(Json(records))
}

pub async fn get_download_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<DownloadDetailResp>, (StatusCode, String)> {
    enforce_agent_policy(&state, &req_headers, "get_download", None, None, Some(&id)).await?;
    let agent = is_agent_actor(&req_headers);

    let mut download = state
        .manager
        .get_download(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Download {} not found", id)))?;

    if agent {
        download = sanitize_record_for_agent(download);
    }

    let segments = state
        .manager
        .get_segments(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(DownloadDetailResp { download, segments }))
}

pub async fn pause_download_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    enforce_agent_policy(&state, &req_headers, "pause_download", None, None, Some(&id)).await?;
    state
        .manager
        .pause(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let _ = state.events_tx.send(serde_json::json!({
        "type": "download:state",
        "id": id,
        "status": "paused"
    }));
    Ok(StatusCode::OK)
}

pub async fn resume_download_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    enforce_agent_policy(&state, &req_headers, "resume_download", None, None, Some(&id)).await?;
    state
        .manager
        .start(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let _ = state.events_tx.send(serde_json::json!({
        "type": "download:state",
        "id": id,
        "status": "probing"
    }));
    Ok(StatusCode::OK)
}

pub async fn cancel_download_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Path(id): Path<String>,
    body: Option<Json<CancelReq>>,
) -> Result<StatusCode, (StatusCode, String)> {
    enforce_agent_policy(&state, &req_headers, "cancel_download", None, None, Some(&id)).await?;
    let delete_file = body.and_then(|b| b.delete_file).unwrap_or(false);
    state
        .manager
        .cancel(&id, delete_file)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let _ = state.events_tx.send(serde_json::json!({
        "type": "download:state",
        "id": id,
        "status": "canceled"
    }));
    Ok(StatusCode::OK)
}

pub async fn refresh_url_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<RefreshUrlReq>,
) -> Result<StatusCode, (StatusCode, String)> {
    enforce_agent_policy(
        &state,
        &req_headers,
        "refresh_url",
        Some(&body.url),
        None,
        Some(&id),
    )
    .await?;
    state
        .manager
        .refresh_url(&id, &body.url)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(StatusCode::OK)
}

pub async fn probe_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Json(body): Json<ProbeReq>,
) -> Result<Json<ProbeInfo>, (StatusCode, String)> {
    enforce_agent_policy(&state, &req_headers, "probe_url", Some(&body.url), None, None).await?;
    let agent = is_agent_actor(&req_headers);

    if crate::engine::ftp::is_ftp_url(&body.url) {
        let mut info = crate::engine::ftp::probe_ftp(&body.url)
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
        if agent {
            info.file_name = sanitize_untrusted_text(&info.file_name);
        }
        return Ok(Json(info));
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let headers = build_headers(
        body.cookies.as_deref(),
        body.referrer.as_deref(),
        body.user_agent.as_deref(),
        body.headers.as_ref(),
    );

    let mut info = probe_url(&client, &body.url, &headers)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    if agent {
        info.file_name = sanitize_untrusted_text(&info.file_name);
        info.final_url = sanitize_untrusted_text(&info.final_url);
        info.etag = info.etag.map(|s| sanitize_untrusted_text(&s));
    }

    Ok(Json(info))
}

pub async fn set_speed_limit_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Json(body): Json<SpeedLimitReq>,
) -> Result<StatusCode, (StatusCode, String)> {
    enforce_agent_policy(&state, &req_headers, "set_speed_limit", None, None, None).await?;
    let limit_bps = body
        .limit_bps
        .or_else(|| body.kbps.map(|k| k * 1024))
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing limit_bps or kbps".to_string()))?;
    state.manager.set_global_speed_limit(limit_bps).await;
    Ok(StatusCode::OK)
}

pub async fn list_queues_handler(
    State(state): State<ApiState>,
) -> Result<Json<Vec<QueueRecord>>, (StatusCode, String)> {
    let conn = state.manager.get_db_conn().await;
    let queues = list_queues(&conn).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(queues))
}

pub async fn ws_events_handler(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

#[derive(Debug, Deserialize)]
pub struct ApiMediaDownloadReq {
    pub url: String,
    pub format_id: Option<String>,
    pub audio_only: Option<bool>,
    pub save_dir: Option<String>,
    pub file_name: Option<String>,
    pub cookies: Option<String>,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub concurrency: Option<u8>,
    #[serde(default)]
    pub prompt_user: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ApiMediaDownloadResp {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub prompted: bool,
}

pub async fn media_probe_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Json(body): Json<ProbeReq>,
) -> Result<Json<crate::media::ytdlp::MediaProbeResult>, (StatusCode, String)> {
    enforce_agent_policy(&state, &req_headers, "media_probe", Some(&body.url), None, None).await?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let headers = build_headers(
        body.cookies.as_deref(),
        body.referrer.as_deref(),
        body.user_agent.as_deref(),
        body.headers.as_ref(),
    );

    let info = crate::media::ytdlp::probe_media(&client, &body.url, &headers)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    Ok(Json(info))
}

pub async fn media_download_handler(
    State(state): State<ApiState>,
    req_headers: HeaderMap,
    Json(body): Json<ApiMediaDownloadReq>,
) -> Result<(StatusCode, Json<ApiMediaDownloadResp>), (StatusCode, String)> {
    let validated_dir = enforce_agent_policy(
        &state,
        &req_headers,
        "media_download",
        Some(&body.url),
        body.save_dir.as_deref(),
        None,
    )
    .await?;

    let save_dir = validated_dir.unwrap_or_else(default_download_dir);

    let headers = build_headers(
        body.cookies.as_deref(),
        body.referrer.as_deref(),
        body.user_agent.as_deref(),
        body.headers.as_ref(),
    );

    if body.prompt_user.unwrap_or(false) && !is_agent_actor(&req_headers) {
        let prompt_id = format!("media-prompt-{}", uuid::Uuid::new_v4());
        state
            .manager
            .push_pending_prompt(crate::manager::PendingDownloadPrompt {
                prompt_id: prompt_id.clone(),
                url: body.url,
                file_name: body.file_name,
                save_dir: save_dir.to_string_lossy().to_string(),
                headers,
                segments_cfg: body.concurrency.map(|c| c as u32),
                speed_limit: None,
                is_media: true,
                format_id: body.format_id,
                audio_only: body.audio_only.unwrap_or(false),
            });
        return Ok((
            StatusCode::ACCEPTED,
            Json(ApiMediaDownloadResp {
                id: prompt_id,
                path: String::new(),
                prompted: true,
            }),
        ));
    }

    let req = crate::manager::MediaDownloadManagerReq {
        url: body.url,
        save_dir: save_dir.clone(),
        file_name: body.file_name,
        format_id: body.format_id,
        audio_only: body.audio_only.unwrap_or(false),
        queue_id: None,
        concurrency: body.concurrency,
        headers,
        start_now: true,
    };

    let id = state
        .manager
        .add_media_download(req)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    let _ = state.events_tx.send(serde_json::json!({
        "type": "download:added",
        "id": id
    }));

    Ok((
        StatusCode::CREATED,
        Json(ApiMediaDownloadResp {
            id,
            path: save_dir.to_string_lossy().to_string(),
            prompted: false,
        }),
    ))
}
async fn handle_ws(mut socket: WebSocket, state: ApiState) {
    let mut rx = state.events_tx.subscribe();
    while let Ok(ev) = rx.recv().await {
        if let Ok(text) = serde_json::to_string(&ev) {
            if socket.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    }
}
