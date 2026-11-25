import { create } from "zustand";
import { Language, TranslationKey, translate } from "../i18n";

export interface DownloadItem {
  id: string;
  url: string;
  final_url?: string;
  file_name: string;
  save_dir: string;
  total_bytes?: number;
  downloaded: number;
  status: string; // queued | probing | downloading | paused | verifying | completed | failed | waiting_refresh | canceled
  error?: string;
  resumable: boolean;
  etag?: string;
  category: string;
  progress_percent: number;
  speed_bps: number;
  eta_seconds?: number;
  created_at: number;
  completed_at?: number;
}

export interface SegmentItem {
  idx: number;
  start_byte: number;
  end_byte: number;
  written: number;
}

export interface ProbeResult {
  url: string;
  final_url: string;
  file_name: string;
  total_bytes?: number;
  resumable: boolean;
  etag?: string;
  last_modified?: string;
  content_type?: string;
}

export interface AddDownloadParams {
  url: string;
  save_dir?: string;
  file_name?: string;
  category_id?: string;
  queue_id?: string;
  segments_cfg?: number;
  speed_limit?: number;
  headers?: [string, string][];
  start_now?: boolean;
  is_media?: boolean;
  format_id?: string;
  audio_only?: boolean;
}

export interface PendingDownloadPrompt {
  prompt_id: string;
  url: string;
  file_name?: string;
  save_dir: string;
  headers: [string, string][];
  segments_cfg?: number;
  speed_limit?: number;
  is_media?: boolean;
  format_id?: string;
  audio_only?: boolean;
}

export interface MediaFormat {
  format_id: string;
  ext: string;
  resolution?: string;
  width?: number;
  height?: number;
  fps?: number;
  vcodec?: string;
  acodec?: string;
  filesize?: number;
  is_filesize_estimated?: boolean;
  tbr?: number;
  format_note?: string;
  protocol?: string;
  url?: string;
  has_video: boolean;
  has_audio: boolean;
}

export interface MediaProbeResult {
  title: string;
  url: string;
  thumbnail?: string;
  duration?: number;
  extractor?: string;
  formats: MediaFormat[];
}

export interface MediaToolsStatus {
  ytdlp_installed: boolean;
  ytdlp_path?: string;
  ffmpeg_installed: boolean;
  ffmpeg_path?: string;
}

export interface MediaDownloadParams {
  url: string;
  format_id?: string;
  audio_only?: boolean;
  save_dir?: string;
  file_name?: string;
  queue_id?: string;
  headers?: [string, string][];
  concurrency?: number;
}

export type SettingsTab = "network" | "browsers" | "appearance" | "categories" | "agent" | "logs";

export interface BrowserIntegrationInfo {
  id: string;
  name: string;
  installed: boolean;
  running: boolean;
  executable_path?: string;
  extension_installed: boolean;
  supports_developer_mode: boolean;
  supports_cli_autoload: boolean;
  coming_soon: boolean;
  status_label: string;
  extensions_url: string;
}

export interface BrowserIntegrateResult {
  browser_id: string;
  browser_name: string;
  extension_dir: string;
  extensions_url: string;
  launched: boolean;
  auto_loaded: boolean;
  needs_relaunch: boolean;
  message: string;
}

export interface NetworkSettings {
  proxy_mode: "system" | "direct" | "manual";
  proxy_url?: string;
  proxy_username?: string;
  proxy_password?: string;
  max_connections_per_host: number;
  auto_update_enabled: boolean;
  update_feed_url?: string;
  speed_limit_kbps: number;
}

export interface AgentAccessPolicy {
  mode: "deny" | "ask" | "allowlist";
  allowed_dirs: string[];
  block_private_ips: boolean;
  max_requests_per_minute: number;
}

export interface AuditLogEntry {
  id: number;
  ts: number;
  level: string;
  download_id?: string;
  message: string;
}

export interface UpdateCheckResult {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  notes?: string;
  download_url?: string;
}

export interface QueueRecord {
  id: string;
  name: string;
  max_concurrent: number;
  speed_limit?: number | null;
  enabled: boolean;
}

export interface ScheduleRecord {
  id: string;
  queue_id: string;
  days_mask: number;
  start_time?: string | null;
  stop_time?: string | null;
  after_action: string;
  after_command?: string | null;
  enabled: boolean;
}

export interface CategoryRecord {
  id: string;
  name: string;
  extensions: string;
  save_dir: string;
}

export interface CrawlerConfig {
  start_url: string;
  max_depth: number;
  allowed_domains: string[];
  include_subdomains: boolean;
  file_extensions: string[];
  include_patterns: string[];
  exclude_patterns: string[];
  max_pages?: number | null;
}

export interface CrawledItem {
  url: string;
  file_name: string;
  content_type?: string | null;
  content_length?: number | null;
  depth: number;
  source_page: string;
}

export interface BatchDownloadArgs {
  urls_or_text: string;
  save_dir?: string;
  category_id?: string;
  queue_id?: string;
  segments_cfg?: number;
  start_now?: boolean;
}

