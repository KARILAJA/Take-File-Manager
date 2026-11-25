// Take File Manager MV3 Background Service Worker
const NATIVE_HOST_NAME = "com.turboget.host";

const DEFAULT_SETTINGS = {
  enabled: true,
  askSaveLocation: true,
  minFileSizeBytes: 1024 * 1024, // 1 MiB
  captureExtensions: [
    "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "tgz", "torrent",
    "exe", "msi", "apk", "dmg", "deb", "rpm",
    "mp4", "mkv", "avi", "mov", "webm", "mp3", "flac", "pdf"
  ],
  excludedHosts: []
};

// In-memory cache of settings & detected media per tab
let cachedSettings = { ...DEFAULT_SETTINGS };
const detectedMediaByTab = new Map();
const pageVideoStateByTab = new Map();
const tabUrlsById = new Map();
const handledDownloadIds = new Set();
const inFlightDownloadIds = new Set();
const recentInterceptedUrls = new Map();
const DUPLICATE_URL_WINDOW_MS = 4000;

function extractExtension(fileNameOrUrl) {
  if (!fileNameOrUrl) return "";
  const clean = fileNameOrUrl.split("?")[0].split("#")[0];
  const base = clean.split(/[\\/]/).pop() || "";
  const dotIdx = base.lastIndexOf(".");
  if (dotIdx <= 0 || dotIdx === base.length - 1) return "";
  return base.slice(dotIdx + 1).toLowerCase();
}

function isExcludedHost(urlStr, excludedHosts) {
  try {
    const parsed = new URL(urlStr);
    const host = parsed.hostname.toLowerCase();
    return (excludedHosts || []).some((pattern) => {
      const p = pattern.trim().toLowerCase();
      if (!p) return false;
      return host === p || host.endsWith("." + p);
    });
  } catch {
    return true;
  }
}

function isSupportedVideoPageUrl(urlStr) {
  if (!urlStr || (!urlStr.startsWith("http://") && !urlStr.startsWith("https://"))) {
    return false;
  }
  try {
    const u = new URL(urlStr);
    const host = u.hostname.toLowerCase();
    const path = u.pathname.toLowerCase();
    if (host.includes("youtube.com") && (path.startsWith("/watch") || path.startsWith("/shorts/") || path.startsWith("/live/"))) {
      return true;
    }
    if (host === "youtu.be" && path.length > 1) return true;
    if (host.includes("tiktok.com") && (path.includes("/video/") || path.includes("/@"))) return true;
    if (host.includes("instagram.com") && (path.startsWith("/reel/") || path.startsWith("/reels/") || path.startsWith("/p/"))) return true;
    if ((host.includes("facebook.com") || host === "fb.watch") && (path.includes("/watch") || path.includes("/reel") || path.includes("/videos/") || host === "fb.watch")) return true;
    if ((host.includes("twitter.com") || host.includes("x.com")) && path.includes("/status/")) return true;
    if (host.includes("vimeo.com") && /^\/\d+/.test(path)) return true;
    if (host.includes("dailymotion.com") && path.includes("/video/")) return true;
    if (host.includes("twitch.tv") && (path.includes("/videos/") || path.includes("/clip/"))) return true;
    if (host.includes("bilibili.com") && path.includes("/video/")) return true;
    return false;
  } catch {
    return false;
  }
}

function isNoisyMediaSegmentUrl(urlStr) {
  const lower = (urlStr || "").toLowerCase();
  if (lower.includes("googlevideo.com/videoplayback")) {
    return true;
  }
  const clean = lower.split("?")[0].split("#")[0];
  if (clean.endsWith(".ts") || clean.endsWith(".m4s") || clean.endsWith(".cmfv") || clean.endsWith(".cmfa")) {
    return true;
  }
  if (/([?&](range|rn|rbuf|sq|seg|segment)=|\/seg-\d+|chunk[-_]?\d+)/i.test(lower)) {
    return true;
  }
  return false;
}

function inferLabelFromUrl(urlStr, kind) {
  try {
    const u = new URL(urlStr);
    const seg = u.pathname.split("/").filter(Boolean).pop() || "";
    if (seg && seg.length <= 64) {
      return `${decodeURIComponent(seg)} (${kind})`;
    }
    return `${u.hostname} (${kind})`;
  } catch {
    return urlStr;
  }
}

