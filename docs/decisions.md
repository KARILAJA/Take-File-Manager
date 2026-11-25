# Architectural Decisions Log

## ADR 0001: M0 Initial Project Scaffold
- **Date**: 2026-11-24
- **Context**: Setting up TurboGet based on SPEC.md.
- **Decision**:
  - Desktop framework: Tauri 2 (Rust)
  - Frontend: React 18 + Vite + TypeScript + Tailwind CSS
  - SQLite for persistent storage with embedded initial schema migrations (`rusqlite` bundled)
  - Architecture adheres strictly to SPEC.md Section 3 and Section 11.

## ADR 0002: M1 Single-Stream Engine & Mock Test Server
- **Date**: 2026-11-24
- **Context**: Implementing Milestone M1 (Probe, single-stream download to `.part`, pause/resume, SHA-256 verification, and test server).
- **Decision**:
  - `probe_url`: Issues `HEAD` request with fallback to `GET Range: bytes=0-0` to inspect `Content-Length`, `Accept-Ranges`, `ETag`, and `Content-Disposition`.
  - `filename`: RFC 5987 / RFC 6266 `filename*` and `filename` parser with OS path traversal sanitization.
  - `SingleStreamDownloader`:
    - Downloads chunked streams directly into `<filename>.part`.
    - Handles resume via `Range: bytes={existing_bytes}-` and `If-Range: {etag}` with HTTP 206 Partial Content detection.
    - Supports cooperative pause via atomic cancel tokens.
    - Computes streaming SHA-256 and atomically renames `.part` upon completion.
  - `test-server`:
    - Deterministic generator with zero RAM bloat (`(offset + i) % 251`).
    - Configurable connection drops (`set_drop_after`) and per-chunk delays to simulate real-world intermittent network interruptions.
    - Verified via automated integration tests (`m1_resume_test.rs`).

## ADR 0003: M2 Multi-Segment & Dynamic Segmentation
- **Date**: 2026-11-24
- **Context**: Implementing Milestone M2 (parallel segmentation, dynamic split, disk pre-allocation, concurrent offset writes, fallback on missing Range).
- **Decision**:
  - Disk writes: `write_all_at` using OS file extensions (`seek_write` on Windows, `write_all_at` on Unix) allowing concurrent writes from multiple worker tasks to an `Arc<File>` without mutex locks on disk I/O.
  - Disk pre-allocation: Full size pre-allocated via `set_len` on the `.part` file to guarantee disk space availability prior to downloading.
  - `SegmentManager`:
    - Tracks segment bounds `[start_byte..=end_byte]`, `written`, `completed`, and worker assignment.
    - Dynamic Split: When an idle worker requests work, the manager selects the segment with the largest remaining bytes (`remaining >= 2 * min_segment_size`), bisects it at `split_at`, shortens the original segment's `end_byte` to `split_at - 1`, and creates a new segment for the idle worker starting at `split_at`.
    - Active worker bounds clamping: Active streaming workers clamp write chunks to `end_byte - current_offset + 1` so that dynamic splits never cause data overwrites.
    - Property tests: Mathematically verified invariant continuity (no gaps, no overlaps, contiguous `0..=total_size-1`) across hundreds of mutations.
  - Fallback: Transparent fallback to `SingleStreamDownloader` when server reports no Range support (`Accept-Ranges: none` or missing) or files < 1 MB.
  - Speedup benchmark: Verified with per-connection throttled test server that 8 segments achieves >= 5x speedup over 1 segment with identical SHA-256 hash.

## ADR 0004: M3 State Machine, Checkpointing & Crash Recovery
- **Date**: 2026-11-24
- **Context**: Implementing Milestone M3 (State machine, error classification, 1-2s database checkpointing, session crash/kill recovery).
- **Decision**:
  - `DownloadStatus` state machine:
    - Enforces valid lifecycle transitions: `Queued -> Probing -> Downloading -> Verifying -> Completed`.
    - Terminal states (`Completed`, `Canceled`) reject invalid resumes.
    - Transient states (`Downloading`, `Probing`, `Verifying`) transition to `Paused` upon abnormal session termination.
  - Error classification (`DownloadErrorKind`):
    - HTTP 403 / 410 classified as `LinkExpired`, moving download to `WaitingRefresh` status while preserving all progress on disk.
    - `refresh_url` enables updating the link address and resuming without data loss.
    - HTTP 416 classified as `RangeNotSatisfiable`.
    - HTTP 429 / 503 classified as `RateLimited`.
  - SQLite persistence & Checkpointing:
    - `checkpoint_download`: Atomic transaction updating downloaded bytes and slice progress (`segments` table) every 1 second.
    - `recover_interrupted_downloads`: Scans for incomplete downloads from previous abnormal process termination and marks them `Paused` with checkpoint progress preserved (maximum progress lost <= 1-2 seconds).
    - Verified via automated crash simulation test `test_m3_crash_and_kill_recovery_from_checkpoint`.