export interface FileChecksumResult {
  algorithm: string;
  hash: string;
  matches?: boolean | null;
  file_size: number;
  elapsed_ms: number;
}

export interface DownloadToast {
  id: string;
  downloadId: string;
  kind: "completed" | "failed";
  fileName: string;
  saveDir: string;
  totalBytes?: number;
  errorMessage?: string;
  createdAt: number;
}

export type ActiveView = "downloads" | "grabber" | "scheduler";

export const mockDownloads: DownloadItem[] = [];
const mockQueues: QueueRecord[] = [
  { id: "default", name: "Antrean Utama", max_concurrent: 3, speed_limit: null, enabled: true },
];
const mockSchedules: ScheduleRecord[] = [];
const mockCategories: CategoryRecord[] = [
  { id: "compressed", name: "Compressed", extensions: "zip,rar,7z,tar,gz,bz2,xz,iso,tgz", save_dir: "E:/Project/idm/downloads/Compressed" },
  { id: "documents", name: "Dokumen", extensions: "pdf,doc,docx,xls,xlsx,ppt,pptx,txt,rtf,odt,epub", save_dir: "E:/Project/idm/downloads/Documents" },
  { id: "music", name: "Musik", extensions: "mp3,wav,flac,aac,ogg,m4a,wma", save_dir: "E:/Project/idm/downloads/Music" },
  { id: "programs", name: "Program", extensions: "exe,msi,apk,dmg,pkg,deb,rpm,appimage", save_dir: "E:/Project/idm/downloads/Programs" },
  { id: "video", name: "Video", extensions: "mp4,mkv,avi,mov,wmv,flv,webm,m4v,ts", save_dir: "E:/Project/idm/downloads/Video" },
  { id: "images", name: "Gambar", extensions: "jpg,jpeg,png,gif,bmp,webp,svg,tiff,ico", save_dir: "E:/Project/idm/downloads/Images" },
  { id: "torrent", name: "Torrent", extensions: "torrent", save_dir: "E:/Project/idm/downloads/Torrent" },
  { id: "others", name: "Lainnya", extensions: "*", save_dir: "E:/Project/idm/downloads/Others" },
];