function shouldCaptureDownload(item, settings = cachedSettings) {
  if (!settings || !settings.enabled) return false;
  const targetUrl = item.finalUrl || item.url || "";
  if (!targetUrl.startsWith("http://") && !targetUrl.startsWith("https://")) {
    return false;
  }
  if (isExcludedHost(targetUrl, settings.excludedHosts)) {
    return false;
  }

  const ext = extractExtension(item.filename) || extractExtension(targetUrl);
  const extMatch = (settings.captureExtensions || []).some(
    (e) => e.trim().toLowerCase() === ext
  );
  const sizeMatch =
    typeof item.fileSize === "number" &&
    item.fileSize > 0 &&
    item.fileSize >= (settings.minFileSizeBytes ?? DEFAULT_SETTINGS.minFileSizeBytes);

  return extMatch || sizeMatch;
}

function formatCookies(cookies) {
  if (!Array.isArray(cookies) || cookies.length === 0) return "";
  return cookies.map((c) => `${c.name}=${c.value}`).join("; ");
}

function extractBaseName(filePath) {
  if (!filePath) return undefined;
  const base = filePath.split(/[\\/]/).pop();
  return base && base.trim() ? base.trim() : undefined;
}

async function getSettings() {
  if (typeof chrome !== "undefined" && chrome.storage?.local) {
    const stored = await chrome.storage.local.get("turbogetSettings");
    cachedSettings = { ...DEFAULT_SETTINGS, ...(stored.turbogetSettings || {}) };
  }
  return cachedSettings;
}

async function fetchCookieString(url) {
  if (typeof chrome !== "undefined" && chrome.cookies?.getAll && url) {
    try {
      const cookies = await chrome.cookies.getAll({ url });
      return formatCookies(cookies);
    } catch {
      return "";
    }
  }
  return "";
}

async function sendToNativeHost(message) {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendNativeMessage(NATIVE_HOST_NAME, message, (response) => {
      if (chrome.runtime.lastError) {
        reject(new Error(chrome.runtime.lastError.message));
      } else {
        resolve(response);
      }
    });
  });
}

async function handleDownloadCreated(item) {
  const hasId = item && typeof item.id === "number";
  if (hasId && (handledDownloadIds.has(item.id) || inFlightDownloadIds.has(item.id))) {
    return { intercepted: false, duplicate: true };
  }
  if (hasId) {
    inFlightDownloadIds.add(item.id);
  }

  try {
    const settings = await getSettings();
    if (!shouldCaptureDownload(item, settings)) {
      return { intercepted: false };
    }

    const targetUrl = item.finalUrl || item.url;
    const now = Date.now();
    const lastTs = recentInterceptedUrls.get(targetUrl);
    if (lastTs && now - lastTs < DUPLICATE_URL_WINDOW_MS) {
      if (hasId) handledDownloadIds.add(item.id);
      return { intercepted: false, duplicate: true };
    }

    if (hasId) {
      handledDownloadIds.add(item.id);
      if (handledDownloadIds.size > 500) {
        const oldest = handledDownloadIds.values().next().value;
        handledDownloadIds.delete(oldest);
      }
    }
    recentInterceptedUrls.set(targetUrl, now);
    if (recentInterceptedUrls.size > 100) {
      for (const [k, ts] of recentInterceptedUrls.entries()) {
        if (now - ts >= DUPLICATE_URL_WINDOW_MS) recentInterceptedUrls.delete(k);
      }
    }

    const cookies = await fetchCookieString(targetUrl);

    try {
      await chrome.downloads.cancel(item.id);
      await chrome.downloads.erase({ id: item.id });
    } catch (err) {
      console.warn("TFM: failed to cancel browser download:", err);
    }

    const payload = {
      url: targetUrl,
      referrer: item.referrer || "",
      cookies,
      userAgent: typeof navigator !== "undefined" ? navigator.userAgent : "",
      fileName: extractBaseName(item.filename),
      mime: item.mime || "",
      askSaveLocation: settings.askSaveLocation !== false
    };

    try {
      const resp = await sendToNativeHost({ type: "add_download", payload });
      return { intercepted: true, response: resp };
    } catch (err) {
      console.error("TFM: native host error:", err);
      return { intercepted: true, error: err.message };
    }
  } finally {
    if (hasId) {
      inFlightDownloadIds.delete(item.id);
    }
  }
}

