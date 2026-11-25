use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use turboget_lib::api::server::start_local_api;
use turboget_lib::manager::DownloadManager;
use turboget_lib::media::ytdlp::{download_media, find_binary, probe_media, MediaDownloadRequest};

async fn start_static_http_server(root_dir: PathBuf) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            if let Ok((mut socket, _)) = listener.accept().await {
                let dir = root_dir.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let path_part = req
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("/")
                        .trim_start_matches('/');

                    let clean_rel = path_part.split('?').next().unwrap_or("");
                    let file_path = dir.join(clean_rel);

                    if !file_path.exists() || !file_path.is_file() {
                        let _ = socket
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                        return;
                    }

                    let data = std::fs::read(&file_path).unwrap();
                    let ct = if clean_rel.ends_with(".m3u8") {
                        "application/vnd.apple.mpegurl"
                    } else if clean_rel.ends_with(".mpd") {
                        "application/dash+xml"
                    } else if clean_rel.ends_with(".ts") {
                        "video/mp2t"
                    } else {
                        "video/mp4"
                    };

                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        ct,
                        data.len()
                    );
                    let _ = socket.write_all(header.as_bytes()).await;
                    let _ = socket.write_all(&data).await;
                });
            }
        }
    });

    port
}

fn inspect_streams_with_ffmpeg(ffmpeg_bin: &std::path::Path, file: &std::path::Path) -> (bool, bool) {
    let out = Command::new(ffmpeg_bin)
        .args(["-hide_banner", "-i"])
        .arg(file)
        .output()
        .expect("failed running ffmpeg inspect");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let has_video = stderr.contains("Video:");
    let has_audio = stderr.contains("Audio:");
    (has_video, has_audio)
}

#[tokio::test]
async fn test_m7_hls_parallel_download_and_ffmpeg_audio_video_merge() {
    let ffmpeg_bin = find_binary("ffmpeg", "TURBOGET_FFMPEG_PATH")
        .expect("ffmpeg must be installed to run M7 video grabber test");

    let srv_dir = tempdir().unwrap();
    let out_dir = tempdir().unwrap();

    // Generate synthetic self-owned test video (2s, 320x240) as HLS .ts segments
    let vid_status = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=320x240:rate=25",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-hls_time",
            "1",
            "-hls_list_size",
            "0",
            "-hls_segment_filename",
        ])
        .arg(srv_dir.path().join("v_%d.ts"))
        .arg(srv_dir.path().join("video.m3u8"))
        .output()
        .unwrap();
    assert!(
        vid_status.status.success(),
        "ffmpeg video gen failed: {}",
        String::from_utf8_lossy(&vid_status.stderr)
    );

    // Generate synthetic self-owned test audio (2s sine wave) as HLS .ts segments
    let aud_status = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-hls_time",
            "1",
            "-hls_list_size",
            "0",
            "-hls_segment_filename",
        ])
        .arg(srv_dir.path().join("a_%d.ts"))
        .arg(srv_dir.path().join("audio.m3u8"))
        .output()
        .unwrap();
    assert!(
        aud_status.status.success(),
        "ffmpeg audio gen failed: {}",
        String::from_utf8_lossy(&aud_status.stderr)
    );

    // Write master.m3u8 tying separate video and audio HLS playlists together
    let master_m3u8 = r#"#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aac",LANGUAGE="en",NAME="SineAudio",URI="audio.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=320x240,CODECS="avc1.42e01e,mp4a.40.2",AUDIO="aac"