// Safely invoke Tauri commands with mock fallback for browser testing
export async function tauriInvoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (typeof window !== "undefined" && (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }

  // Browser Fallback Mock Implementation
  if (cmd === "probe_url") {
    const url = (args.url as string) || "";
    const name = url.split("/").pop() || "sample_file.zip";
    return {
      url,
      final_url: url,
      file_name: name.split("?")[0],
      total_bytes: 45 * 1024 * 1024,
      resumable: true,
      etag: "mock-etag-123",
      content_type: "application/octet-stream",
    } as unknown as T;
  }

  if (cmd === "add_download") {
    const params = (args.args as AddDownloadParams) || { url: "" };
    const id = "mock-" + Math.random().toString(36).substring(2, 9);
    const fileName = params.file_name || params.url.split("/").pop() || "file.bin";
    const item: DownloadItem = {
      id,
      url: params.url,
      file_name: fileName,
      save_dir: params.save_dir || "E:/Project/idm/downloads",
      total_bytes: 45 * 1024 * 1024,
      downloaded: params.start_now ? 5 * 1024 * 1024 : 0,
      status: params.start_now ? "downloading" : "paused",
      resumable: true,
      category:
        params.category_id ||
        (params.url.toLowerCase().startsWith("magnet:") || fileName.toLowerCase().endsWith(".torrent")
          ? "torrent"
          : fileName.endsWith(".mp3")
          ? "music"
          : fileName.endsWith(".zip")
          ? "compressed"
          : "others"),
      progress_percent: params.start_now ? 11.1 : 0,
      speed_bps: params.start_now ? 1048576 : 0,
      created_at: Date.now(),
    };
    mockDownloads.unshift(item);
    return id as unknown as T;
  }

  if (cmd === "list_downloads") {
    return [...mockDownloads] as unknown as T;
  }

  if (cmd === "delete_download") {
    const id = args.id as string;
    const idx = mockDownloads.findIndex((d) => d.id === id);
    if (idx !== -1) mockDownloads.splice(idx, 1);
    return {} as unknown as T;
  }

  if (cmd === "pause_download") {
    const id = args.id as string;
    const item = mockDownloads.find((d) => d.id === id);
    if (item) {
      item.status = "paused";
      item.speed_bps = 0;
    }
    return {} as unknown as T;
  }

  if (cmd === "pause_all_downloads") {
    let count = 0;
    mockDownloads.forEach((d) => {
      if (d.status === "downloading" || d.status === "probing" || d.status === "queued") {
        d.status = "paused";
        d.speed_bps = 0;
        count += 1;
      }
    });
    return count as unknown as T;
  }

  if (cmd === "resume_download") {
    const id = args.id as string;
    const item = mockDownloads.find((d) => d.id === id);
    if (item) item.status = "downloading";
    return {} as unknown as T;
  }

  if (cmd === "queue_list") {
    return [...mockQueues] as unknown as T;
  }
  if (cmd === "queue_create") {
    const q = args.queue as QueueRecord;
    mockQueues.push(q);
    return {} as unknown as T;
  }
  if (cmd === "queue_update") {
    const q = args.queue as QueueRecord;
    const i = mockQueues.findIndex((x) => x.id === q.id);
    if (i !== -1) mockQueues[i] = q;
    return {} as unknown as T;
  }
  if (cmd === "queue_delete") {
    const id = args.id as string;
    const i = mockQueues.findIndex((x) => x.id === id);
    if (i !== -1) mockQueues.splice(i, 1);
    return {} as unknown as T;
  }
  if (cmd === "schedule_list") {
    return [...mockSchedules] as unknown as T;
  }
  if (cmd === "schedule_create") {
    const s = args.schedule as ScheduleRecord;
    mockSchedules.push(s);
    return {} as unknown as T;
  }
  if (cmd === "schedule_update") {
    const s = args.schedule as ScheduleRecord;
    const i = mockSchedules.findIndex((x) => x.id === s.id);
    if (i !== -1) mockSchedules[i] = s;
    return {} as unknown as T;
  }
  if (cmd === "schedule_delete") {
    const id = args.id as string;
    const i = mockSchedules.findIndex((x) => x.id === id);
    if (i !== -1) mockSchedules.splice(i, 1);
    return {} as unknown as T;
  }
  if (cmd === "category_list") {
    return [...mockCategories] as unknown as T;
  }
  if (cmd === "category_update") {
    const c = args.category as CategoryRecord;
    const i = mockCategories.findIndex((x) => x.id === c.id);
    if (i !== -1) mockCategories[i] = c;
    return {} as unknown as T;
  }

  if (cmd === "expand_url_pattern") {
    const pat = (args.pattern as string) || "";
    const rangeMatch = pat.match(/\[(\d+)-(\d+)\]/);
    if (rangeMatch) {
      const start = parseInt(rangeMatch[1], 10);
      const end = Math.min(start + 25, parseInt(rangeMatch[2], 10));
      const pad = rangeMatch[1].length;
      const out: string[] = [];
      for (let i = start; i <= end; i++) {
        out.push(pat.replace(rangeMatch[0], String(i).padStart(pad, "0")));
      }
      return out as unknown as T;
    }
    return [pat] as unknown as T;
  }

  if (cmd === "add_batch" || cmd === "add_from_pattern") {
    const batchArgs = (args.args as BatchDownloadArgs) || { urls_or_text: "" };
    const lines = batchArgs.urls_or_text
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter(Boolean);
    const ids: string[] = [];
    for (const url of lines) {
      const id = await tauriInvoke<string>("add_download", {
        args: {
          url,
          save_dir: batchArgs.save_dir,
          category_id: batchArgs.category_id,
          queue_id: batchArgs.queue_id,
          segments_cfg: batchArgs.segments_cfg,
          start_now: batchArgs.start_now,
        },
      });
      ids.push(id);
    }
    return ids as unknown as T;
  }

  if (cmd === "grab_site_start") {
    const cfg = args.config as CrawlerConfig;
    const hostUrl = cfg.start_url.replace(/\/$/, "");
    return [
      {
        url: `${hostUrl}/releases/turboget-setup.exe`,
        file_name: "turboget-setup.exe",
        content_type: "application/octet-stream",
        content_length: 24 * 1024 * 1024,
        depth: 1,
        source_page: cfg.start_url,
      },
      {
        url: `${hostUrl}/docs/user-manual.pdf`,
        file_name: "user-manual.pdf",
        content_type: "application/pdf",
        content_length: 3 * 1024 * 1024,
        depth: 1,
        source_page: cfg.start_url,
      },
    ] as unknown as T;
  }

  if (cmd === "export_list") {
    const fmt = ((args.format as string) || "txt").toLowerCase();
    if (fmt === "json") return JSON.stringify(mockDownloads, null, 2) as unknown as T;
    return mockDownloads.map((d) => d.url).join("\n") as unknown as T;
  }

  if (cmd === "verify_file_checksum") {
    const algo = ((args.algorithm as string) || "sha256").toUpperCase();
    const expected = (args.expected as string) || "";
    const fakeHash = algo.includes("512")
      ? "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
      : "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    return {
      algorithm: algo.includes("512") ? "SHA-512" : "SHA-256",
      hash: fakeHash,
      matches: expected.trim() ? expected.trim().toLowerCase() === fakeHash : null,
      file_size: 1024 * 1024,
      elapsed_ms: 4,
    } as unknown as T;
  }

  if (cmd === "settings_get") {
    return {
      proxy_mode: "system",
      max_connections_per_host: 6,
      auto_update_enabled: true,
      speed_limit_kbps: 0,
    } as unknown as T;
  }

  if (cmd === "agent_access_get") {
    return {
      mode: "ask",
      allowed_dirs: ["E:/Project/idm/downloads"],
      block_private_ips: true,
      max_requests_per_minute: 30,
    } as unknown as T;
  }

  if (cmd === "get_audit_logs") {
    return [
      {
        id: 1,
        ts: Date.now(),
        level: "INFO",
        message: "TurboGet Core Engine initialized",
      },
    ] as unknown as T;
  }

  if (cmd === "get_default_download_dir") {
    return "E:/Project/idm/downloads" as unknown as T;
  }

  if (cmd === "pick_folder") {
    return null as unknown as T;
  }

  if (cmd === "pick_torrent_file") {
    return null as unknown as T;
  }

  if (cmd === "pop_pending_download_prompt") {
    return null as unknown as T;
  }

  if (cmd === "media_tools_status") {
    return {
      ytdlp_installed: true,
      ffmpeg_installed: true,
    } as unknown as T;
  }

  if (cmd === "media_probe") {
    const url = (args.url as string) || "";
    return {
      title: "Sample_Video",
      url,
      extractor: "mock",
      duration: 120,
      formats: [
        { format_id: "137", ext: "mp4", resolution: "1920x1080", width: 1920, height: 1080, has_video: true, has_audio: false, filesize: 42000000 },
        { format_id: "136", ext: "mp4", resolution: "1280x720", width: 1280, height: 720, has_video: true, has_audio: true, filesize: 21000000 },
      ],
    } as unknown as T;
  }

  if (cmd === "media_download") {
    return ("mock-media-" + Math.random().toString(36).substring(2, 8)) as unknown as T;
  }

  if (cmd === "autostart_get") {
    return true as unknown as T;
  }

  if (cmd === "autostart_set") {
    return Boolean(args.enabled) as unknown as T;
  }

  if (cmd === "get_extension_dir") {
    return "E:/Project/idm/extension" as unknown as T;
  }

  if (cmd === "browsers_detect") {
    return [
      {
        id: "chrome",
        name: "Google Chrome",
        installed: true,
        running: false,
        extension_installed: true,
        supports_developer_mode: true,
        supports_cli_autoload: false,
        coming_soon: false,
        status_label: "Terpasang",
        extensions_url: "chrome://extensions",
      },
      {
        id: "edge",
        name: "Microsoft Edge",
        installed: true,
        running: false,
        extension_installed: false,
        supports_developer_mode: true,
        supports_cli_autoload: true,
        coming_soon: false,
        status_label: "Auto-Load Siap",
        extensions_url: "edge://extensions",
      },
      {
        id: "opera",
        name: "Opera Browser",
        installed: false,
        running: false,
        extension_installed: false,
        supports_developer_mode: true,
        supports_cli_autoload: true,
        coming_soon: false,
        status_label: "Auto-Load Siap",
        extensions_url: "opera://extensions",
      },
      {
        id: "firefox",
        name: "Mozilla Firefox",
        installed: true,
        running: false,
        extension_installed: false,
        supports_developer_mode: false,
        supports_cli_autoload: false,
        coming_soon: true,
        status_label: "Soon on Store Addon",
        extensions_url: "about:addons",
      },
    ] as unknown as T;
  }

  if (cmd === "browser_integrate") {
    const bid = String(args.browserId || "chrome");
    return {
      browser_id: bid,
      browser_name: bid === "edge" ? "Microsoft Edge" : bid === "opera" ? "Opera Browser" : "Google Chrome",
      extension_dir: "E:/Project/idm/extension",
      extensions_url: `${bid}://extensions`,
      launched: true,
      auto_loaded: bid === "edge" || bid === "opera",
      needs_relaunch: false,
      message: "Browser dibuka ke halaman panduan ekstensi. Aktifkan Developer Mode lalu Load unpacked.",
    } as unknown as T;
  }

  return {} as unknown as T;
}

