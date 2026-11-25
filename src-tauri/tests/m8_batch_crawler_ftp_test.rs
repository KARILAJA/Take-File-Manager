use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use turboget_lib::engine::ftp::{probe_ftp, FtpDownloader};
use turboget_lib::engine::single::DownloadOutcome;
use turboget_lib::engine::test_server::compute_expected_hash;
use turboget_lib::manager::state::DownloadStatus;
use turboget_lib::sitegrab::batch::{
    expand_url_pattern, export_downloads, import_downloads, parse_pasted_urls,
};
use turboget_lib::sitegrab::crawler::{crawl_site, CrawlerConfig};
use turboget_lib::storage::db::DownloadRecord;

#[test]
fn test_m8_batch_patterns_paste_and_import_export() {
    // 1. Pattern [001-100] produces exact 100 zero-padded URLs
    let urls = expand_url_pattern("http://x.com/img[001-100].jpg").unwrap();
    assert_eq!(urls.len(), 100);
    assert_eq!(urls[0], "http://x.com/img001.jpg");
    assert_eq!(urls[9], "http://x.com/img010.jpg");
    assert_eq!(urls[99], "http://x.com/img100.jpg");

    // 2. Brace + nested numeric range
    let nested = expand_url_pattern("https://mirror.example.org/{us,eu,asia}/iso_[01-03].bin").unwrap();
    assert_eq!(nested.len(), 9);
    assert_eq!(nested[0], "https://mirror.example.org/us/iso_01.bin");
    assert_eq!(nested[8], "https://mirror.example.org/asia/iso_03.bin");

    // 3. Pasted URLs parsing & deduplication
    let pasted = parse_pasted_urls(
        "# batch list\n  https://a.com/1.zip  \nhttps://a.com/2.zip\nhttps://a.com/1.zip\nftp://b.com/f.iso\n",
    );
    assert_eq!(
        pasted,
        vec![
            "https://a.com/1.zip",
            "https://a.com/2.zip",
            "ftp://b.com/f.iso",
        ]
    );

    // 4. Export & Import roundtrip (TXT, CSV, JSON)
    let records = vec![DownloadRecord {
        id: "id-1".into(),
        url: "https://example.com/report,2026.pdf".into(),
        final_url: None,
        file_name: "report,2026.pdf".into(),
        save_dir: "C:/Downloads/Documents".into(),
        total_bytes: Some(2048),
        downloaded: 2048,
        status: DownloadStatus::Completed,
        error: None,
        resumable: true,
        etag: None,
        last_modified: None,
        category_id: Some("documents".into()),
        queue_id: Some("default".into()),
        priority: 0,
        segments_cfg: 8,
        speed_limit: None,
        headers_json: None,
        checksum_kind: None,
        checksum_value: None,
        created_at: 12345,
        started_at: None,
        completed_at: None,
    }];

    for fmt in ["txt", "csv", "json"] {
        let exported = export_downloads(&records, fmt).unwrap();
        let imported = import_downloads(&exported, fmt).unwrap();
        assert_eq!(imported.len(), 1, "format {} roundtrip count mismatch", fmt);
        assert_eq!(imported[0].url, "https://example.com/report,2026.pdf");
    }
}

async fn start_crawl_mock_server(routes: Vec<(&'static str, &'static str, &'static str)>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let routes: Vec<(String, String, String)> = routes
        .into_iter()
        .map(|(p, ct, body)| (p.to_string(), ct.to_string(), body.to_string()))
        .collect();

    tokio::spawn(async move {
        loop {
            if let Ok((mut socket, _)) = listener.accept().await {
                let r_list = routes.clone();
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = [0u8; 4096];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let path = req
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("/");

                    if let Some((_, ct, body)) = r_list.iter().find(|(p, _, _)| p == path) {
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            ct,
                            body.len(),
                            body
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                    } else {
                        let _ = socket
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                    }
                });
            }
        }
    });

    port
}