## ADR 0005: M4 Main Desktop UI
- **Date**: 2026-11-24
- **Context**: Implementing Milestone M4 (Download list, Add URL dialog with live probe, detail modal with segment visualization & speed chart, status/category filtering, i18n ID/EN, dark/light themes).
- **Decision**:
  - React + Tailwind + Zustand store architecture:
    - Global store in `src/store/index.ts` connecting React state to Tauri IPC commands (`tauriInvoke`) with browser mock fallback for preview/testing environments.
    - Periodic 1-second state synchronization for active downloads and speed metrics history.
  - UI Component Hierarchy:
    - `Toolbar`: Action controls (Add URL, Pause, Resume, Delete, Open Folder, Open File), Dark/Light theme toggle, ID/EN language selector.
    - `Sidebar`: Filter by download status (`All`, `Downloading`, `Completed`, `Paused/Failed`) and auto-categorization (`Compressed`, `Documents`, `Music`, `Programs`, `Video`, `Images`, `Others`) with dynamic item counters.
    - `DownloadList`: Data table with search query filtering, multi-column sorting (name, size, status, progress, speed, ETA, date), progress bars, and status badges.
    - `AddUrlModal`: Interactive URL input with asynchronous probing (`probe_url`), file name detection, save folder customization, and segment count configuration.
    - `DownloadDetailModal`: Real-time segment visualization progress bars (colored by segment state), SVG speed history graph, and transfer metadata.
    - `DownloadCompletedModal`: Post-download completion dialog with quick actions to open the downloaded file or reveal its folder in the OS file explorer.
  - Internationalization & Themes:
    - Lightweight key-based dictionary in `src/i18n/` supporting Indonesian (`id`) and English (`en`).
    - Class-based dark and light theme switching via Tailwind CSS.
  - Verification:
    - TypeScript compilation and Vite production build verified with zero errors.


## ADR 0006: M5 Queues, Scheduler, Token Bucket Rate Limiter, Categories & System Integration
- **Date**: 2026-11-24
- **Context**: Implementing Milestone M5 (Antrean, penjadwal, limiter, kategori, system tray/autostart/power actions).
- **Decision**:
  - `RateLimiter` (Token Bucket Algorithm):
    - Configurable token bucket with capped burst capacity (`0.25s` burst) preventing initial transfer rate spikes.
    - Asynchronous cooperative token acquisition (`acquire(bytes).await`) integrated directly into chunk reading streams for both multi-segment and single-stream downloaders.
    - Measured accuracy verified at 500 KB/s target with empirical speed within ±10% tolerance (460 - 560 KB/s) on deterministic benchmark stream.
  - `QueueManager` & Concurrency Promotion:
    - Custom & default queues with configurable `max_concurrent` limits (default: 3).
    - Multi-priority download ordering (`priority DESC, created_at ASC`).
    - Automatic slot promotion: when active downloads transition to `Completed`, `Paused`, or `Failed`, queued items in that queue are automatically promoted to active downloading state without exceeding `max_concurrent`.
  - `Scheduler` & Lifecycle Automation:
    - 7-day bitmask representation (`days_mask`: bits 0..6 for Mon..Sun) with "HH:MM" 24h start/stop time triggers.
    - Post-completion actions (`after_action`): `none`, `exit`, `shutdown`, `sleep`, `hibernate`, `run`.
    - Executed gracefully on Windows (`shutdown /s /t 60`, `powrprof.dll,SetSuspendState`, etc.).
  - Auto-Category & File Extension Resolution:
    - Rule-based file categorization based on standard MIME/extension dictionaries (`compressed`, `documents`, `music`, `programs`, `video`, `images`, `others`).
    - Destination directory auto-resolution mapping unassigned downloads to category subdirectories.
  - Verification:
    - Automated integration tests in `tests/m5_queue_scheduler_limiter_test.rs` (4/4 tests passed).
    - Full project test suite passes (31/31 unit & integration tests).
    - Zero compiler or clippy warnings.