interface AppState {
  downloads: DownloadItem[];
  selectedId: string | null;
  selectedIds: string[];
  activeView: ActiveView;
  viewMode: "table" | "cards";
  telemetryExpanded: boolean;
  telemetryMode: "area" | "bars";
  filter: string; // 'all' | 'downloading' | 'completed' | 'paused' | 'unfinished' | 'compressed' | ...
  searchQuery: string;
  sortField: keyof DownloadItem;
  sortAsc: boolean;
  theme: "dark" | "light";
  lang: Language;
  speedHistory: Record<string, number[]>;
  globalSpeedHistory: number[];
  activeSegments: SegmentItem[];

  // Queues, Schedules & Categories
  queues: QueueRecord[];
  schedules: ScheduleRecord[];
  categories: CategoryRecord[];

  // Modal & Toast states
  isAddModalOpen: boolean;
  isDetailModalOpen: boolean;
  isSettingsModalOpen: boolean;
  settingsTab: SettingsTab;
  autostartEnabled: boolean;
  browsers: BrowserIntegrationInfo[];
  extensionDir: string | null;
  completedModalItem: DownloadItem | null;
  pendingPrompt: PendingDownloadPrompt | null;
  deleteModalTargetIds: string[] | null;
  checksumTargetItem: DownloadItem | null;
  toasts: DownloadToast[];