#[tokio::test]
async fn test_m8_crawler_respects_depth_domain_and_filters() {
    let external_port = start_crawl_mock_server(vec![
        ("/external.pdf", "application/pdf", "external"),
        ("/other_page.html", "text/html", "<a href=\"/leak.pdf\">leak</a>"),
    ])
    .await;

    // Box::leak purely for clean static string construction in local mock server routes
    let index_html: &'static str = Box::leak(
        format!(
            r#"<html><body>
                <a href="/files/doc0.pdf">Doc 0</a>
                <a href="/files/secret_doc.pdf">Secret Excluded</a>
                <a href="/files/installer.exe">Exe Filtered Out</a>
                <a href="/sub/page1.html">Level 1 Page</a>
                <a href="http://localhost:{}/external.pdf">Cross Domain PDF</a>
                <a href="http://localhost:{}/other_page.html">Cross Domain Page</a>
            </body></html>"#,
            external_port, external_port
        )
        .into_boxed_str(),
    );

    let page1_html = r#"<html><body>
        <a href="/files/archive1.zip">Archive Level 1</a>
        <a href="/sub/page2.html">Level 2 Page</a>
    </body></html>"#;

    let page2_html = r#"<html><body>
        <a href="/files/too_deep2.zip">Too Deep Level 2</a>
    </body></html>"#;

    let port = start_crawl_mock_server(vec![
        ("/index.html", "text/html", index_html),
        ("/sub/page1.html", "text/html", page1_html),
        ("/sub/page2.html", "text/html", page2_html),
        ("/files/doc0.pdf", "application/pdf", "pdfbytes"),
        ("/files/secret_doc.pdf", "application/pdf", "secret"),
        ("/files/installer.exe", "application/octet-stream", "exe"),
        ("/files/archive1.zip", "application/zip", "zipbytes"),
        ("/files/too_deep2.zip", "application/zip", "deep"),
    ])
    .await;

    let client = reqwest::Client::new();
    let cfg = CrawlerConfig {
        start_url: format!("http://127.0.0.1:{}/index.html", port),
        max_depth: 1,
        allowed_domains: vec!["127.0.0.1".to_string()], // Excludes `localhost`
        include_subdomains: false,
        file_extensions: vec!["pdf".to_string(), "zip".to_string()],
        include_patterns: vec![],
        exclude_patterns: vec!["*secret*".to_string()],
        max_pages: Some(50),
    };

    let items = crawl_site(&client, cfg, Arc::new(AtomicBool::new(false)))
        .await
        .expect("crawl_site failed");

    let urls: Vec<&str> = items.iter().map(|i| i.url.as_str()).collect();
    println!("Crawled URLs: {:?}", urls);

    assert!(urls.iter().any(|u| u.ends_with("/files/doc0.pdf")), "Depth 0 file missing");
    assert!(urls.iter().any(|u| u.ends_with("/files/archive1.zip")), "Depth 1 file missing");
    assert!(!urls.iter().any(|u| u.ends_with("/files/too_deep2.zip")), "Depth 2 file exceeded max_depth=1!");
    assert!(!urls.iter().any(|u| u.ends_with("secret_doc.pdf")), "Exclude pattern failed!");
    assert!(!urls.iter().any(|u| u.ends_with("installer.exe")), "Extension filter failed!");
    assert!(!urls.iter().any(|u| u.contains("localhost")), "Cross-domain URL leaked through boundary!");
}