## ADR 0007: M6 Browser Extension (MV3), Native Messaging Host & Local REST/WS API
- **Date**: 2026-11-25
- **Context**: Implementing Milestone M6 (Ekstensi MV3 tangkap download + menu klik-kanan + cookie/referer, `turboget-host`, registrasi manifest host, LocalApi + Bearer token).
- **Decision**:
  - `LocalApi` (`src-tauri/src/api/`):
    - Implemented with `axum` (with WebSocket upgrade support) bound strictly to `127.0.0.1:0` (ephemeral loopback port).
    - Connection metadata `{ port, token, pid }` written to `api.json` in `%LOCALAPPDATA%\turboget\` on startup.
    - Security guard middleware (`auth_and_origin_guard`): validates `Host` header is loopback, rejects web `Origin` headers (allows `chrome-extension://` and `moz-extension://` only) to prevent CSRF/DNS rebinding, and enforces constant-time `Authorization: Bearer <token>` verification (or `?token=` for `/v1/events` WebSocket).
    - Exposes `/v1/downloads`, `/v1/downloads/:id`, `/pause`, `/resume`, `/cancel`, `/refresh-url`, `/v1/probe`, `/v1/settings/speed-limit`, `/v1/queues`, and `/v1/events` WS stream.
  - Header & Cookie Persistence in `DownloadManager`:
    - Serializes HTTP headers (`Cookie`, `Referer`, `User-Agent`, custom headers) into `headers_json` in SQLite and forwards them to both `probe_url` and `MultiSegmentDownloader`/`SingleStreamDownloader`.
  - Native Messaging Host (`native-host/`, binary `turboget-host`):
    - Standalone dependency-light Rust binary using synchronous stdio 4-byte native-endian framing (`u32::from_ne_bytes`/`to_ne_bytes`) + JSON validation (`MAX_MESSAGE_BYTES = 1 MiB`) and loopback HTTP/1.1 client over `TcpStream`.
    - Validates URL schemes (`http://`, `https://`, `ftp://` only).
    - Supports `--register <EXTENSION_ID>` and `register-host.ps1` to generate `com.turboget.host.json` with strict `allowed_origins` and register Chrome/Edge/Brave keys in Windows HKCU Registry.
  - Browser Extension MV3 (`extension/`):
    - `background.js`: handles `chrome.downloads.onCreated` (rule matching by extension/min size/excluded hosts, cookie extraction via `chrome.cookies.getAll`, browser download cancellation + erasure, forwarding `{url, referrer, cookies, userAgent, fileName, mime}` via `chrome.runtime.sendNativeMessage`), `chrome.contextMenus` ("Download dengan TurboGet"), and `chrome.webRequest.onHeadersReceived` media header buffering per tab.
    - Includes `popup/popup.html` + `popup.js` and `options/options.html` + `options.js` for toggle, core connection status, and capture rules configuration.
  - Verification:
    - `native-host`: 2/2 unit tests passed (`cargo test`), 0 clippy warnings.
    - `src-tauri`: `m6_local_api_native_host_test` (2/2 passed — verified 401/403 security guards and real `turboget-host.exe` stdio invocation forwarding cookies to complete a cookie-protected download with SHA-256 verification) + full suite (33/33 backend unit + integration tests passed), 0 clippy warnings.
    - Frontend & Extension (`vitest`): 7/7 tests passed (`m6_extension.test.ts` verified download capture, `chrome.downloads.cancel`/`erase`, cookie forwarding, and exclusion filtering).


