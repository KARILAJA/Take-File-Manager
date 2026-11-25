// Take File Manager — IDM-style Floating Video Grabber Overlay
(() => {
  if (window.top !== window.self) return;
  const HOST_ID = "tfm-video-grabber-host";
  let dismissedForUrl = null;
  let activeVideo = null;
  let shadowRoot = null;
  let hostEl = null;
  let panelEl = null;
  let pillBtn = null;
  let dropdownOpen = false;
  let lastNotifiedSignature = "";

  function collectPageVideoInfo() {
    const videos = Array.from(document.querySelectorAll("video"));
    const sources = [];
    let bestVisibleVideo = null;
    let bestScore = -1;

    for (const v of videos) {
      const rect = v.getBoundingClientRect();
      if (rect.width < 160 || rect.height < 90) continue;
      if (rect.bottom < 0 || rect.top > window.innerHeight) continue;

      const s = v.currentSrc || v.src || "";
      if (s && (s.startsWith("http://") || s.startsWith("https://"))) {
        sources.push(s);
      }
      v.querySelectorAll("source").forEach((el) => {
        const src = el.src || el.getAttribute("src") || "";
        if (src && (src.startsWith("http://") || src.startsWith("https://"))) {
          sources.push(src);
        }
      });

      const score = (!v.paused ? 1000000 : 0) + rect.width * rect.height;
      if (score > bestScore) {
        bestScore = score;
        bestVisibleVideo = v;
      }
    }

    const uniqSources = Array.from(new Set(sources));
    const sig = `${location.href}|${videos.length > 0}|${uniqSources.join(",")}`;
    if (sig !== lastNotifiedSignature && (videos.length > 0 || uniqSources.length > 0)) {
      lastNotifiedSignature = sig;
      try {
        chrome.runtime.sendMessage({
          type: "video_detected_on_page",
          pageUrl: location.href,
          title: (document.title || "").replace(/\s*-\s*YouTube$/i, "").trim(),
          hasVideoTag: videos.length > 0,
          sources: uniqSources
        });
      } catch {
        // Extension context may be invalidated after reload
      }
    }

    return bestVisibleVideo;
  }

  function ensureOverlayUi() {
    if (hostEl && document.documentElement.contains(hostEl)) return;
    hostEl = document.createElement("div");
    hostEl.id = HOST_ID;
    hostEl.style.cssText =
      "position:fixed;z-index:2147483647;top:0;left:0;pointer-events:none;font-family:system-ui,-apple-system,sans-serif;";
    shadowRoot = hostEl.attachShadow({ mode: "open" });

    const style = document.createElement("style");
    style.textContent = `
      .wrap { position: fixed; pointer-events: auto; display: none; flex-direction: column; align-items: flex-end; gap: 6px; }
      .pill {
        display: inline-flex; align-items: center; gap: 7px;
        background: rgba(11, 19, 38, 0.94); color: #dae2fd;
        border: 1px solid rgba(96, 165, 250, 0.45); border-radius: 9px;
        padding: 6px 10px; font-size: 12px; font-weight: 600;
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45); cursor: pointer;
        backdrop-filter: blur(8px); user-select: none;
      }
      .pill:hover { background: rgba(23, 31, 51, 0.98); border-color: #60a5fa; }
      .pill-dot { width: 8px; height: 8px; border-radius: 50%; background: #68dba9; flex-shrink: 0; }
      .close-x {
        margin-left: 4px; color: #93a1be; font-size: 14px; line-height: 1;
        padding: 0 3px; border-radius: 4px; border: none; background: transparent; cursor: pointer;
      }
      .close-x:hover { color: #ffffff; background: rgba(255,255,255,0.12); }
      .menu {
        display: none; width: 375px; max-height: 410px; overflow-y: auto;
        background: #0b1326; color: #dae2fd; border: 1px solid rgba(141,144,160,0.35);
        border-radius: 10px; box-shadow: 0 14px 36px rgba(0,0,0,0.6); padding: 8px;
      }
      .menu.open { display: block; }
      .menu-head { font-size: 11px; font-weight: 700; color: #93a1be; padding: 2px 4px 6px; border-bottom: 1px solid rgba(141,144,160,0.22); margin-bottom: 6px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
      .section-lbl { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: #93a1be; padding: 4px 2px 3px; }
      .item {
        width: 100%; text-align: left; display: flex; justify-content: space-between; align-items: center; gap: 8px;
        background: #171f33; color: #dae2fd; border: 1px solid rgba(141,144,160,0.25);
        border-radius: 7px; padding: 7px 9px; margin-bottom: 5px; font-size: 11.5px; cursor: pointer;
      }
      .item:hover { background: #222a3d; border-color: #3b82f6; }
      .item-left { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
      .item-main { font-weight: 600; color: #dae2fd; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
      .item-sub { font-size: 10px; color: #93a1be; font-family: ui-monospace, monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
      .item-badge { font-family: ui-monospace, monospace; font-size: 10.5px; font-weight: 700; color: #68dba9; flex-shrink: 0; text-align: right; }
      .status { font-size: 11px; color: #93a1be; padding: 6px 4px; text-align: center; }
      .status.ok { color: #68dba9; font-weight: 600; }
      .status.err { color: #ffb4ab; }
    `;

    panelEl = document.createElement("div");
    panelEl.className = "wrap";

    pillBtn = document.createElement("div");
    pillBtn.className = "pill";
    const pillDot = document.createElement("span");
    pillDot.className = "pill-dot";
    const pillText = document.createElement("span");
    pillText.textContent = "Unduh Video (TFM)";
    pillBtn.appendChild(pillDot);
    pillBtn.appendChild(pillText);

    const closeBtn = document.createElement("button");
    closeBtn.className = "close-x";
    closeBtn.title = "Sembunyikan di halaman ini";
    closeBtn.textContent = "×";
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      dismissedForUrl = location.href;
      dropdownOpen = false;
      panelEl.style.display = "none";
    });
    pillBtn.appendChild(closeBtn);

    const menuEl = document.createElement("div");
    menuEl.className = "menu";

    pillBtn.addEventListener("click", async (e) => {
      e.stopPropagation();
      dropdownOpen = !dropdownOpen;
      menuEl.classList.toggle("open", dropdownOpen);
      if (dropdownOpen) {
        await populateQualityMenu(menuEl);
      }
    });

    panelEl.appendChild(pillBtn);
    panelEl.appendChild(menuEl);
    shadowRoot.appendChild(style);
    shadowRoot.appendChild(panelEl);
    document.documentElement.appendChild(hostEl);
  }

  function formatBytesDetailed(bytes) {
    if (!bytes || bytes <= 0) return "";
    if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 ** 3)).toFixed(2)} GB`;
    if (bytes >= 1024 * 1024) return `${(bytes / (1024 ** 2)).toFixed(2)} MB`;
    return `${(bytes / 1024).toFixed(1)} KB`;
  }

  function shortCodec(codec) {
    if (!codec || codec === "none") return "";
    const low = codec.toLowerCase();
    if (low.startsWith("avc1") || low.includes("h264")) return "H.264";
    if (low.startsWith("vp09") || low.startsWith("vp9")) return "VP9";
    if (low.startsWith("av01")) return "AV1";
    if (low.startsWith("hev1") || low.startsWith("hvc1")) return "HEVC";
    if (low.startsWith("mp4a")) return "AAC";
    if (low.startsWith("opus")) return "Opus";
    return codec.split(".")[0].toUpperCase();
  }

  function protocolRank(proto) {
    const p = (proto || "").toLowerCase();
    if (p === "https" || p === "http") return 3;
    if (p.includes("dash")) return 2;
    if (p.includes("m3u8")) return 0;
    return 1;
  }

  function codecRank(vcodec) {
    const c = shortCodec(vcodec);
    if (c === "H.264") return 3;
    if (c === "VP9") return 2;
    if (c === "AV1") return 1;
    return 0;
  }

  function renderMenuMessage(menuEl, titleText, statusText, statusKind) {
    menuEl.replaceChildren();
    const head = document.createElement("div");
    head.className = "menu-head";
    head.textContent = titleText;
    menuEl.appendChild(head);
    if (statusText) {
      const st = document.createElement("div");
      st.className = statusKind ? `status ${statusKind}` : "status";
      st.textContent = statusText;
      menuEl.appendChild(st);
    }
  }

  async function populateQualityMenu(menuEl) {
    const cleanTitle = (document.title || "video").replace(/\s*-\s*YouTube$/i, "").trim();
    renderMenuMessage(menuEl, cleanTitle, "Menganalisis semua resolusi & ukuran file...", "");

    chrome.runtime.sendMessage(
      { type: "get_detected_media", tabUrl: location.href, tabTitle: cleanTitle },
      (mediaRes) => {
        const items = mediaRes?.media || [];
        const primary = items[0] || {
          url: location.href,
          title: cleanTitle,
          kind: "VIDEO",
          isPageVideo: true
        };

        chrome.runtime.sendMessage(
          { type: "probe_media", url: primary.url, referrer: location.href },
          (probeRes) => {
            renderMenuMessage(menuEl, cleanTitle, "", "");

            const addOptionBtn = (mainLabel, subLabel, badge, opts) => {
              const btn = document.createElement("button");
              btn.className = "item";

              const left = document.createElement("div");
              left.className = "item-left";
              const mainSpan = document.createElement("span");
              mainSpan.className = "item-main";
              mainSpan.textContent = mainLabel;
              left.appendChild(mainSpan);
              if (subLabel) {
                const subSpan = document.createElement("span");
                subSpan.className = "item-sub";
                subSpan.textContent = subLabel;
                left.appendChild(subSpan);
              }
              const badgeSpan = document.createElement("span");
              badgeSpan.className = "item-badge";
              badgeSpan.textContent = badge;
              btn.appendChild(left);
              btn.appendChild(badgeSpan);

              btn.addEventListener("click", () => {
                renderMenuMessage(menuEl, cleanTitle, "Mengirim ke Take File Manager...", "");
                chrome.runtime.sendMessage(
                  {
                    type: "send_media_download",
                    url: opts.url,
                    referrer: location.href,
                    formatId: opts.formatId,
                    audioOnly: opts.audioOnly,
                    fileName: opts.fileName || cleanTitle,
                    isPageVideo: opts.isPageVideo
                  },
                  (dlRes) => {
                    if (dlRes && dlRes.ok) {
                      renderMenuMessage(menuEl, cleanTitle, "✓ Ditambahkan ke Take File Manager!", "ok");
                      setTimeout(() => {
                        dropdownOpen = false;
                        menuEl.classList.remove("open");
                      }, 1500);
                    } else {
                      const errMsg = dlRes?.error || "Buka aplikasi Take File Manager terlebih dahulu.";
                      renderMenuMessage(menuEl, cleanTitle, errMsg, "err");
                    }
                  }
                );
              });
              menuEl.appendChild(btn);
            };

            const data = probeRes?.resp?.data;
            const resolvedTitle = data?.title || cleanTitle;

            addOptionBtn(
              "★ Kualitas Terbaik (Auto Merge MP4)",
              "Pilih otomatis resolusi tertinggi + audio terbaik",
              "BEST",
              {
                url: primary.url,
                fileName: resolvedTitle,
                isPageVideo: primary.isPageVideo
              }
            );

            const allFormats = Array.isArray(data?.formats)
              ? data.formats.filter((f) => f.ext?.toLowerCase() !== "mhtml" && (f.has_video || f.has_audio))
              : [];

            if (allFormats.length > 0) {
              const audioTracks = allFormats
                .filter((f) => !f.has_video && f.has_audio && f.filesize > 0)
                .sort((a, b) => (b.filesize || 0) - (a.filesize || 0));
              const bestM4aAudio = audioTracks.find((a) => a.ext === "m4a")?.filesize || 0;
              const bestWebmAudio = audioTracks.find((a) => a.ext === "webm")?.filesize || 0;
              const bestAnyAudio = audioTracks[0]?.filesize || 0;

              const videoFmts = allFormats
                .filter((f) => f.has_video)
                .sort((a, b) => {
                  const hDiff = (b.height || 0) - (a.height || 0);
                  if (hDiff !== 0) return hDiff;
                  const pDiff = protocolRank(b.protocol) - protocolRank(a.protocol);
                  if (pDiff !== 0) return pDiff;
                  const eDiff = Number(!b.is_filesize_estimated) - Number(!a.is_filesize_estimated);
                  if (eDiff !== 0) return eDiff;
                  const sDiff = Number(Boolean(b.filesize)) - Number(Boolean(a.filesize));
                  if (sDiff !== 0) return sDiff;
                  return codecRank(b.vcodec) - codecRank(a.vcodec);
                });

              const vidLbl = document.createElement("div");
              vidLbl.className = "section-lbl";
              vidLbl.textContent = "Semua Resolusi Video";
              menuEl.appendChild(vidLbl);

              const seen = new Set();
              for (const f of videoFmts) {
                const height = f.height || 0;
                const res = height > 0 ? `${height}p` : (f.resolution || f.ext.toUpperCase());
                const codec = shortCodec(f.vcodec);
                const extUp = (f.ext || "mp4").toUpperCase();
                const key = `${res}|${extUp}|${codec}|${f.has_audio ? "mux" : "sep"}`;
                if (seen.has(key)) continue;
                seen.add(key);

                const needsAudio = !f.has_audio && f.format_id !== "direct";
                const matchedAudio = needsAudio
                  ? extUp === "WEBM"
                    ? bestWebmAudio || bestAnyAudio
                    : bestM4aAudio || bestAnyAudio
                  : 0;
                const baseSz = f.filesize || 0;
                const totalSz = baseSz > 0 ? baseSz + matchedAudio : 0;

                const fpsPart = f.fps ? ` • ${Math.round(f.fps)}fps` : "";
                const kbpsPart = f.tbr ? ` • ${Math.round(f.tbr)} kbps` : "";
                const detailSub =
                  needsAudio && baseSz > 0 && matchedAudio > 0
                    ? `Video ${formatBytesDetailed(baseSz)} + Audio ${formatBytesDetailed(matchedAudio)}${kbpsPart}`
                    : `${f.resolution || extUp}${fpsPart}${kbpsPart}`;

                const badgeSize =
                  totalSz > 0
                    ? `${f.is_filesize_estimated || (needsAudio && matchedAudio > 0) ? "~" : ""}${formatBytesDetailed(totalSz)}`
                    : f.format_id;

                addOptionBtn(
                  `${res} [${extUp}${codec ? ` • ${codec}` : ""}${needsAudio ? " +Audio" : ""}]`,
                  detailSub,
                  badgeSize,
                  {
                    url: primary.url,
                    formatId: f.has_audio || f.format_id === "direct" ? f.format_id : `${f.format_id}+ba/${f.format_id}`,
                    fileName: resolvedTitle,
                    isPageVideo: primary.isPageVideo
                  }
                );
              }
            } else if (!probeRes?.ok && probeRes?.error) {
              const warn = document.createElement("div");
              warn.className = "status err";
              warn.textContent = probeRes.error;
              menuEl.appendChild(warn);
            }

            const audLbl = document.createElement("div");
            audLbl.className = "section-lbl";
            audLbl.textContent = "Audio Saja";
            menuEl.appendChild(audLbl);

            addOptionBtn(
              "Audio MP3 (Konversi Terbaik)",
              "Ekstrak audio terbaik ke MP3",
              "MP3",
              {
                url: primary.url,
                audioOnly: true,
                fileName: resolvedTitle,
                isPageVideo: primary.isPageVideo
              }
            );

            const seenAudio = new Set();
            allFormats
              .filter((f) => !f.has_video && f.has_audio)
              .sort((a, b) => (b.tbr || 0) - (a.tbr || 0))
              .forEach((a) => {
                const extUp = (a.ext || "m4a").toUpperCase();
                const codec = shortCodec(a.acodec);
                const kbps = a.tbr ? Math.round(a.tbr) : 0;
                const aKey = `${extUp}|${codec}|${kbps}`;
                if (seenAudio.has(aKey)) return;
                seenAudio.add(aKey);
                addOptionBtn(
                  `Audio [${extUp}${codec ? ` • ${codec}` : ""}]`,
                  kbps > 0 ? `${kbps} kbps${a.format_note ? ` • ${a.format_note}` : ""}` : (a.format_note || extUp),
                  a.filesize > 0 ? `${a.is_filesize_estimated ? "~" : ""}${formatBytesDetailed(a.filesize)}` : a.format_id,
                  {
                    url: primary.url,
                    formatId: a.format_id,
                    audioOnly: false,
                    fileName: resolvedTitle,
                    isPageVideo: primary.isPageVideo
                  }
                );
              });

            for (const extra of items.slice(1, 4)) {
              addOptionBtn(extra.title || extra.kind, extra.url, extra.kind, {
                url: extra.url,
                fileName: resolvedTitle
              });
            }
          }
        );
      }
    );
  }

  function positionOverlay() {
    if (dismissedForUrl === location.href) return;
    const video = collectPageVideoInfo();
    activeVideo = video;
    if (!video) {
      if (panelEl && !dropdownOpen) panelEl.style.display = "none";
      return;
    }
    ensureOverlayUi();
    const rect = video.getBoundingClientRect();
    if (rect.width < 160 || rect.height < 90 || rect.bottom < 20 || rect.top > window.innerHeight - 20) {
      if (!dropdownOpen) panelEl.style.display = "none";
      return;
    }
    const top = Math.max(10, Math.min(window.innerHeight - 50, rect.top + 12));
    const right = Math.max(12, window.innerWidth - rect.right + 12);
    panelEl.style.top = `${top}px`;
    panelEl.style.right = `${right}px`;
    panelEl.style.left = "auto";
    panelEl.style.display = "flex";
  }

  setInterval(positionOverlay, 1200);
  window.addEventListener("scroll", positionOverlay, { passive: true });
  window.addEventListener("resize", positionOverlay, { passive: true });
  document.addEventListener("play", positionOverlay, true);
  setTimeout(positionOverlay, 800);
})();