function updateTabBadge(tabId) {
  if (typeof chrome === "undefined" || !chrome.action?.setBadgeText || tabId < 0) return;
  const items = getCombinedTabMedia(tabId);
  const text = items.length > 0 ? String(items.length) : "";
  chrome.action.setBadgeText({ text, tabId });
  if (text && chrome.action.setBadgeBackgroundColor) {
    chrome.action.setBadgeBackgroundColor({ color: "#2563eb", tabId });
  }
}

function detectMediaResponse(details) {
  if (!details || details.tabId < 0 || !details.responseHeaders) return null;
  if (!details.url || (!details.url.startsWith("http://") && !details.url.startsWith("https://"))) {
    return null;
  }

  const ctHeader = details.responseHeaders.find(
    (h) => h.name.toLowerCase() === "content-type"
  );
  const ct = ctHeader && ctHeader.value ? ctHeader.value.toLowerCase() : "";
  const urlLower = details.url.toLowerCase();
  const ext = extractExtension(details.url);

  const isHls =
    ct.includes("application/vnd.apple.mpegurl") ||
    ct.includes("application/x-mpegurl") ||
    urlLower.includes(".m3u8");
  const isDash =
    ct.includes("application/dash+xml") ||
    urlLower.includes(".mpd");
  const isDirectVideoExt = ["mp4", "webm", "mkv", "mov", "m4v", "flv"].includes(ext);
  const isDirectAudioExt = ["mp3", "m4a", "aac", "ogg", "flac"].includes(ext);
  const isOctetOrEmpty = !ct || ct.includes("octet-stream");

  const isMedia =
    ct.startsWith("video/") ||
    ct.startsWith("audio/") ||
    isHls ||
    isDash ||
    (isOctetOrEmpty && (isDirectVideoExt || isDirectAudioExt));

  if (!isMedia) return null;
  if (!isHls && !isDash && isNoisyMediaSegmentUrl(details.url)) return null;

  const clHeader = details.responseHeaders.find(
    (h) => h.name.toLowerCase() === "content-length"
  );
  const size = clHeader ? parseInt(clHeader.value, 10) || null : null;
  // Ignore tiny < 40KB tracking/probe video blobs unless HLS/DASH manifest
  if (!isHls && !isDash && size !== null && size > 0 && size < 40 * 1024) {
    return null;
  }

  const kind = isHls
    ? "HLS"
    : isDash
    ? "DASH"
    : ct.startsWith("audio/") || isDirectAudioExt
    ? "Audio"
    : "Video";

  const list = detectedMediaByTab.get(details.tabId) || [];
  if (!list.some((m) => m.url === details.url)) {
    const entry = {
      url: details.url,
      title: inferLabelFromUrl(details.url, kind),
      contentType: ctHeader?.value || (isDirectVideoExt ? `video/${ext}` : "application/octet-stream"),
      kind,
      size,
      timestamp: Date.now()
    };
    list.push(entry);
    detectedMediaByTab.set(details.tabId, list);
    updateTabBadge(details.tabId);
    return entry;
  }
  return null;
}

function getCombinedTabMedia(tabId, explicitUrl, explicitTitle) {
  const pageState = pageVideoStateByTab.get(tabId);
  const pageUrl = explicitUrl || pageState?.pageUrl || tabUrlsById.get(tabId) || "";
  const pageTitle = explicitTitle || pageState?.title || "";
  const streams = detectedMediaByTab.get(tabId) || [];

  const result = [];
  const seenUrls = new Set();

  if (pageUrl && isSupportedVideoPageUrl(pageUrl)) {
    seenUrls.add(pageUrl);
    result.push({
      url: pageUrl,
      title: pageTitle || inferLabelFromUrl(pageUrl, "PAGE"),
      kind: "VIDEO",
      isPageVideo: true,
      size: null,
      timestamp: Date.now()
    });
  }

  if (pageState && Array.isArray(pageState.sources)) {
    for (const src of pageState.sources) {
      if (src && (src.startsWith("http://") || src.startsWith("https://")) && !seenUrls.has(src)) {
        seenUrls.add(src);
        result.push({
          url: src,
          title: pageTitle ? `${pageTitle}.mp4` : inferLabelFromUrl(src, "Video"),
          kind: src.toLowerCase().includes(".m3u8")
            ? "HLS"
            : src.toLowerCase().includes(".mpd")
            ? "DASH"
            : "Video",
          size: null,
          timestamp: Date.now()
        });
      }
    }
  }

  for (const s of streams) {
    if (!seenUrls.has(s.url)) {
      seenUrls.add(s.url);
      result.push(s);
    }
  }

  // Fallback for sites playing <video src="blob:..."> with no HLS/DASH intercepted yet
  if (result.length === 0 && pageUrl && pageState?.hasVideoTag) {
    result.push({
      url: pageUrl,
      title: pageTitle || inferLabelFromUrl(pageUrl, "PAGE"),
      kind: "PAGE",
      isPageVideo: true,
      size: null,
      timestamp: Date.now()
    });
  }

  return result;
}