## ADR 0008: M7 Video Grabber (Sniffer, yt-dlp Sidecar, Parallel HLS/DASH + ffmpeg Merge)
- **Date**: 2026-11-25
- **Context**: Implementing Milestone M7 (sniffer `webRequest`, popup pilih kualitas, integrasi `yt-dlp` sidecar, pengunduhan HLS/DASH paralel + penggabungan audio/video dengan `ffmpeg`).
- **Decision**:
  - HLS Parser & Parallel Downloader (`src-tauri/src/media/hls.rs`):
    - Parses Master (`#EXT-X-STREAM-INF`, `#EXT-X-MEDIA:TYPE=AUDIO`) and Media (`.m3u8` segments + `#EXT-X-MAP`) playlists with relative URL resolution.
    - Downloads segments concurrently via bounded `Semaphore` concurrency (`download_urls_parallel_concat`) and assembles streams cleanly.
  - MPEG-DASH Parser & Parallel Downloader (`src-tauri/src/media/dash.rs`):
    - Parses `.mpd` `<AdaptationSet>`, `<Representation>`, `<BaseURL>`, `<SegmentList>`, and `<SegmentTemplate>` (`$Number$`, `$RepresentationID$`, `<SegmentTimeline>`).
    - Separates video and audio adaptation representations and downloads segments concurrently.
  - yt-dlp Sidecar + ffmpeg Merger (`src-tauri/src/media/ytdlp.rs`):
    - Discovers `yt-dlp` and `ffmpeg` via env overrides (`TURBOGET_YTDLP_PATH`, `TURBOGET_FFMPEG_PATH`), executable directory (Tauri sidecar folder), or `PATH`.
    - Routes `.m3u8` / `.mpd` streams to the native parallel engine and general page URLs to `yt-dlp --dump-json` / download flags (`-f`, `-x --audio-format mp3`).
    - Invokes `ffmpeg` (`-c copy -movflags +faststart`) to mux separate video and audio tracks into `.mp4` (or extract `.m4a`/`.mp3` for audio-only requests), cleaning up intermediate files via `TempWorkDir` RAII guard.
  - IPC, LocalApi, Native Messaging & Extension Sniffer:
    - Exposed `media_probe` & `media_download` in Tauri `commands.rs`, LocalApi `/v1/media/probe` & `/v1/media/download` (`routes.rs`), and `turboget-host`.
    - Enhanced `extension/background.js` (`detectMediaResponse`) and `extension/popup/popup.js` with live format probing dropdown ("Terbaik", "Audio Saja", parsed variants) and "Download video ini" action.
  - Verification:
    - `m7_video_grabber_test.rs`: 2/2 passed (synthesized self-owned HLS and DASH multi-track streams via `ffmpeg`, downloaded video + audio tracks in parallel, merged via `ffmpeg`, and verified both `Video:` and `Audio:` streams exist in output `.mp4`).
    - Full Cargo suite: 38/38 tests passed in `src-tauri`, 2/2 in `native-host`, 0 clippy warnings.
    - Vitest (`npm test`): 8/8 passed including `m7_sniffer.test.ts` verifying `webRequest` detection, deduplication, and tab badge counters.


## ADR 0009: M8 Batch Download, Site Grabber Crawler & FTP/FTPS with REST Resume
- **Date**: 2026-11-25
- **Context**: Implementing Milestone M8 (pola `[001-100]`, tempel banyak URL, impor/ekspor daftar, crawler situs dengan batas kedalaman & filter, serta dukungan protokol FTP/FTPS + resume via `REST`).
- **Decision**:
  - Batch Pattern Expansion, Paste & Import/Export (`src-tauri/src/sitegrab/batch.rs`):
    - `expand_url_pattern`: supports zero-padded/unpadded numeric ranges `[001-100]` & stepped ranges `[02-10:2]`, alphabetical ranges `[a-z]`, and brace alternatives `{a,b,c}` with Cartesian expansion capped safely at `MAX_EXPANDED_URLS = 10_000`.
    - `parse_pasted_urls`: normalizes multi-line input, ignores `#` comments/blank lines, validates schemes, and deduplicates preserving order.
    - `export_downloads` & `import_downloads`: full roundtrip support for `txt`, `csv` (RFC 4180 quote/comma escaping), and `json`.
  - Site Grabber Web Crawler (`src-tauri/src/sitegrab/crawler.rs`):
    - Async BFS crawler tracking visited URLs and recursion depth (`0..=max_depth`) with cooperative cancellation (`AtomicBool`).
    - Strictly enforces `allowed_domains` (defaulting to `start_url` host) and `include_subdomains` for both followed HTML pages and collected resource links.
    - Filters discovered assets via `file_extensions`, wildcard `include_patterns`, and `exclude_patterns`.
  - Async FTP/FTPS Engine (`src-tauri/src/engine/ftp.rs` using `suppaftp` v13 `tokio-rustls-ring` + `webpki-roots`):
    - Unified `AsyncRustlsFtpStream` handles plain `ftp://` and explicit TLS `ftps://` (`into_secure`), anonymous/authenticated URLs, binary transfer (`TYPE I`), `SIZE` & `FEAT` probing, cooperative pause, `RateLimiter` throttling, and offset resume via FTP `REST` (`resume_transfer`) into `.part` with SHA-256 verification.
    - Routed transparently in `DownloadManager::run_download`, `commands::probe_url`, and LocalApi `probe_handler` via `is_ftp_url`.
  - IPC Commands (`src-tauri/src/commands.rs` & `lib.rs`):
    - Exposed `expand_url_pattern`, `add_batch`, `add_from_pattern`, `export_list`, `import_list`, `grab_site_start`, and `grab_site_stop` (with `CrawlCancelState`).
  - Verification:
    - `m8_batch_crawler_ftp_test.rs`: 3/3 passed (verified `[001-100]` & brace pattern expansion, TXT/CSV/JSON roundtrip, BFS crawler depth limit + cross-domain boundary `127.0.0.1` vs `localhost` + extension/wildcard filters, and live RFC-959 mock FTP server pause + `REST` resume with SHA-256 verification).
    - Full Cargo suite (`src-tauri`): 46/46 tests passed (27 unit tests + 19 integration tests across M1, M2, M3, M5, M6, M7, M8); 0 clippy warnings.