  networkSettings: NetworkSettings;
  agentPolicy: AgentAccessPolicy;
  auditLogs: AuditLogEntry[];

  // Actions
  setActiveView: (view: ActiveView) => void;
  setViewMode: (mode: "table" | "cards") => void;
  setTelemetryExpanded: (expanded: boolean) => void;
  setTelemetryMode: (mode: "area" | "bars") => void;
  setFilter: (f: string) => void;
  setSearchQuery: (q: string) => void;
  setSort: (field: keyof DownloadItem) => void;
  setSelectedId: (id: string | null) => void;
  toggleSelectId: (id: string) => void;
  setSelectedIds: (ids: string[]) => void;
  clearSelectedIds: () => void;
  toggleTheme: () => void;
  setTheme: (theme: "dark" | "light") => void;
  setLanguage: (lang: Language) => void;
  setAddModalOpen: (open: boolean) => void;
  setDetailModalOpen: (open: boolean) => void;
  setSettingsModalOpen: (open: boolean, tab?: SettingsTab) => void;
  setSettingsTab: (tab: SettingsTab) => void;
  setCompletedModalItem: (item: DownloadItem | null) => void;
  setPendingPrompt: (prompt: PendingDownloadPrompt | null) => void;
  setDeleteModalTargetIds: (ids: string[] | null) => void;
  setChecksumTargetItem: (item: DownloadItem | null) => void;
  dismissToast: (toastId: string) => void;

  t: (key: TranslationKey) => string;

  // Async Operations
  loadDownloads: () => Promise<void>;
  probeUrl: (url: string) => Promise<ProbeResult>;
  addDownload: (params: AddDownloadParams) => Promise<string>;
  pauseDownload: (id: string) => Promise<void>;
  pauseAllDownloads: () => Promise<void>;
  resumeDownload: (id: string) => Promise<void>;
  bulkResume: (ids: string[]) => Promise<void>;
  bulkPause: (ids: string[]) => Promise<void>;
  bulkDelete: (ids: string[], deleteFile?: boolean) => Promise<void>;
  cancelDownload: (id: string, deleteFile?: boolean) => Promise<void>;
  deleteDownload: (id: string, deleteFile?: boolean) => Promise<void>;
  loadSegments: (id: string) => Promise<void>;
  openFile: (path: string) => Promise<void>;
  openFolder: (path: string) => Promise<void>;
  loadSettingsAndLogs: () => Promise<void>;
  saveNetworkSettings: (settings: NetworkSettings) => Promise<void>;
  saveAgentPolicy: (policy: AgentAccessPolicy) => Promise<void>;
  loadSystemIntegration: () => Promise<void>;
  setAutostartEnabled: (enabled: boolean) => Promise<boolean>;
  integrateBrowser: (browserId: string, relaunch?: boolean) => Promise<BrowserIntegrateResult>;
  checkAppUpdate: (feedUrl?: string) => Promise<UpdateCheckResult>;
  setGlobalSpeedLimit: (kbps: number) => Promise<void>;
  getDefaultDownloadDir: () => Promise<string>;
  pickFolder: (initialDir?: string) => Promise<string | null>;
  pickTorrentFile: () => Promise<string | null>;
  popPendingDownloadPrompt: () => Promise<PendingDownloadPrompt | null>;

  // Scheduler, Queues, Categories, Grabber, Checksum
  loadQueuesAndSchedules: () => Promise<void>;
  createQueue: (queue: QueueRecord) => Promise<void>;
  updateQueue: (queue: QueueRecord) => Promise<void>;
  deleteQueue: (id: string) => Promise<void>;
  startQueue: (id: string) => Promise<void>;
  pauseQueue: (id: string) => Promise<void>;
  createSchedule: (schedule: ScheduleRecord) => Promise<void>;
  updateSchedule: (schedule: ScheduleRecord) => Promise<void>;
  deleteSchedule: (id: string) => Promise<void>;
  updateCategory: (category: CategoryRecord) => Promise<void>;
  setDownloadPriority: (id: string, priority: number) => Promise<void>;
  expandUrlPattern: (pattern: string) => Promise<string[]>;
  addBatch: (args: BatchDownloadArgs) => Promise<string[]>;
  addFromPattern: (args: BatchDownloadArgs) => Promise<string[]>;
  mediaProbe: (url: string, headers?: [string, string][]) => Promise<MediaProbeResult>;
  mediaDownload: (args: MediaDownloadParams) => Promise<string>;
  getMediaToolsStatus: () => Promise<MediaToolsStatus>;
  grabSiteStart: (config: CrawlerConfig) => Promise<CrawledItem[]>;
  grabSiteStop: () => Promise<void>;
  exportDownloadsList: (format: string) => Promise<string>;
  importDownloadsList: (format: string, content: string, saveDir?: string, startNow?: boolean) => Promise<string[]>;
  verifyFileChecksum: (path: string, algorithm: string, expected?: string) => Promise<FileChecksumResult>;
}

