use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

use crate::client::{request, url_encode_component};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

pub fn tool_definitions() -> Value {
    json!([
        {
            "name": "add_download",
            "description": "Tambah unduhan ke TurboGet",
            "inputSchema": {
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": { "type": "string" },
                    "save_dir": { "type": "string" },
                    "category": { "type": "string" },
                    "segments": { "type": "integer", "minimum": 1, "maximum": 32 },
                    "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                    "start_now": { "type": "boolean", "default": true }
                }
            }
        },
        {
            "name": "list_downloads",
            "description": "Daftar unduhan",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string" },
                    "query": { "type": "string" },
                    "limit": { "type": "integer" }
                }
            }
        },
        {
            "name": "get_download",
            "description": "Detail & progres satu unduhan",
            "inputSchema": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string" }
                }
            }
        },
        {
            "name": "pause_download",
            "description": "Jeda satu unduhan aktif",
            "inputSchema": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string" }
                }
            }
        },
        {
            "name": "resume_download",
            "description": "Lanjutkan satu unduhan terjeda",
            "inputSchema": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string" }
                }
            }
        },
        {
            "name": "cancel_download",
            "description": "Batalkan unduhan",
            "inputSchema": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string" },
                    "delete_file": { "type": "boolean", "default": false }
                }
            }
        },
        {
            "name": "probe_url",
            "description": "Cek ukuran, nama, dukungan resume tanpa mengunduh",
            "inputSchema": {
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": { "type": "string" }
                }
            }
        },
        {
            "name": "set_speed_limit",
            "description": "Atur batas kecepatan global dalam KB/s (0 = tanpa batas)",
            "inputSchema": {
                "type": "object",
                "required": ["kbps"],
                "properties": {
                    "kbps": { "type": "integer", "minimum": 0 }
                }
            }
        }
    ])
}

fn call_tool(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "add_download" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required argument 'url'".to_string())?;
            let payload = json!({
                "url": url,
                "save_dir": args.get("save_dir").and_then(|v| v.as_str()),
                "category_id": args.get("category").and_then(|v| v.as_str()),
                "segments": args.get("segments").and_then(|v| v.as_u64()),
                "headers": args.get("headers"),
                "start_now": args.get("start_now").and_then(|v| v.as_bool()).unwrap_or(true),
            });
            request("POST", "/v1/downloads", Some(&payload)).map_err(|e| e.message())
        }
        "list_downloads" => {
            let mut params = Vec::new();
            if let Some(s) = args.get("status").and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    params.push(format!("status={}", url_encode_component(s)));
                }
            }
            if let Some(q) = args.get("query").and_then(|v| v.as_str()) {
                if !q.is_empty() {
                    params.push(format!("q={}", url_encode_component(q)));
                }
            }
            if let Some(lim) = args.get("limit").and_then(|v| v.as_u64()) {
                params.push(format!("limit={}", lim));
            }
            let path = if params.is_empty() {
                "/v1/downloads".to_string()
            } else {
                format!("/v1/downloads?{}", params.join("&"))
            };
            request("GET", &path, None).map_err(|e| e.message())
        }
        "get_download" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required argument 'id'".to_string())?;
            let path = format!("/v1/downloads/{}", url_encode_component(id));
            request("GET", &path, None).map_err(|e| e.message())
        }
        "pause_download" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required argument 'id'".to_string())?;
            let path = format!("/v1/downloads/{}/pause", url_encode_component(id));
            request("POST", &path, None).map_err(|e| e.message())
        }
        "resume_download" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required argument 'id'".to_string())?;
            let path = format!("/v1/downloads/{}/resume", url_encode_component(id));
            request("POST", &path, None).map_err(|e| e.message())
        }
        "cancel_download" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required argument 'id'".to_string())?;
            let delete_file = args
                .get("delete_file")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let path = format!("/v1/downloads/{}/cancel", url_encode_component(id));
            request("POST", &path, Some(&json!({ "delete_file": delete_file })))
                .map_err(|e| e.message())
        }
        "probe_url" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required argument 'url'".to_string())?;
            request("POST", "/v1/probe", Some(&json!({ "url": url }))).map_err(|e| e.message())
        }
        "set_speed_limit" => {
            let kbps = args
                .get("kbps")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "Missing required argument 'kbps'".to_string())?;
            request("PUT", "/v1/settings/speed-limit", Some(&json!({ "kbps": kbps })))
                .map_err(|e| e.message())
        }
        unknown => Err(format!("Unknown tool '{}'", unknown)),
    }
}

pub fn handle_rpc_line(line: &str) -> Option<String> {
    let req: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            let err_resp = JsonRpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("Parse error: {}", e),
                }),
            };
            return serde_json::to_string(&err_resp).ok();
        }
    };

    // Notifications without `id` do not produce a response
    let id = req.id?;

    let response = match req.method.as_str() {
        "initialize" => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "turboget",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
            error: None,
        },
        "ping" => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({})),
            error: None,
        },
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "tools": tool_definitions()
            })),
            error: None,
        },
        "tools/call" => {
            let params = req.params.unwrap_or_else(|| json!({}));
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

            let tool_res = match call_tool(tool_name, &args) {
                Ok(data) => json!({
                    "content": [
                        {
                            "type": "text",
                            "text": serde_json::to_string(&data).unwrap_or_default()
                        }
                    ],
                    "isError": false
                }),
                Err(err_msg) => json!({
                    "content": [
                        {
                            "type": "text",
                            "text": err_msg
                        }
                    ],
                    "isError": true
                }),
            };

            JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(tool_res),
                error: None,
            }
        }
        other => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", other),
            }),
        },
    };

    serde_json::to_string(&response).ok()
}

pub fn run_mcp_stdio() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Some(resp_json) = handle_rpc_line(trimmed) {
                    if writeln!(writer, "{}", resp_json).is_err() || writer.flush().is_err() {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
}
