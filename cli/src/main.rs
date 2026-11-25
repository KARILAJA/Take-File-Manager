mod client;
mod mcp;

use serde_json::json;
use std::collections::HashMap;
use std::process;

use client::{request, url_encode_component, ClientError};

fn print_usage() {
    eprintln!(
        "TurboGet CLI v{}\n\
         Usage:\n  \
           turboget add <url> [--dir D] [--category C] [--segments N] [--header \"K: V\"] [--later] [--json]\n  \
           turboget list [--status S] [--query Q] [--limit N] [--json]\n  \
           turboget status <id> [--json]\n  \
           turboget pause|resume|cancel <id> [--delete-file] [--json]\n  \
           turboget limit <KB/s> [--json]\n  \
           turboget probe <url> [--json]\n  \
           turboget mcp",
        env!("CARGO_PKG_VERSION")
    );
}

fn output_success(val: &serde_json::Value, json_mode: bool) -> i32 {
    if json_mode {
        println!("{}", serde_json::to_string_pretty(val).unwrap_or_else(|_| "{}".to_string()));
    } else if let Some(id) = val.get("id").and_then(|v| v.as_str()) {
        println!("{}", id);
    } else {
        println!("{}", serde_json::to_string(val).unwrap_or_default());
    }
    0
}

fn output_error(err: ClientError, json_mode: bool) -> i32 {
    let code = err.exit_code();
    let msg = err.message();
    if json_mode {
        eprintln!("{}", json!({ "error": msg, "exitCode": code }));
    } else {
        eprintln!("Error: {}", msg);
    }
    code
}

