use librqbit::{
    create_torrent, spawn_utils::BlockingSpawner, AddTorrent, AddTorrentOptions, CreateTorrentOptions,
    ListenerMode, ListenerOptions, Session, SessionOptions,
};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use turboget_lib::engine::torrent::{is_torrent_url, probe_torrent};
use turboget_lib::manager::{state::DownloadStatus, DownloadManager, DownloadRequest};

fn sha256_file(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("read file");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}

#[tokio::test]
async fn test_torrent_probe_magnet_local_and_http_file() {
    let tmp = tempdir().unwrap();
    let source_file = tmp.path().join("ubuntu-sample.iso");
    let payload: Vec<u8> = (0usize..(64 * 1024)).map(|i| ((i * 17) % 251) as u8).collect();
    std::fs::write(&source_file, &payload).unwrap();

    let created = create_torrent(
        &source_file,
        CreateTorrentOptions {
            name: Some("ubuntu-sample.iso"),
            ..Default::default()
        },
        &BlockingSpawner::new(1),
    )
    .await
    .unwrap();
    let torrent_bytes = created.as_bytes().unwrap();

    let torrent_path = tmp.path().join("ubuntu.torrent");
    std::fs::write(&torrent_path, &torrent_bytes).unwrap();

    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    // 1. Magnet URL probe
    let mag = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Fedora+Workstation+41&xl=9876543";
    assert!(is_torrent_url(mag));
    let mag_probe = probe_torrent(&client, mag).await.unwrap();
    assert_eq!(mag_probe.file_name, "Fedora Workstation 41");
    assert_eq!(mag_probe.total_bytes, Some(9876543));

    // 2. Local .torrent file probe
    let local_str = torrent_path.to_string_lossy().to_string();
    assert!(is_torrent_url(&local_str));
    let local_probe = probe_torrent(&client, &local_str).await.unwrap();
    assert_eq!(local_probe.file_name, "ubuntu-sample.iso");
    assert_eq!(local_probe.total_bytes, Some(payload.len() as u64));

    // 3. HTTP .torrent URL probe
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let srv_bytes = torrent_bytes.clone();
    tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = vec![0u8; 2048];
            let _ = sock.read(&mut buf).await;
            let hdr = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-bittorrent\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                srv_bytes.len()
            );
            let _ = sock.write_all(hdr.as_bytes()).await;
            let _ = sock.write_all(&srv_bytes).await;
        }
    });

    let http_url = format!("http://127.0.0.1:{}/releases/ubuntu.torrent", port);
    assert!(is_torrent_url(&http_url));
    let http_probe = probe_torrent(&client, &http_url).await.unwrap();
    assert_eq!(http_probe.file_name, "ubuntu-sample.iso");
    assert_eq!(http_probe.total_bytes, Some(payload.len() as u64));
}