video.m3u8
"#;
    std::fs::write(srv_dir.path().join("master.m3u8"), master_m3u8).unwrap();

    let port = start_static_http_server(srv_dir.path().to_path_buf()).await;
    let master_url = format!("http://127.0.0.1:{}/master.m3u8", port);
    let client = reqwest::Client::new();

    // 1. Probe HLS stream via MediaProbe + LocalApi
    let probe = probe_media(&client, &master_url, &[]).await.expect("probe_media failed");
    assert_eq!(probe.extractor.as_deref(), Some("hls"));
    assert_eq!(probe.formats.len(), 1);
    assert_eq!(probe.formats[0].resolution.as_deref(), Some("320x240"));

    // Verify LocalApi /v1/media/probe endpoint
    let mgr = DownloadManager::new_in_memory().unwrap();
    let api = start_local_api(mgr, None).await.unwrap();
    let api_probe_resp = client
        .post(format!("http://127.0.0.1:{}/v1/media/probe", api.port))
        .header("Authorization", format!("Bearer {}", api.token))
        .json(&serde_json::json!({ "url": master_url }))
        .send()
        .await
        .unwrap();
    assert_eq!(api_probe_resp.status().as_u16(), 200);

    // 2. Download HLS video & audio segments in parallel and merge via ffmpeg
    let final_file = download_media(
        &client,
        MediaDownloadRequest {
            url: master_url,
            format_id: Some("hls-0".to_string()),
            audio_only: false,
            save_dir: out_dir.path().to_path_buf(),
            file_name: Some("merged_hls_test".to_string()),
            headers: vec![],
            concurrency: Some(4),
        },
    )
    .await
    .expect("download_media HLS failed");

    assert!(final_file.exists(), "Output MP4 was not created");
    assert!(std::fs::metadata(&final_file).unwrap().len() > 1000);

    // Verify merged MP4 actually contains BOTH video and audio streams
    let (has_video, has_audio) = inspect_streams_with_ffmpeg(&ffmpeg_bin, &final_file);
    assert!(has_video, "Merged HLS output missing video stream");
    assert!(has_audio, "Merged HLS output missing audio stream");
}

#[tokio::test]
async fn test_m7_dash_manifest_parallel_download_and_ffmpeg_merge() {
    let ffmpeg_bin = find_binary("ffmpeg", "TURBOGET_FFMPEG_PATH")
        .expect("ffmpeg must be installed to run M7 DASH test");

    let srv_dir = tempdir().unwrap();
    let out_dir = tempdir().unwrap();

    // Generate synthetic separate MP4 video track & M4A audio track
    let vid = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x240:d=1",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(srv_dir.path().join("video_track.mp4"))
        .output()
        .unwrap();
    assert!(vid.status.success());

    let aud = Command::new(&ffmpeg_bin)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=520:duration=1",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
        ])
        .arg(srv_dir.path().join("audio_track.m4a"))
        .output()
        .unwrap();
    assert!(aud.status.success());

    let mpd = r#"<?xml version="1.0" encoding="utf-8"?>
<MPD>
  <Period>
    <AdaptationSet mimeType="video/mp4" contentType="video">
      <Representation id="v240" bandwidth="400000" width="320" height="240" codecs="avc1.42e01e">
        <BaseURL>video_track.mp4</BaseURL>
      </Representation>
    </AdaptationSet>
    <AdaptationSet mimeType="audio/mp4" contentType="audio">
      <Representation id="a64" bandwidth="64000" codecs="mp4a.40.2">
        <BaseURL>audio_track.m4a</BaseURL>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;
    std::fs::write(srv_dir.path().join("manifest.mpd"), mpd).unwrap();

    let port = start_static_http_server(srv_dir.path().to_path_buf()).await;
    let mpd_url = format!("http://127.0.0.1:{}/manifest.mpd", port);
    let client = reqwest::Client::new();

    let probe = probe_media(&client, &mpd_url, &[]).await.unwrap();
    assert_eq!(probe.extractor.as_deref(), Some("dash"));
    assert_eq!(probe.formats.len(), 2);

    let merged_dash = download_media(
        &client,
        MediaDownloadRequest {
            url: mpd_url,
            format_id: Some("v240".to_string()),
            audio_only: false,
            save_dir: out_dir.path().to_path_buf(),
            file_name: Some("merged_dash_test".to_string()),
            headers: vec![],
            concurrency: Some(4),
        },
    )
    .await
    .expect("DASH download_media failed");

    assert!(merged_dash.exists());
    let (has_video, has_audio) = inspect_streams_with_ffmpeg(&ffmpeg_bin, &merged_dash);
    assert!(has_video, "Merged DASH output missing video stream");
    assert!(has_audio, "Merged DASH output missing audio stream");
}
