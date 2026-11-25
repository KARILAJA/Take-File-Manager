document.addEventListener("DOMContentLoaded", async () => {
  const badge = document.getElementById("statusBadge");
  const statusText = document.getElementById("statusText");
  const toggleCapture = document.getElementById("toggleCapture");
  const toggleAskSave = document.getElementById("toggleAskSave");
  const mediaList = document.getElementById("mediaList");
  const mediaCount = document.getElementById("mediaCount");
  const openOptions = document.getElementById("openOptions");
  const themeBtn = document.getElementById("themeBtn");

  const applyTheme = (mode) => {
    document.body.classList.toggle("light", mode === "light");
    themeBtn.textContent = mode === "light" ? "\u2600" : "\u263E";
  };

  if (chrome.storage?.local) {
    const t = await chrome.storage.local.get("tfmExtTheme");
    applyTheme(t.tfmExtTheme || "dark");
  }

  themeBtn.addEventListener("click", async () => {
    const next = document.body.classList.contains("light") ? "dark" : "light";
    applyTheme(next);
    await chrome.storage.local.set({ tfmExtTheme: next });
  });

  chrome.runtime.sendMessage({ type: "get_status" }, (resp) => {
    if (!resp) return;
    toggleCapture.checked = Boolean(resp.enabled);
    toggleAskSave.checked = resp.askSaveLocation !== false;
    if (resp.ok && resp.ping?.coreRunning) {
      statusText.textContent = "Core Online";
      badge.className = "badge online";
    } else {
      statusText.textContent = "Core Offline";
      badge.className = "badge offline";
    }
  });

  toggleCapture.addEventListener("change", () => {
    chrome.runtime.sendMessage({ type: "toggle_enabled", enabled: toggleCapture.checked });
  });

  toggleAskSave.addEventListener("change", () => {
    chrome.runtime.sendMessage({ type: "toggle_ask_save", askSaveLocation: toggleAskSave.checked });
  });

  openOptions.addEventListener("click", (e) => {
    e.preventDefault();
    chrome.runtime.openOptionsPage();
  });

  const formatSize = (bytes) => {
    if (!bytes || bytes <= 0) return "";
    if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 ** 3)).toFixed(2)} GB`;
    if (bytes >= 1024 * 1024) return `${(bytes / (1024 ** 2)).toFixed(2)} MB`;
    return `${(bytes / 1024).toFixed(1)} KB`;
  };

  const shortCodec = (codec) => {
    if (!codec || codec === "none") return "";
    const low = codec.toLowerCase();
    if (low.startsWith("avc1") || low.includes("h264")) return "H.264";
    if (low.startsWith("vp09") || low.startsWith("vp9")) return "VP9";
    if (low.startsWith("av01")) return "AV1";
    if (low.startsWith("hev1") || low.startsWith("hvc1")) return "HEVC";
    if (low.startsWith("mp4a")) return "AAC";
    if (low.startsWith("opus")) return "Opus";
    return codec.split(".")[0].toUpperCase();
  };

  const protocolRank = (proto) => {
    const p = (proto || "").toLowerCase();
    if (p === "https" || p === "http") return 3;
    if (p.includes("dash")) return 2;
    if (p.includes("m3u8")) return 0;
    return 1;
  };

  const codecRank = (vcodec) => {
    const c = shortCodec(vcodec);
    if (c === "H.264") return 3;
    if (c === "VP9") return 2;
    if (c === "AV1") return 1;
    return 0;
  };

  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (tab?.id) {
    chrome.runtime.sendMessage(
      { type: "get_detected_media", tabId: tab.id, tabUrl: tab.url, tabTitle: tab.title },
      (res) => {
        const items = res?.media || [];
        mediaCount.textContent = `${items.length} item`;
        if (items.length === 0) return;

        mediaList.replaceChildren();
        items.forEach((m) => {
          const row = document.createElement("div");
          row.className = "media-item";

          const topRow = document.createElement("div");
          topRow.className = "media-top";

          const pill = document.createElement("span");
          pill.className = "kind-pill";
          pill.textContent = (m.kind || "MEDIA").toUpperCase();

          const label = document.createElement("span");
          label.className = "media-url";
          label.title = m.url;
          label.textContent = (m.title || m.url) + (m.size ? ` (${formatSize(m.size)})` : "");

          const select = document.createElement("select");
          const bestOpt = document.createElement("option");
          bestOpt.value = "best";
          bestOpt.textContent = "Terbaik (Auto Merge MP4)";
          const audioOpt = document.createElement("option");
          audioOpt.value = "audio_only";
          audioOpt.textContent = "Audio Saja (Konversi MP3)";
          select.appendChild(bestOpt);
          select.appendChild(audioOpt);

          let resolvedTitle = m.title || "";
          const hint = document.createElement("div");
          hint.className = "media-hint";
          hint.textContent = "Memeriksa semua resolusi & ukuran file...";

          chrome.runtime.sendMessage(
            { type: "probe_media", url: m.url, referrer: tab.url },
            (probeResp) => {
              const data = probeResp?.resp?.data;
              const rawFormats = Array.isArray(data?.formats)
                ? data.formats.filter((f) => f.ext?.toLowerCase() !== "mhtml" && (f.has_video || f.has_audio))
                : [];
              if (data?.title) {
                resolvedTitle = data.title;
                label.textContent = data.title;
              }
              if (rawFormats.length > 0) {
                const audioTracks = rawFormats
                  .filter((f) => !f.has_video && f.has_audio && f.filesize > 0)
                  .sort((a, b) => (b.filesize || 0) - (a.filesize || 0));
                const bestM4aAudio = audioTracks.find((a) => a.ext === "m4a")?.filesize || 0;
                const bestWebmAudio = audioTracks.find((a) => a.ext === "webm")?.filesize || 0;
                const bestAnyAudio = audioTracks[0]?.filesize || 0;

                const sorted = rawFormats
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

                const seen = new Set();
                sorted.forEach((f) => {
                  const resLabel = f.height ? `${f.height}p` : (f.resolution || f.ext.toUpperCase());
                  const extUp = (f.ext || "mp4").toUpperCase();
                  const codec = shortCodec(f.vcodec);
                  const key = `${resLabel}|${extUp}|${codec}|${f.has_audio ? "mux" : "sep"}`;
                  if (seen.has(key)) return;
                  seen.add(key);

                  const opt = document.createElement("option");
                  const needsAudio = !f.has_audio && f.format_id !== "direct";
                  opt.value = !needsAudio ? f.format_id : `${f.format_id}+ba/${f.format_id}`;

                  const matchedAudio = needsAudio
                    ? extUp === "WEBM"
                      ? bestWebmAudio || bestAnyAudio
                      : bestM4aAudio || bestAnyAudio
                    : 0;
                  const baseSz = f.filesize || 0;
                  const totalSz = baseSz > 0 ? baseSz + matchedAudio : 0;
                  const szText =
                    totalSz > 0
                      ? ` — ${f.is_filesize_estimated || (needsAudio && matchedAudio > 0) ? "~" : ""}${formatSize(totalSz)}`
                      : "";
                  opt.textContent = `${resLabel} [${extUp}${codec ? ` • ${codec}` : ""}${needsAudio ? " +Audio" : ""}]${szText}`;
                  select.appendChild(opt);
                });

                const seenAudio = new Set();
                rawFormats
                  .filter((f) => !f.has_video && f.has_audio)
                  .sort((a, b) => (b.tbr || 0) - (a.tbr || 0))
                  .forEach((a) => {
                    const extUp = (a.ext || "m4a").toUpperCase();
                    const codec = shortCodec(a.acodec);
                    const kbps = a.tbr ? Math.round(a.tbr) : 0;
                    const aKey = `${extUp}|${codec}|${kbps}`;
                    if (seenAudio.has(aKey)) return;
                    seenAudio.add(aKey);
                    const opt = document.createElement("option");
                    opt.value = a.format_id;
                    const szText = a.filesize > 0 ? ` — ${a.is_filesize_estimated ? "~" : ""}${formatSize(a.filesize)}` : "";
                    opt.textContent = `Audio [${extUp}${codec ? ` • ${codec}` : ""}${kbps ? ` • ${kbps}k` : ""}]${szText}`;
                    select.appendChild(opt);
                  });

                hint.textContent = `${seen.size} resolusi video + ${seenAudio.size} audio tersedia`;
              } else if (probeResp && !probeResp.ok) {
                hint.textContent = probeResp.error || "Siap diunduh";
              } else {
                hint.textContent = "Siap diunduh langsung";
              }
            }
          );

          const btn = document.createElement("button");
          btn.className = "btn-primary";
          btn.textContent = "Unduh";
          select.addEventListener("change", () => {
            btn.disabled = false;
            btn.textContent = "Unduh";
          });
          btn.addEventListener("click", () => {
            const val = select.value;
            btn.textContent = "Mengirim...";
            btn.disabled = true;
            chrome.runtime.sendMessage(
              {
                type: "send_media_download",
                url: m.url,
                referrer: tab.url,
                fileName: resolvedTitle || undefined,
                isPageVideo: Boolean(m.isPageVideo),
                formatId: val !== "best" && val !== "audio_only" ? val : undefined,
                audioOnly: val === "audio_only"
              },
              (dlResp) => {
                if (dlResp && dlResp.ok) {
                  btn.textContent = "Terkirim \u2713";
                  hint.textContent = "Masuk ke daftar unduhan Take File Manager";
                } else {
                  btn.disabled = false;
                  btn.textContent = "Coba Lagi";
                  hint.textContent = dlResp?.error || "Gagal menghubungi Take File Manager";
                }
              }
            );
          });

          const controls = document.createElement("div");
          controls.className = "media-controls";
          controls.appendChild(select);
          controls.appendChild(btn);

          topRow.appendChild(pill);
          topRow.appendChild(label);
          row.appendChild(topRow);
          row.appendChild(controls);
          row.appendChild(hint);
          mediaList.appendChild(row);
        });
      }
    );
  }
});
