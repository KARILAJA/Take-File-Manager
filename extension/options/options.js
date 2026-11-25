const DEFAULT_EXTENSIONS = [
  "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "tgz", "torrent",
  "exe", "msi", "apk", "dmg", "deb", "rpm",
  "mp4", "mkv", "avi", "mov", "webm", "mp3", "flac", "pdf"
];

document.addEventListener("DOMContentLoaded", async () => {
  const enabledEl = document.getElementById("enabled");
  const askSaveEl = document.getElementById("askSaveLocation");
  const minSizeEl = document.getElementById("minSizeKb");
  const extEl = document.getElementById("extensions");
  const exclEl = document.getElementById("excluded");
  const saveBtn = document.getElementById("saveBtn");
  const resetExtBtn = document.getElementById("resetExtBtn");
  const statusMsg = document.getElementById("statusMsg");
  const themeBtn = document.getElementById("themeBtn");

  const applyTheme = (mode) => {
    document.body.classList.toggle("light", mode === "light");
    themeBtn.textContent = mode === "light" ? "Mode Gelap" : "Mode Terang";
  };

  const stored = await chrome.storage.local.get(["turbogetSettings", "tfmExtTheme"]);
  const cfg = stored.turbogetSettings || {};
  applyTheme(stored.tfmExtTheme || "dark");

  themeBtn.addEventListener("click", async () => {
    const next = document.body.classList.contains("light") ? "dark" : "light";
    applyTheme(next);
    await chrome.storage.local.set({ tfmExtTheme: next });
  });

  enabledEl.checked = cfg.enabled !== undefined ? cfg.enabled : true;
  askSaveEl.checked = cfg.askSaveLocation !== undefined ? cfg.askSaveLocation : true;
  minSizeEl.value = Math.round((cfg.minFileSizeBytes ?? 1024 * 1024) / 1024);
  extEl.value = (cfg.captureExtensions || DEFAULT_EXTENSIONS).join(", ");
  exclEl.value = (cfg.excludedHosts || []).join("\n");

  document.querySelectorAll(".size-preset").forEach((btn) => {
    btn.addEventListener("click", () => {
      minSizeEl.value = btn.getAttribute("data-kb") || "1024";
    });
  });

  resetExtBtn.addEventListener("click", () => {
    extEl.value = DEFAULT_EXTENSIONS.join(", ");
  });

  saveBtn.addEventListener("click", async () => {
    const captureExtensions = extEl.value
      .split(",")
      .map((s) => s.trim().toLowerCase())
      .filter(Boolean);
    const excludedHosts = exclEl.value
      .split(/\r?\n/)
      .map((s) => s.trim().toLowerCase())
      .filter(Boolean);

    const minKb = parseInt(minSizeEl.value, 10);
    const minFileSizeBytes = Number.isFinite(minKb) && minKb >= 0 ? minKb * 1024 : 1024 * 1024;

    await chrome.storage.local.set({
      turbogetSettings: {
        enabled: enabledEl.checked,
        askSaveLocation: askSaveEl.checked,
        minFileSizeBytes,
        captureExtensions,
        excludedHosts
      }
    });

    statusMsg.textContent = "Pengaturan disimpan!";
    setTimeout(() => {
      statusMsg.textContent = "";
    }, 2500);
  });
});