fn run_cli(args: &[String]) -> i32 {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_usage();
        return if args.is_empty() { 2 } else { 0 };
    }

    let json_mode = args.iter().any(|a| a == "--json");
    let sub = args[0].as_str();

    match sub {
        "mcp" => {
            mcp::run_mcp_stdio();
            0
        }
        "add" => {
            let mut url: Option<String> = None;
            let mut save_dir: Option<String> = None;
            let mut category: Option<String> = None;
            let mut segments: Option<u32> = None;
            let mut start_now = true;
            let mut headers = HashMap::new();

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {}
                    "--later" => start_now = false,
                    "--dir" => {
                        i += 1;
                        if i >= args.len() {
                            return 2;
                        }
                        save_dir = Some(args[i].clone());
                    }
                    "--category" => {
                        i += 1;
                        if i >= args.len() {
                            return 2;
                        }
                        category = Some(args[i].clone());
                    }
                    "--segments" => {
                        i += 1;
                        if i >= args.len() {
                            return 2;
                        }
                        match args[i].parse::<u32>() {
                            Ok(n) if (1..=32).contains(&n) => segments = Some(n),
                            _ => return 2,
                        }
                    }
                    "--header" => {
                        i += 1;
                        if i >= args.len() {
                            return 2;
                        }
                        if let Some((k, v)) = args[i].split_once(':') {
                            headers.insert(k.trim().to_string(), v.trim().to_string());
                        } else {
                            return 2;
                        }
                    }
                    arg if !arg.starts_with("--") && url.is_none() => {
                        url = Some(arg.to_string());
                    }
                    _ => return 2,
                }
                i += 1;
            }

            let Some(u) = url else {
                return 2;
            };

            let payload = json!({
                "url": u,
                "save_dir": save_dir,
                "category_id": category,
                "segments": segments,
                "headers": if headers.is_empty() { None } else { Some(headers) },
                "start_now": start_now,
            });

            match request("POST", "/v1/downloads", Some(&payload)) {
                Ok(v) => output_success(&v, json_mode),
                Err(e) => output_error(e, json_mode),
            }
        }
        "list" => {
            let mut status: Option<String> = None;
            let mut query: Option<String> = None;
            let mut limit: Option<usize> = None;

            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {}
                    "--status" => {
                        i += 1;
                        if i >= args.len() {
                            return 2;
                        }
                        status = Some(args[i].clone());
                    }
                    "--query" => {
                        i += 1;
                        if i >= args.len() {
                            return 2;
                        }
                        query = Some(args[i].clone());
                    }
                    "--limit" => {
                        i += 1;
                        if i >= args.len() {
                            return 2;
                        }
                        limit = match args[i].parse::<usize>() {
                            Ok(n) => Some(n),
                            Err(_) => return 2,
                        };
                    }
                    _ => return 2,
                }
                i += 1;
            }

            let mut qp = Vec::new();
            if let Some(s) = status {
                qp.push(format!("status={}", url_encode_component(&s)));
            }
            if let Some(q) = query {
                qp.push(format!("q={}", url_encode_component(&q)));
            }
            if let Some(l) = limit {
                qp.push(format!("limit={}", l));
            }
            let path = if qp.is_empty() {
                "/v1/downloads".to_string()
            } else {
                format!("/v1/downloads?{}", qp.join("&"))
            };

            match request("GET", &path, None) {
                Ok(v) => output_success(&v, json_mode),
                Err(e) => output_error(e, json_mode),
            }
        }
        "status" => {
            let id = args.iter().skip(1).find(|a| *a != "--json");
            let Some(dl_id) = id else {
                return 2;
            };
            let path = format!("/v1/downloads/{}", url_encode_component(dl_id));
            match request("GET", &path, None) {
                Ok(v) => output_success(&v, json_mode),
                Err(e) => output_error(e, json_mode),
            }
        }
        "pause" | "resume" | "cancel" => {
            let delete_file = args.iter().any(|a| a == "--delete-file");
            let id = args
                .iter()
                .skip(1)
                .find(|a| *a != "--json" && *a != "--delete-file");
            let Some(dl_id) = id else {
                return 2;
            };
            let path = format!("/v1/downloads/{}/{}", url_encode_component(dl_id), sub);
            let body = if sub == "cancel" {
                Some(json!({ "delete_file": delete_file }))
            } else {
                None
            };
            match request("POST", &path, body.as_ref()) {
                Ok(v) => output_success(&v, json_mode),
                Err(e) => output_error(e, json_mode),
            }
        }
        "limit" => {
            let kbps_arg = args.iter().skip(1).find(|a| *a != "--json");
            let Ok(kbps) = kbps_arg.ok_or(()).and_then(|s| s.parse::<u64>().map_err(|_| ())) else {
                return 2;
            };
            match request("PUT", "/v1/settings/speed-limit", Some(&json!({ "kbps": kbps }))) {
                Ok(v) => output_success(&v, json_mode),
                Err(e) => output_error(e, json_mode),
            }
        }
        "probe" => {
            let url = args.iter().skip(1).find(|a| *a != "--json");
            let Some(u) = url else {
                return 2;
            };
            match request("POST", "/v1/probe", Some(&json!({ "url": u }))) {
                Ok(v) => output_success(&v, json_mode),
                Err(e) => output_error(e, json_mode),
            }
        }
        _ => {
            print_usage();
            2
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = run_cli(&args);
    process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_invalid_args_exit_code_2() {
        assert_eq!(run_cli(&[]), 2);
        assert_eq!(run_cli(&["unknowncmd".to_string()]), 2);
        assert_eq!(run_cli(&["add".to_string()]), 2);
        assert_eq!(
            run_cli(&[
                "add".to_string(),
                "https://example.com/a.zip".to_string(),
                "--segments".to_string(),
                "99".to_string()
            ]),
            2
        );
    }

    #[test]
    fn test_mcp_tools_list_matches_spec_8_tools() {
        let req = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let resp_str = mcp::handle_rpc_line(req).unwrap();
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            vec![
                "add_download",
                "list_downloads",
                "get_download",
                "pause_download",
                "resume_download",
                "cancel_download",
                "probe_url",
                "set_speed_limit"
            ]
        );
    }
}