## ADR 0010: M9 CLI (`turboget`), MCP Server (`turboget mcp`) & Agent Access Security Policy
- **Date**: 2026-11-25
- **Context**: Implementing Milestone M9 (CLI dengan `--json`, subcommand `mcp` dengan 8 tool Bagian 10.2, pengaturan "Akses Agent", allowlist folder, SSRF guard, rate limit, dan log audit).
- **Decision**:
  - Agent Access Policy & Security (`src-tauri/src/manager/agent_policy.rs` & `src-tauri/src/api/routes.rs`):
    - Policy stored persistently in SQLite `settings` table under key `agent_access_policy` with modes `Deny` (`Tolak`), `Ask` (`Tanya setiap kali`, default), and `Allowlist` (`Otomatis untuk folder diizinkan`).
    - When requests arrive from CLI/MCP agents (`X-TurboGet-Actor: agent`):
      - `AgentRateLimiter`: sliding 60-second window (default 30 req/min) returning HTTP `429 Too Many Requests` when exceeded.
      - `resolve_and_validate_agent_dir`: resolves symlinked ancestor directories, normalizes `.` / `..`, rejects relative paths and traversal, and checks Windows/POSIX path prefix containment strictly against `allowed_dirs`, returning HTTP `403 Forbidden` (`PERMISSION_DENIED`) for out-of-allowlist targets (e.g. `C:\Windows`).
      - `validate_agent_url`: blocks unsupported schemes and private/loopback/link-local SSRF targets when `block_private_ips` is enabled.
      - Audit trail: writes `AGENT_AUDIT` entries into SQLite `logs` table via `db::log_event` for all permitted, denied, and rate-limited agent actions.
      - Prompt-injection hygiene: strips control characters (`\r`, `\n`, null bytes) from untrusted metadata (`sanitize_untrusted_text`) before returning JSON to agents.
      - Prohibits post-download program execution hooks via agent API/MCP schemas.
  - CLI Binary & MCP Stdio Server (`cli/`):
    - Implemented `turboget` (`cli/src/main.rs`, `client.rs`, `mcp.rs`) with `--json` output across `add`, `list`, `status`, `pause`, `resume`, `cancel`, `limit`, `probe`, and `mcp`.
    - Enforces SPEC Section 10.1 exit codes: `0` (success), `2` (invalid args), `3` (TurboGet not running), `4` (permission denied).
    - `turboget mcp` implements stdio JSON-RPC 2.0 (`initialize`, `ping`, `tools/list`, `tools/call`) exposing all 8 SPEC Section 10.2 tools (`add_download`, `list_downloads`, `get_download`, `pause_download`, `resume_download`, `cancel_download`, `probe_url`, `set_speed_limit`).
  - IPC Commands (`src-tauri/src/commands.rs` & `lib.rs`):
    - Registered `agent_access_get`, `agent_access_set`, and `get_audit_logs`.
  - Verification:
    - `turboget-cli`: 2/2 unit tests passed, 0 clippy warnings.
    - `m9_cli_mcp_agent_policy_test.rs`: 3/3 passed (verified Deny/Ask/Allowlist/SSRF/rate-limit enforcement + audit logs, live `turboget mcp` stdio roundtrip rejecting out-of-allowlist folder and completing in-allowlist `add_download` + `get_download`, and CLI `--json` exit codes `0`/`3`/`4`).
    - Full `src-tauri` Cargo suite (`-- --test-threads=1`): 52/52 tests passed (30 unit tests + 22 integration tests across M1, M2, M3, M5, M6, M7, M8, M9); 0 clippy warnings.