#[tokio::test]
async fn test_torrent_real_peer_download_through_download_manager() {
    let tmp = tempdir().unwrap();
    let seeder_dir = tmp.path().join("seeder");
    let leecher_dir = tmp.path().join("leecher");
    std::fs::create_dir_all(&seeder_dir).unwrap();
    std::fs::create_dir_all(&leecher_dir).unwrap();

    let source_file = seeder_dir.join("torrent_payload.bin");
    let payload: Vec<u8> = (0usize..(128 * 1024)).map(|i| ((i * 31 + 7) % 253) as u8).collect();
    std::fs::write(&source_file, &payload).unwrap();
    let expected_hash = sha256_file(&source_file);

    let created = create_torrent(
        &source_file,
        CreateTorrentOptions {
            name: Some("torrent_payload.bin"),
            ..Default::default()
        },
        &BlockingSpawner::new(1),
    )
    .await
    .unwrap();
    let torrent_bytes = created.as_bytes().unwrap();
    let torrent_file = tmp.path().join("payload.torrent");
    std::fs::write(&torrent_file, &torrent_bytes).unwrap();

    // Start offline loopback seeder session
    let seeder_session = Session::new_with_opts(
        seeder_dir.clone(),
        SessionOptions {
            dht: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            fastresume: false,
            ipv4_only: true,
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpOnly,
                listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
                ipv4_only: true,
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let seeder_handle = seeder_session
        .add_torrent(
            AddTorrent::from_bytes(torrent_bytes),
            Some(AddTorrentOptions {
                overwrite: true,
                output_folder: Some(seeder_dir.to_string_lossy().to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();

    seeder_handle.wait_until_completed().await.unwrap();

    let peer_addr = seeder_session
        .listen_addr()
        .expect("seeder should expose TCP listen addr");

    // Configure TurboGet DownloadManager offline BitTorrent session pointing at local seeder peer
    let manager = DownloadManager::new_in_memory().unwrap();
    manager
        .torrent_engine
        .configure_session_options(SessionOptions {
            dht: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            fastresume: false,
            ipv4_only: true,
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpOnly,
                listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
                ipv4_only: true,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await;
    manager
        .torrent_engine
        .configure_initial_peers(vec![peer_addr])
        .await;

    let id = manager
        .add_download(DownloadRequest {
            url: torrent_file.to_string_lossy().to_string(),
            save_dir: leecher_dir.clone(),
            file_name: None,
            category_id: None,
            queue_id: None,
            segments_cfg: None,
            speed_limit: None,
            headers: vec![],
            start_now: true,
        })
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let rec = manager.get_download(&id).await.unwrap().unwrap();
        if rec.status == DownloadStatus::Completed {
            assert_eq!(rec.file_name, "torrent_payload.bin");
            assert_eq!(rec.category_id.as_deref(), Some("torrent"));
            let out_file = Path::new(&rec.save_dir).join(&rec.file_name);
            assert!(out_file.exists(), "missing torrent output file: {:?}", out_file);
            assert_eq!(sha256_file(&out_file), expected_hash);
            break;
        }
        assert_ne!(rec.status, DownloadStatus::Failed, "torrent download failed: {:?}", rec.error);
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for torrent download completion"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn test_torrent_real_magnet_link_peer_download_and_default_session_init() {
    // 1. Verify default production SessionOptions (DHT + TCP/uTP listener) initializes cleanly on host OS
    let tmp = tempdir().unwrap();
    let default_engine = turboget_lib::engine::torrent::TorrentEngine::new();
    let init_res = default_engine.get_or_init_session(tmp.path()).await;
    assert!(init_res.is_ok(), "default DHT/TCP/uTP torrent session failed to init: {:?}", init_res.err());

    // 2. Verify end-to-end magnet:?xt=urn:btih:... resolution & piece transfer from local peer
    let seeder_dir = tmp.path().join("magnet_seeder");
    let leecher_dir = tmp.path().join("magnet_leecher");
    std::fs::create_dir_all(&seeder_dir).unwrap();
    std::fs::create_dir_all(&leecher_dir).unwrap();

    let source_file = seeder_dir.join("magnet_sample.bin");
    let payload: Vec<u8> = (0usize..(64 * 1024)).map(|i| ((i * 19 + 5) % 251) as u8).collect();
    std::fs::write(&source_file, &payload).unwrap();
    let expected_hash = sha256_file(&source_file);

    let created = create_torrent(
        &source_file,
        CreateTorrentOptions {
            name: Some("magnet_sample.bin"),
            ..Default::default()
        },
        &BlockingSpawner::new(1),
    )
    .await
    .unwrap();
    let torrent_bytes = created.as_bytes().unwrap();
    let magnet_uri = format!("{}&dn=magnet_sample.bin", created.as_magnet());

    let seeder_session = Session::new_with_opts(
        seeder_dir.clone(),
        SessionOptions {
            dht: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            fastresume: false,
            ipv4_only: true,
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpOnly,
                listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
                ipv4_only: true,
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let seeder_handle = seeder_session
        .add_torrent(
            AddTorrent::from_bytes(torrent_bytes),
            Some(AddTorrentOptions {
                overwrite: true,
                output_folder: Some(seeder_dir.to_string_lossy().to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();
    seeder_handle.wait_until_completed().await.unwrap();
    let peer_addr = seeder_session.listen_addr().unwrap();

    let manager = DownloadManager::new_in_memory().unwrap();
    manager
        .torrent_engine
        .configure_session_options(SessionOptions {
            dht: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            fastresume: false,
            ipv4_only: true,
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpOnly,
                listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
                ipv4_only: true,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await;
    manager
        .torrent_engine
        .configure_initial_peers(vec![peer_addr])
        .await;

    let id = manager
        .add_download(DownloadRequest {
            url: magnet_uri,
            save_dir: leecher_dir.clone(),
            file_name: None,
            category_id: None,
            queue_id: None,
            segments_cfg: None,
            speed_limit: None,
            headers: vec![],
            start_now: true,
        })
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let rec = manager.get_download(&id).await.unwrap().unwrap();
        if rec.status == DownloadStatus::Completed {
            assert_eq!(rec.category_id.as_deref(), Some("torrent"));
            let out_file = Path::new(&rec.save_dir).join(&rec.file_name);
            assert!(out_file.exists(), "missing magnet output file: {:?}", out_file);
            assert_eq!(sha256_file(&out_file), expected_hash);
            break;
        }
        assert_ne!(rec.status, DownloadStatus::Failed, "magnet download failed: {:?}", rec.error);
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for magnet download completion"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