/// Minimal RFC 959 / 2428 in-process FTP test server supporting SIZE, FEAT, PASV, EPSV, REST, RETR
async fn start_mock_ftp_server(total_bytes: u64, seen_rest_offset: Arc<AtomicU64>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let rest_tracker = seen_rest_offset.clone();
            tokio::spawn(async move {
                let (read_half, mut write_half) = socket.into_split();
                let mut reader = BufReader::new(read_half);
                let _ = write_half.write_all(b"220 MockFTP Ready\r\n").await;

                let mut pending_data_listener: Option<TcpListener> = None;
                let mut restart_offset = 0u64;
                let mut line = String::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    let cmd_line = line.trim();
                    let (cmd, arg) = match cmd_line.split_once(' ') {
                        Some((c, a)) => (c.to_uppercase(), a.trim().to_string()),
                        None => (cmd_line.to_uppercase(), String::new()),
                    };

                    match cmd.as_str() {
                        "USER" => {
                            let _ = write_half.write_all(b"331 Password required\r\n").await;
                        }
                        "PASS" => {
                            let _ = write_half.write_all(b"230 Logged in\r\n").await;
                        }
                        "TYPE" => {
                            let _ = write_half.write_all(b"200 Type set to I\r\n").await;
                        }
                        "FEAT" => {
                            let _ = write_half
                                .write_all(b"211-Features:\r\n REST STREAM\r\n SIZE\r\n211 End\r\n")
                                .await;
                        }
                        "SIZE" => {
                            let resp = format!("213 {}\r\n", total_bytes);
                            let _ = write_half.write_all(resp.as_bytes()).await;
                        }
                        "EPSV" => {
                            let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
                            let dp = dl.local_addr().unwrap().port();
                            pending_data_listener = Some(dl);
                            let resp = format!("229 Entering Extended Passive Mode (|||{}|)\r\n", dp);
                            let _ = write_half.write_all(resp.as_bytes()).await;
                        }
                        "PASV" => {
                            let dl = TcpListener::bind("127.0.0.1:0").await.unwrap();
                            let dp = dl.local_addr().unwrap().port();
                            pending_data_listener = Some(dl);
                            let resp = format!(
                                "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                                dp / 256,
                                dp % 256
                            );
                            let _ = write_half.write_all(resp.as_bytes()).await;
                        }
                        "REST" => {
                            if let Ok(off) = arg.parse::<u64>() {
                                restart_offset = off;
                                if off > 0 {
                                    rest_tracker.store(off, Ordering::SeqCst);
                                }
                                let resp = format!("350 Restarting at {}\r\n", off);
                                let _ = write_half.write_all(resp.as_bytes()).await;
                            } else {
                                let _ = write_half.write_all(b"501 Invalid REST offset\r\n").await;
                            }
                        }
                        "RETR" => {
                            let Some(dl) = pending_data_listener.take() else {
                                let _ = write_half.write_all(b"425 Use PASV or EPSV first\r\n").await;
                                continue;
                            };
                            let _ = write_half.write_all(b"150 Opening binary data connection\r\n").await;
                            let offset = restart_offset;
                            restart_offset = 0;

                            if let Ok((mut data_sock, _)) = dl.accept().await {
                                let mut cur = offset;
                                let chunk = 32 * 1024usize;
                                let mut aborted = false;
                                while cur < total_bytes {
                                    let to_send = (chunk as u64).min(total_bytes - cur) as usize;
                                    let mut buf = vec![0u8; to_send];
                                    for (i, b) in buf.iter_mut().enumerate() {
                                        *b = ((cur + i as u64) % 251) as u8;
                                    }
                                    if data_sock.write_all(&buf).await.is_err() {
                                        aborted = true;
                                        break;
                                    }
                                    cur += to_send as u64;
                                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                                }
                                drop(data_sock);
                                if !aborted {
                                    let _ = write_half.write_all(b"226 Transfer complete\r\n").await;
                                }
                            }
                        }
                        "QUIT" => {
                            let _ = write_half.write_all(b"221 Goodbye\r\n").await;
                            break;
                        }
                        _ => {
                            let _ = write_half.write_all(b"502 Command not implemented\r\n").await;
                        }
                    }
                }
            });
        }
    });

    port
}

#[tokio::test]
async fn test_m8_ftp_probe_download_and_rest_resume() {
    let total_bytes: u64 = 512 * 1024; // 512 KB deterministic stream
    let expected_hash = compute_expected_hash(total_bytes);
    let seen_rest_offset = Arc::new(AtomicU64::new(0));

    let port = start_mock_ftp_server(total_bytes, seen_rest_offset.clone()).await;
    let ftp_url = format!("ftp://testuser:testpass@127.0.0.1:{}/pub/sample.bin", port);

    // 1. Probe FTP
    let probe = probe_ftp(&ftp_url).await.expect("probe_ftp failed");
    assert_eq!(probe.total_bytes, Some(total_bytes));
    assert!(probe.resumable);
    assert_eq!(probe.file_name, "sample.bin");

    // 2. Partial download + cooperative pause
    let tmp = tempdir().unwrap();
    let dl = FtpDownloader::new(
        ftp_url.clone(),
        tmp.path().to_path_buf(),
        probe.file_name.clone(),
        probe.total_bytes,
    );

    let cancel_token = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel_token.clone();

    let outcome1 = dl
        .execute(cancel_token, move |p| {
            if p.downloaded_bytes >= 128 * 1024 {
                cancel_clone.store(true, Ordering::Relaxed);
            }
        })
        .await
        .expect("partial FTP download failed");

    match outcome1 {
        DownloadOutcome::Paused { downloaded_bytes, part_path } => {
            assert!(downloaded_bytes > 0 && downloaded_bytes < total_bytes);
            assert!(part_path.exists());
        }
        _ => panic!("Expected Paused outcome after mid-transfer cancel"),
    }

    // 3. Resume via FTP REST command and verify final SHA-256 matches
    let resume_token = Arc::new(AtomicBool::new(false));
    let outcome2 = dl
        .execute(resume_token, |_| {})
        .await
        .expect("resumed FTP download failed");

    assert!(
        seen_rest_offset.load(Ordering::SeqCst) > 0,
        "Mock FTP server must have received a non-zero REST offset on resume"
    );

    match outcome2 {
        DownloadOutcome::Completed {
            final_path,
            total_bytes: completed_bytes,
            sha256_hex,
        } => {
            assert_eq!(completed_bytes, total_bytes);
            assert_eq!(sha256_hex, expected_hash);
            assert!(final_path.exists());
        }
        _ => panic!("Expected Completed outcome on FTP resume"),
    }
}