## ADR 0011: M10 Polish, Security & Release Readiness
- **Date**: 2026-11-25
- **Context**: Implementing Milestone M10 (Polish & rilis: auto-update checker, Windows MSI/NSIS installer + Native Messaging host HKCU registry hooks, Authenticode code-signing docs, HTTP Proxy / Basic Auth, Mark-of-the-Web `Zone.Identifier` ADS, UI Log Viewer & Settings modal, user documentation, and sustained queue stability verification).
- **Decision**:
  - Network Settings, Proxy & Basic Auth (`src-tauri/src/engine/network.rs`):
    - Stores persistent `NetworkSettings` (`ProxyMode`: `System` / `Direct` / `Manual`, `proxy_url`, `proxy_username`, `proxy_password`, `max_connections_per_host`, `auto_update_enabled`, `update_feed_url`) under SQLite key `network_settings`.
    - `build_http_client` configures `reqwest::Proxy` (including `Proxy-Authorization` via `proxy.basic_auth`), connection pool limits (`pool_max_idle_per_host`), RFC 4648/7617 `encode_basic_auth` default `Authorization` headers, and per-download TLS override; `mask_secret` prevents plain-text credential leaks in UI/logs.
    - Integrated into `DownloadManager::run_download`.
  - Windows Mark-of-the-Web (`src-tauri/src/system/security.rs`):
    - `apply_mark_of_the_web` writes NTFS Alternate Data Stream `<file>:Zone.Identifier` (`[ZoneTransfer]`, `ZoneId=3`, `HostUrl=...`, optional `ReferrerUrl=...`) with `sanitize_ini_value` stripping control/newline characters (`\r`, `\n`) to prevent INI header injection while degrading gracefully on non-NTFS/non-Windows targets.
    - Invoked automatically upon download completion in `DownloadManager::run_download`, alongside writing an `INFO` audit entry in SQLite `logs`.
  - Auto-Update Checker (`src-tauri/src/system/updater.rs`):
    - Implements semver comparison (`parse_semver_tuple`, `is_newer_version`) and JSON feed fetching (`check_for_updates` returning `UpdateCheckResult`).
    - Exposed IPC commands `settings_get`, `settings_set`, and `check_app_update` in `src-tauri/src/commands.rs` and registered in `src-tauri/src/lib.rs`.
  - Installer, Release & User Documentation:
    - Configured NSIS (`currentUser` + `installerHooks`) and WiX MSI targets in `src-tauri/tauri.conf.json`.
    - Created `src-tauri/installer/windows-hooks.nsh` (`NSIS_HOOK_POSTINSTALL` & `NSIS_HOOK_PREUNINSTALL`) managing `com.turboget.host` registry entries in HKCU for Chrome, Edge, and Brave.
    - Added `docs/USER_GUIDE.md` and `docs/RELEASE.md` covering UI workflows, Chrome extension setup, CLI/MCP configuration, `signtool` Authenticode signing, and Windows 11 clean-VM verification steps.
  - UI Settings & Log Viewer (`src/components/SettingsLogsModal.tsx`, `src/store/index.ts`, `src/components/Toolbar.tsx`, `src/App.tsx`):
    - Added 3-tab modal ("Jaringan & Proxy", "Akses Agent (MCP/CLI)", "Log & Audit") accessible from the Toolbar settings button (`title="Pengaturan & Log"`).
  - Verification:
    - `src-tauri` Cargo suite (`cargo test -- --test-threads=1`): 59/59 passed (33 unit tests + 26 integration tests across M1–M10, including `m10_polish_security_test.rs` 4/4 covering NTFS `Zone.Identifier` MotW, HTTP 401 Basic Auth & authenticated manual HTTP proxy forwarding, mock auto-update JSON feed check, and sustained 8-item queued batch with SHA-256 verification and zero leftover `.part` files); `cargo clippy --lib --tests -- -D warnings` exited with code 0.
    - `native-host` (`2/2` unit tests) and `cli` (`2/2` unit tests) passed with 0 clippy warnings.
    - Frontend (`npm test` & `npm run build`): 4/4 test files (9/9 tests including `m10_settings_logs.test.tsx`) passed; `tsc && vite build` succeeded.