export const useAppStore = create<AppState>((set, get) => ({
  downloads: [],
  selectedId: null,
  selectedIds: [],
  activeView: "downloads",
  viewMode: "table",
  telemetryExpanded: true,
  telemetryMode: "area",
  filter: "all",
  searchQuery: "",
  sortField: "created_at",
  sortAsc: false,
  theme: "dark",
  lang: "id",
  speedHistory: {},
  globalSpeedHistory: [],
  activeSegments: [],

  queues: [],
  schedules: [],
  categories: [],

  isAddModalOpen: false,
  isDetailModalOpen: false,
  isSettingsModalOpen: false,
  settingsTab: "network",
  autostartEnabled: true,
  browsers: [],
  extensionDir: null,
  completedModalItem: null,
  pendingPrompt: null,
  deleteModalTargetIds: null,
  checksumTargetItem: null,
  toasts: [],

  networkSettings: {
    proxy_mode: "system",
    max_connections_per_host: 6,
    auto_update_enabled: true,
    speed_limit_kbps: 0,
  },
  agentPolicy: {
    mode: "ask",
    allowed_dirs: [],
    block_private_ips: true,
    max_requests_per_minute: 30,
  },
  auditLogs: [],

  setActiveView: (activeView) => set({ activeView }),
  setViewMode: (viewMode) => set({ viewMode }),
  setTelemetryExpanded: (telemetryExpanded) => set({ telemetryExpanded }),
  setTelemetryMode: (telemetryMode) => set({ telemetryMode }),
  setFilter: (filter) => set({ filter, activeView: "downloads" }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setSort: (field) => {
    const { sortField, sortAsc } = get();
    if (sortField === field) {
      set({ sortAsc: !sortAsc });
    } else {
      set({ sortField: field, sortAsc: true });
    }
  },
  setSelectedId: (selectedId) => set({ selectedId }),
  toggleSelectId: (id) =>
    set((s) => {
      const exists = s.selectedIds.includes(id);
      return {
        selectedIds: exists ? s.selectedIds.filter((x) => x !== id) : [...s.selectedIds, id],
        selectedId: id,
      };
    }),
  setSelectedIds: (selectedIds) => set({ selectedIds }),
  clearSelectedIds: () => set({ selectedIds: [] }),
  toggleTheme: () => set((s) => ({ theme: s.theme === "dark" ? "light" : "dark" })),
  setTheme: (theme) => set({ theme }),
  setLanguage: (lang) => set({ lang }),
  setAddModalOpen: (isAddModalOpen) => set({ isAddModalOpen }),
  setDetailModalOpen: (isDetailModalOpen) => set({ isDetailModalOpen }),
  setSettingsModalOpen: (isSettingsModalOpen, tab) =>
    set((s) => ({
      isSettingsModalOpen,
      settingsTab: tab ?? s.settingsTab,
    })),
  setSettingsTab: (settingsTab) => set({ settingsTab }),
  setCompletedModalItem: (completedModalItem) => set({ completedModalItem }),
  setPendingPrompt: (pendingPrompt) => set({ pendingPrompt }),
  setDeleteModalTargetIds: (deleteModalTargetIds) => set({ deleteModalTargetIds }),
  setChecksumTargetItem: (checksumTargetItem) => set({ checksumTargetItem }),
  dismissToast: (toastId) =>
    set((s) => ({ toasts: s.toasts.filter((toast) => toast.id !== toastId) })),

  t: (key) => translate(get().lang, key),

  loadDownloads: async () => {
    try {
      const items = await tauriInvoke<DownloadItem[]>("list_downloads");
      set((state) => {
        const prevMap = new Map(state.downloads.map((d) => [d.id, d]));
        let newlyCompleted: DownloadItem | null = null;
        const newToasts = [...state.toasts];
        const newHistory = { ...state.speedHistory };
        let totalSpeed = 0;

        items.forEach((item) => {
          const sp = item.speed_bps || 0;
          if (item.status === "downloading" || item.status === "probing") {
            totalSpeed += sp;
          }
          if (!newHistory[item.id]) {
            newHistory[item.id] = [];
          }
          newHistory[item.id].push(sp);
          if (newHistory[item.id].length > 30) {
            newHistory[item.id].shift();
          }

          const prev = prevMap.get(item.id);
          if (
            prev &&
            (prev.status === "downloading" || prev.status === "probing" || prev.status === "verifying") &&
            item.status === "completed"
          ) {
            if (!newlyCompleted && !state.completedModalItem) {
              newlyCompleted = item;
            }
            newToasts.unshift({
              id: `toast-done-${item.id}-${Date.now()}`,
              downloadId: item.id,
              kind: "completed",
              fileName: item.file_name,
              saveDir: item.save_dir,
              totalBytes: item.total_bytes || item.downloaded,
              createdAt: Date.now(),
            });
          } else if (
            prev &&
            (prev.status === "downloading" || prev.status === "probing") &&
            item.status === "failed"
          ) {
            newToasts.unshift({
              id: `toast-fail-${item.id}-${Date.now()}`,
              downloadId: item.id,
              kind: "failed",
              fileName: item.file_name,
              saveDir: item.save_dir,
              totalBytes: item.total_bytes,
              errorMessage: item.error || "Koneksi terputus / timeout",
              createdAt: Date.now(),
            });
          }
        });

        const nextGlobal = [...state.globalSpeedHistory, totalSpeed];
        if (nextGlobal.length > 40) nextGlobal.shift();

        const validIds = new Set(items.map((i) => i.id));
        const cleanedSelected = state.selectedIds.filter((id) => validIds.has(id));

        return {
          downloads: items,
          selectedIds: cleanedSelected,
          speedHistory: newHistory,
          globalSpeedHistory: nextGlobal,
          completedModalItem: newlyCompleted || state.completedModalItem,
          toasts: newToasts.slice(0, 5),
        };
      });
    } catch (e) {
      console.error("Failed to load downloads:", e);
    }
  },

  probeUrl: async (url: string) => {
    return await tauriInvoke<ProbeResult>("probe_url", { url });
  },

  addDownload: async (params) => {
    const id = await tauriInvoke<string>("add_download", { args: params });
    await get().loadDownloads();
    return id;
  },

  pauseDownload: async (id) => {
    await tauriInvoke("pause_download", { id });
    await get().loadDownloads();
  },

  pauseAllDownloads: async () => {
    await tauriInvoke("pause_all_downloads");
    await get().loadDownloads();
  },

  resumeDownload: async (id) => {
    await tauriInvoke("resume_download", { id });
    await get().loadDownloads();
  },

  bulkResume: async (ids) => {
    for (const id of ids) {
      await tauriInvoke("resume_download", { id });
    }
    await get().loadDownloads();
  },

  bulkPause: async (ids) => {
    for (const id of ids) {
      await tauriInvoke("pause_download", { id });
    }
    await get().loadDownloads();
  },

  bulkDelete: async (ids, deleteFile = false) => {
    for (const id of ids) {
      await tauriInvoke("delete_download", { id, deleteFile });
    }
    set((s) => ({
      selectedIds: s.selectedIds.filter((x) => !ids.includes(x)),
      selectedId: s.selectedId && ids.includes(s.selectedId) ? null : s.selectedId,
    }));
    await get().loadDownloads();
  },

  cancelDownload: async (id, deleteFile = false) => {
    await tauriInvoke("cancel_download", { id, deleteFile });
    await get().loadDownloads();
  },

  deleteDownload: async (id, deleteFile = false) => {
    await tauriInvoke("delete_download", { id, deleteFile });
    set((s) => ({
      selectedId: s.selectedId === id ? null : s.selectedId,
      selectedIds: s.selectedIds.filter((x) => x !== id),
    }));
    await get().loadDownloads();
  },

  loadSegments: async (id) => {
    try {
      const segs = await tauriInvoke<SegmentItem[]>("get_segments", { id });
      set({ activeSegments: segs });
    } catch (e) {
      console.error("Failed to load segments:", e);
    }
  },

  openFile: async (path) => {
    await tauriInvoke("open_file", { path });
  },

  openFolder: async (path) => {
    await tauriInvoke("open_folder", { path });
  },

  loadSettingsAndLogs: async () => {
    try {
      const [net, policy, logs, cats] = await Promise.all([
        tauriInvoke<NetworkSettings>("settings_get"),
        tauriInvoke<AgentAccessPolicy>("agent_access_get"),
        tauriInvoke<AuditLogEntry[]>("get_audit_logs", { limit: 100 }),
        tauriInvoke<CategoryRecord[]>("category_list").catch(() => []),
      ]);
      set({
        networkSettings: net,
        agentPolicy: policy,
        auditLogs: Array.isArray(logs) ? logs : [],
        categories: Array.isArray(cats) ? cats : [],
      });
    } catch (e) {
      console.error("Failed loading settings/logs:", e);
    }
  },

  saveNetworkSettings: async (settings) => {
    await tauriInvoke("settings_set", { settings });
    set({ networkSettings: settings });
  },

  saveAgentPolicy: async (policy) => {
    await tauriInvoke("agent_access_set", { policy });
    set({ agentPolicy: policy });
  },

  loadSystemIntegration: async () => {
    try {
      const [auto, browsers, extDir] = await Promise.all([
        tauriInvoke<boolean>("autostart_get").catch(() => true),
        tauriInvoke<BrowserIntegrationInfo[]>("browsers_detect").catch(() => []),
        tauriInvoke<string | null>("get_extension_dir").catch(() => null),
      ]);
      set({
        autostartEnabled: Boolean(auto),
        browsers: Array.isArray(browsers) ? browsers : [],
        extensionDir: extDir,
      });
    } catch (e) {
      console.error("Failed loading system integration:", e);
    }
  },

  setAutostartEnabled: async (enabled) => {
    const actual = await tauriInvoke<boolean>("autostart_set", { enabled });
    set({ autostartEnabled: Boolean(actual) });
    return Boolean(actual);
  },

  integrateBrowser: async (browserId, relaunch = false) => {
    const res = await tauriInvoke<BrowserIntegrateResult>("browser_integrate", { browserId, relaunch });
    await get().loadSystemIntegration();
    return res;
  },

  checkAppUpdate: async (feedUrl) => {
    return await tauriInvoke<UpdateCheckResult>("check_app_update", { feedUrl });
  },

  setGlobalSpeedLimit: async (kbps) => {
    const clean = Math.max(0, Math.floor(kbps));
    const nextSettings: NetworkSettings = {
      ...get().networkSettings,
      speed_limit_kbps: clean,
    };
    await tauriInvoke("set_speed_limit", { limitBps: clean * 1024 });
    await tauriInvoke("settings_set", { settings: nextSettings });
    set({ networkSettings: nextSettings });
  },

  getDefaultDownloadDir: async () => {
    try {
      return await tauriInvoke<string>("get_default_download_dir");
    } catch {
      return "E:/Project/idm/downloads";
    }
  },

  pickFolder: async (initialDir) => {
    try {
      return await tauriInvoke<string | null>("pick_folder", { initialDir });
    } catch {
      return null;
    }
  },

  pickTorrentFile: async () => {
    try {
      return await tauriInvoke<string | null>("pick_torrent_file");
    } catch {
      return null;
    }
  },

  popPendingDownloadPrompt: async () => {
    try {
      return await tauriInvoke<PendingDownloadPrompt | null>("pop_pending_download_prompt");
    } catch {
      return null;
    }
  },

  loadQueuesAndSchedules: async () => {
    try {
      const [queues, schedules, categories] = await Promise.all([
        tauriInvoke<QueueRecord[]>("queue_list"),
        tauriInvoke<ScheduleRecord[]>("schedule_list"),
        tauriInvoke<CategoryRecord[]>("category_list"),
      ]);
      set({
        queues: Array.isArray(queues) ? queues : [],
        schedules: Array.isArray(schedules) ? schedules : [],
        categories: Array.isArray(categories) ? categories : [],
      });
    } catch (e) {
      console.error("Failed loading queues/schedules:", e);
    }
  },

  createQueue: async (queue) => {
    await tauriInvoke("queue_create", { queue });
    await get().loadQueuesAndSchedules();
  },

  updateQueue: async (queue) => {
    await tauriInvoke("queue_update", { queue });
    await get().loadQueuesAndSchedules();
  },

  deleteQueue: async (id) => {
    await tauriInvoke("queue_delete", { id });
    await get().loadQueuesAndSchedules();
  },

  startQueue: async (id) => {
    await tauriInvoke("queue_start", { id });
    await get().loadDownloads();
  },

  pauseQueue: async (id) => {
    await tauriInvoke("queue_pause", { id });
    await get().loadDownloads();
  },

  createSchedule: async (schedule) => {
    await tauriInvoke("schedule_create", { schedule });
    await get().loadQueuesAndSchedules();
  },

  updateSchedule: async (schedule) => {
    await tauriInvoke("schedule_update", { schedule });
    await get().loadQueuesAndSchedules();
  },

  deleteSchedule: async (id) => {
    await tauriInvoke("schedule_delete", { id });
    await get().loadQueuesAndSchedules();
  },

  updateCategory: async (category) => {
    await tauriInvoke("category_update", { category });
    await get().loadQueuesAndSchedules();
  },

  setDownloadPriority: async (id, priority) => {
    await tauriInvoke("set_download_priority", { id, priority });
    await get().loadDownloads();
  },

  expandUrlPattern: async (pattern) => {
    return await tauriInvoke<string[]>("expand_url_pattern", { pattern });
  },

  addBatch: async (args) => {
    const ids = await tauriInvoke<string[]>("add_batch", { args });
    await get().loadDownloads();
    return ids;
  },

  addFromPattern: async (args) => {
    const ids = await tauriInvoke<string[]>("add_from_pattern", { args });
    await get().loadDownloads();
    return ids;
  },

  mediaProbe: async (url, headers) => {
    return await tauriInvoke<MediaProbeResult>("media_probe", { url, headers });
  },

  mediaDownload: async (args) => {
    const id = await tauriInvoke<string>("media_download", { args });
    await get().loadDownloads();
    return id;
  },

  getMediaToolsStatus: async () => {
    return await tauriInvoke<MediaToolsStatus>("media_tools_status");
  },

  grabSiteStart: async (config) => {
    return await tauriInvoke<CrawledItem[]>("grab_site_start", { config });
  },

  grabSiteStop: async () => {
    await tauriInvoke("grab_site_stop");
  },

  exportDownloadsList: async (format) => {
    return await tauriInvoke<string>("export_list", { format });
  },

  importDownloadsList: async (format, content, saveDir, startNow = false) => {
    const ids = await tauriInvoke<string[]>("import_list", { format, content, saveDir, startNow });
    await get().loadDownloads();
    return ids;
  },

  verifyFileChecksum: async (path, algorithm, expected) => {
    return await tauriInvoke<FileChecksumResult>("verify_file_checksum", {
      path,
      algorithm,
      expected,
    });
  },
}));
