use anyhow::Result;
use axum::{
    middleware,
    routing::{get, post, put},
    Router,
};
use std::path::PathBuf;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use crate::api::auth::{auth_and_origin_guard, generate_token, write_api_json, ApiConnectionInfo, AuthConfig};
use crate::api::routes::{
    add_download_handler, cancel_download_handler, get_download_handler, list_downloads_handler,
    list_queues_handler, media_download_handler, media_probe_handler, pause_download_handler,
    probe_handler, refresh_url_handler, resume_download_handler, set_speed_limit_handler,
    ws_events_handler, ApiState,
};
use crate::manager::DownloadManager;

pub struct LocalApiHandle {
    pub port: u16,
    pub token: String,
    pub api_json_path: Option<PathBuf>,
    pub task: tokio::task::JoinHandle<()>,
}

/// Starts the Local REST + WebSocket API server bound strictly to 127.0.0.1 on an ephemeral port.
/// Writes `{ port, token, pid }` to `api_json_path` if provided.
pub async fn start_local_api(
    manager: DownloadManager,
    api_json_path: Option<PathBuf>,
) -> Result<LocalApiHandle> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let token = generate_token();

    if let Some(ref path) = api_json_path {
        let info = ApiConnectionInfo {
            port,
            token: token.clone(),
            pid: std::process::id(),
        };
        write_api_json(path, &info)?;
    }

    let (events_tx, _) = broadcast::channel(128);
    let state = ApiState {
        manager,
        events_tx,
        agent_limiter: std::sync::Arc::new(crate::manager::agent_policy::AgentRateLimiter::new()),
    };
    let auth_cfg = AuthConfig {
        token: token.clone(),
        port,
    };

    let app = Router::new()
        .route("/v1/downloads", post(add_download_handler).get(list_downloads_handler))
        .route("/v1/downloads/:id", get(get_download_handler))
        .route("/v1/downloads/:id/pause", post(pause_download_handler))
        .route("/v1/downloads/:id/resume", post(resume_download_handler))
        .route("/v1/downloads/:id/cancel", post(cancel_download_handler))
        .route("/v1/downloads/:id/refresh-url", post(refresh_url_handler))
        .route("/v1/probe", post(probe_handler))
        .route("/v1/media/probe", post(media_probe_handler))
        .route("/v1/media/download", post(media_download_handler))
        .route("/v1/settings/speed-limit", put(set_speed_limit_handler))
        .route("/v1/queues", get(list_queues_handler))
        .route("/v1/events", get(ws_events_handler))
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            auth_cfg,
            auth_and_origin_guard,
        ));

    let task = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("LocalApi server error: {}", e);
        }
    });

    Ok(LocalApiHandle {
        port,
        token,
        api_json_path,
        task,
    })
}