function initExtensionListeners() {
  if (typeof chrome === "undefined" || !chrome.runtime) return;

  getSettings();

  if (chrome.storage?.onChanged) {
    chrome.storage.onChanged.addListener((changes, area) => {
      if (area === "local" && changes.turbogetSettings) {
        cachedSettings = { ...DEFAULT_SETTINGS, ...(changes.turbogetSettings.newValue || {}) };
      }
    });
  }

  chrome.runtime.onInstalled.addListener(() => {
    chrome.contextMenus.create({
      id: "turboget-download-link",
      title: "Download dengan Take File Manager",
      contexts: ["link", "image", "video", "audio"]
    });
  });

  if (chrome.tabs?.onUpdated) {
    chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
      if (changeInfo.url) {
        const prevUrl = tabUrlsById.get(tabId);
        if (prevUrl !== changeInfo.url) {
          tabUrlsById.set(tabId, changeInfo.url);
          detectedMediaByTab.delete(tabId);
          pageVideoStateByTab.delete(tabId);
          updateTabBadge(tabId);
        }
      } else if (tab?.url) {
        tabUrlsById.set(tabId, tab.url);
      }
      if (changeInfo.status === "complete" && tab?.url) {
        updateTabBadge(tabId);
      }
    });
  }

  if (chrome.tabs?.onRemoved) {
    chrome.tabs.onRemoved.addListener((tabId) => {
      detectedMediaByTab.delete(tabId);
      pageVideoStateByTab.delete(tabId);
      tabUrlsById.delete(tabId);
    });
  }

  if (chrome.downloads?.onCreated) {
    chrome.downloads.onCreated.addListener((item) => {
      handleDownloadCreated(item);
    });
  }

  if (chrome.downloads?.onDeterminingFilename) {
    chrome.downloads.onDeterminingFilename.addListener((item) => {
      handleDownloadCreated(item);
    });
  }

  if (chrome.contextMenus?.onClicked) {
    chrome.contextMenus.onClicked.addListener(async (info) => {
      if (info.menuItemId !== "turboget-download-link") return;
      const targetUrl = info.linkUrl || info.srcUrl;
      if (!targetUrl) return;

      const settings = await getSettings();
      const cookies = await fetchCookieString(targetUrl);
      try {
        await sendToNativeHost({
          type: "add_download",
          payload: {
            url: targetUrl,
            referrer: info.pageUrl || "",
            cookies,
            userAgent: typeof navigator !== "undefined" ? navigator.userAgent : "",
            askSaveLocation: settings.askSaveLocation !== false
          }
        });
      } catch (err) {
        console.error("TFM context menu send error:", err);
      }
    });
  }

  if (chrome.webRequest?.onHeadersReceived) {
    chrome.webRequest.onHeadersReceived.addListener(
      (details) => {
        detectMediaResponse(details);
      },
      { urls: ["<all_urls>"] },
      ["responseHeaders"]
    );
  }

  chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
    if (msg.type === "magnet_link_clicked" && msg.url) {
      (async () => {
        const settings = await getSettings();
        if (!settings.enabled) {
          sendResponse({ ok: false, disabled: true });
          return;
        }
        try {
          const resp = await sendToNativeHost({
            type: "add_download",
            payload: {
              url: msg.url,
              referrer: msg.referrer || "",
              userAgent: typeof navigator !== "undefined" ? navigator.userAgent : "",
              askSaveLocation: settings.askSaveLocation !== false
            }
          });
          sendResponse({ ok: Boolean(resp && resp.ok), resp });
        } catch (err) {
          sendResponse({ ok: false, error: err.message });
        }
      })();
      return true;
    }

    if (msg.type === "video_detected_on_page") {
      const tabId = sender?.tab?.id;
      if (typeof tabId === "number" && tabId >= 0) {
        pageVideoStateByTab.set(tabId, {
          pageUrl: msg.pageUrl || sender.tab.url || "",
          title: msg.title || sender.tab.title || "",
          hasVideoTag: Boolean(msg.hasVideoTag),
          sources: Array.isArray(msg.sources) ? msg.sources : []
        });
        if (msg.pageUrl) tabUrlsById.set(tabId, msg.pageUrl);
        updateTabBadge(tabId);
      }
      sendResponse({ ok: true });
      return false;
    }

    if (msg.type === "get_status") {
      (async () => {
        const settings = await getSettings();
        try {
          const ping = await sendToNativeHost({ type: "ping" });
          sendResponse({
            ok: true,
            enabled: settings.enabled,
            askSaveLocation: settings.askSaveLocation !== false,
            ping
          });
        } catch (err) {
          sendResponse({
            ok: false,
            enabled: settings.enabled,
            askSaveLocation: settings.askSaveLocation !== false,
            error: err.message
          });
        }
      })();
      return true;
    }

    if (msg.type === "toggle_enabled") {
      (async () => {
        const settings = await getSettings();
        const updated = { ...settings, enabled: Boolean(msg.enabled) };
        cachedSettings = updated;
        await chrome.storage.local.set({ turbogetSettings: updated });
        sendResponse({ ok: true, enabled: updated.enabled });
      })();
      return true;
    }

    if (msg.type === "toggle_ask_save") {
      (async () => {
        const settings = await getSettings();
        const updated = { ...settings, askSaveLocation: Boolean(msg.askSaveLocation) };
        cachedSettings = updated;
        await chrome.storage.local.set({ turbogetSettings: updated });
        sendResponse({ ok: true, askSaveLocation: updated.askSaveLocation });
      })();
      return true;
    }

    if (msg.type === "get_detected_media") {
      const tabId = typeof msg.tabId === "number" ? msg.tabId : sender?.tab?.id;
      const media = typeof tabId === "number"
        ? getCombinedTabMedia(tabId, msg.tabUrl || sender?.tab?.url, msg.tabTitle || sender?.tab?.title)
        : [];
      sendResponse({ media });
      return false;
    }

    if (msg.type === "probe_media") {
      (async () => {
        const cookies = (await fetchCookieString(msg.url)) || (await fetchCookieString(msg.referrer));
        try {
          const resp = await sendToNativeHost({
            type: "media_probe",
            payload: {
              url: msg.url,
              referrer: msg.referrer || "",
              cookies,
              userAgent: typeof navigator !== "undefined" ? navigator.userAgent : ""
            }
          });
          sendResponse({ ok: Boolean(resp && resp.ok), resp, error: resp?.error });
        } catch (err) {
          sendResponse({ ok: false, error: err.message });
        }
      })();
      return true;
    }

    if (msg.type === "send_media_download") {
      (async () => {
        const settings = await getSettings();
        const cookies = (await fetchCookieString(msg.url)) || (await fetchCookieString(msg.referrer));
        const lowerUrl = (msg.url || "").toLowerCase();
        const useMediaPipeline =
          Boolean(msg.formatId && msg.formatId !== "direct") ||
          Boolean(msg.audioOnly) ||
          Boolean(msg.isPageVideo) ||
          isSupportedVideoPageUrl(msg.url) ||
          lowerUrl.includes(".m3u8") ||
          lowerUrl.includes(".mpd");

        try {
          const resp = await sendToNativeHost({
            type: useMediaPipeline ? "media_download" : "add_download",
            payload: {
              url: msg.url,
              formatId: msg.formatId || undefined,
              audioOnly: Boolean(msg.audioOnly),
              fileName: msg.fileName || undefined,
              referrer: msg.referrer || "",
              cookies,
              userAgent: typeof navigator !== "undefined" ? navigator.userAgent : "",
              askSaveLocation: settings.askSaveLocation !== false
            }
          });
          sendResponse({ ok: Boolean(resp && resp.ok), resp, error: resp?.error });
        } catch (err) {
          sendResponse({ ok: false, error: err.message });
        }
      })();
      return true;
    }

    return false;
  });
}

initExtensionListeners();

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    DEFAULT_SETTINGS,
    extractExtension,
    isExcludedHost,
    isSupportedVideoPageUrl,
    shouldCaptureDownload,
    formatCookies,
    handleDownloadCreated,
    detectMediaResponse,
    detectedMediaByTab
  };
}
