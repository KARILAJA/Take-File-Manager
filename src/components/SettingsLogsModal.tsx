import React, { useEffect, useState } from "react";
import {
  X,
  Settings,
  Shield,
  ScrollText,
  RefreshCw,
  CheckCircle,
  Palette,
  FolderCog,
  Folder,
  Puzzle,
  AlertTriangle,
  ExternalLink,
  Copy,
  Power,
  Sun,
  Moon,
} from "lucide-react";
import { useAppStore, NetworkSettings, AgentAccessPolicy, CategoryRecord } from "../store";
import { formatDate } from "../utils/format";

export const SettingsLogsModal: React.FC = () => {
  const {
    isSettingsModalOpen,
    setSettingsModalOpen,
    settingsTab,
    setSettingsTab,
    theme,
    setTheme,
    networkSettings,
    agentPolicy,
    categories,
    auditLogs,
    autostartEnabled,
    browsers,
    extensionDir,
    loadSettingsAndLogs,
    loadSystemIntegration,
    setAutostartEnabled,
    integrateBrowser,
    saveNetworkSettings,
    saveAgentPolicy,
    updateCategory,
    checkAppUpdate,
    pickFolder,
    openFolder,
    t,
  } = useAppStore();

  const activeTab = settingsTab;
  const setActiveTab = setSettingsTab;
  const [netForm, setNetForm] = useState<NetworkSettings>(networkSettings);
  const [policyForm, setPolicyForm] = useState<AgentAccessPolicy>(agentPolicy);
  const [catDrafts, setCatDrafts] = useState<CategoryRecord[]>(categories);
  const [allowedDirsText, setAllowedDirsText] = useState("");
  const [savedNotice, setSavedNotice] = useState<string | null>(null);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [browserNotice, setBrowserNotice] = useState<string | null>(null);

  useEffect(() => {
    if (isSettingsModalOpen) {
      loadSettingsAndLogs();
      loadSystemIntegration();
    }
  }, [isSettingsModalOpen, loadSettingsAndLogs, loadSystemIntegration]);

  useEffect(() => {
    if (!isSettingsModalOpen || activeTab !== "browsers") return;
    const timer = setInterval(() => {
      loadSystemIntegration();
    }, 2500);
    return () => clearInterval(timer);
  }, [isSettingsModalOpen, activeTab, loadSystemIntegration]);

  useEffect(() => {
    setNetForm(networkSettings);
  }, [networkSettings]);

  useEffect(() => {
    setPolicyForm(agentPolicy);
    setAllowedDirsText((agentPolicy.allowed_dirs || []).join("\n"));
  }, [agentPolicy]);

  useEffect(() => {
    setCatDrafts(categories);
  }, [categories]);

  if (!isSettingsModalOpen) return null;

  const handleSaveNetwork = async () => {
    await saveNetworkSettings(netForm);
    setSavedNotice("Pengaturan jaringan disimpan");
    setTimeout(() => setSavedNotice(null), 2500);
  };

  const handleSavePolicy = async () => {
    const dirs = allowedDirsText
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter(Boolean);
    await saveAgentPolicy({ ...policyForm, allowed_dirs: dirs });
    setSavedNotice("Kebijakan akses agent disimpan");
    setTimeout(() => setSavedNotice(null), 2500);
  };

  const handleSaveCategories = async () => {
    for (const c of catDrafts) {
      await updateCategory(c);
    }
    setSavedNotice("Aturan kategori berkas disimpan");
    setTimeout(() => setSavedNotice(null), 2500);
  };

  const handleCheckUpdate = async () => {
    try {
      const res = await checkAppUpdate(netForm.update_feed_url);
      if (res.update_available) {
        setUpdateMsg(`Tersedia versi baru v${res.latest_version}!`);
      } else {
        setUpdateMsg(`Take File Manager sudah versi terbaru (v${res.current_version})`);
      }
    } catch (e) {
      setUpdateMsg(`Pemeriksaan pembaruan: ${String(e)}`);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="w-full max-w-3xl rounded-xl border border-tg-outline/40 bg-tg-card shadow-2xl overflow-hidden text-xs">
        <div className="flex items-center justify-between border-b border-tg-outline/30 px-4 py-3 bg-tg-low">
          <div className="flex items-center gap-2 text-tg-ink font-semibold">
            <Settings className="h-4 w-4 text-tg-primary" />
            <span>{t("settings")} &amp; Log Viewer</span>
          </div>
          <button
            onClick={() => setSettingsModalOpen(false)}
            className="rounded-md p-1 text-tg-muted hover:bg-tg-high hover:text-tg-ink"
            aria-label="Close Settings"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Stitch Horizontal Tabs */}
        <div className="flex overflow-x-auto border-b border-tg-outline/30 bg-tg-lowest/70 px-4">
          <button
            onClick={() => setActiveTab("network")}
            className={`flex items-center gap-1.5 border-b-2 py-2.5 px-3 font-medium whitespace-nowrap transition ${
              activeTab === "network"
                ? "border-tg-primary text-tg-primary"
                : "border-transparent text-tg-muted hover:text-tg-ink"
            }`}
          >
            <Settings className="h-3.5 w-3.5" />
            Jaringan &amp; Startup
          </button>
          <button
            onClick={() => {
              setActiveTab("browsers");
              loadSystemIntegration();
            }}
            className={`flex items-center gap-1.5 border-b-2 py-2.5 px-3 font-medium whitespace-nowrap transition ${
              activeTab === "browsers"
                ? "border-tg-primary text-tg-primary"
                : "border-transparent text-tg-muted hover:text-tg-ink"
            }`}
          >
            <Puzzle className="h-3.5 w-3.5" />
            Integrasi Browser
          </button>
          <button
            onClick={() => setActiveTab("appearance")}
            className={`flex items-center gap-1.5 border-b-2 py-2.5 px-3 font-medium whitespace-nowrap transition ${
              activeTab === "appearance"
                ? "border-tg-primary text-tg-primary"
                : "border-transparent text-tg-muted hover:text-tg-ink"
            }`}
          >
            <Palette className="h-3.5 w-3.5" />
            Tampilan &amp; Tema
          </button>
          <button
            onClick={() => setActiveTab("categories")}
            className={`flex items-center gap-1.5 border-b-2 py-2.5 px-3 font-medium whitespace-nowrap transition ${
              activeTab === "categories"
                ? "border-tg-primary text-tg-primary"
                : "border-transparent text-tg-muted hover:text-tg-ink"
            }`}
          >
            <FolderCog className="h-3.5 w-3.5" />
            Kategori &amp; Rute
          </button>
          <button
            onClick={() => setActiveTab("agent")}
            className={`flex items-center gap-1.5 border-b-2 py-2.5 px-3 font-medium whitespace-nowrap transition ${
              activeTab === "agent"
                ? "border-tg-primary text-tg-primary"
                : "border-transparent text-tg-muted hover:text-tg-ink"
            }`}
          >
            <Shield className="h-3.5 w-3.5" />
            Akses Agent (MCP/CLI)
          </button>
          <button
            onClick={() => {
              setActiveTab("logs");
              loadSettingsAndLogs();
            }}
            className={`flex items-center gap-1.5 border-b-2 py-2.5 px-3 font-medium whitespace-nowrap transition ${
              activeTab === "logs"
                ? "border-tg-primary text-tg-primary"
                : "border-transparent text-tg-muted hover:text-tg-ink"
            }`}
          >
            <ScrollText className="h-3.5 w-3.5" />
            Log &amp; Audit ({auditLogs.length})
          </button>
        </div>

        {/* Body */}
        <div className="p-4 max-h-[440px] overflow-auto space-y-3 text-tg-ink">
          {savedNotice && (
            <div className="flex items-center gap-1.5 rounded-lg border border-tg-secondary/40 bg-tg-secondary/15 px-3 py-1.5 text-tg-secondary">
              <CheckCircle className="h-3.5 w-3.5" />
              <span>{savedNotice}</span>
            </div>
          )}

          {activeTab === "network" && (
            <div className="space-y-3.5">
              {/* AutoStart on Windows Startup (Runs in Background System Tray) */}
              <div className="flex items-center justify-between gap-3 rounded-xl border border-tg-primary/30 bg-tg-primary-container/10 p-3">
                <div className="flex items-start gap-2.5">
                  <Power className="h-4 w-4 text-tg-secondary mt-0.5 flex-shrink-0" />
                  <div>
                    <div className="font-semibold text-tg-ink">
                      AutoStart Saat Komputer Menyala (Latar Belakang)
                    </div>
                    <p className="text-[11px] text-tg-muted">
                      Take File Manager langsung berjalan otomatis di System Tray (--minimized) tanpa mengganggu layar saat Windows dinyalakan.
                    </p>
                  </div>
                </div>
                <label className="inline-flex items-center gap-2 cursor-pointer flex-shrink-0">
                  <input
                    type="checkbox"
                    aria-label="AutoStart Windows"
                    checked={autostartEnabled}
                    onChange={async (e) => {
                      const next = await setAutostartEnabled(e.target.checked);
                      setSavedNotice(
                        next
                          ? "AutoStart di latar belakang diaktifkan"
                          : "AutoStart dinonaktifkan"
                      );
                      setTimeout(() => setSavedNotice(null), 2500);
                    }}
                    className="h-4 w-4 accent-tg-secondary"
                  />
                  <span className="font-mono text-[11px] font-semibold text-tg-secondary">
                    {autostartEnabled ? "AKTIF" : "NONAKTIF"}
                  </span>
                </label>
              </div>

              <div>
                <label className="block text-tg-ink font-medium mb-1">
                  Limiter Kecepatan Global (KB/s, 0 = Tanpa Batas)
                </label>
                <div className="flex flex-wrap items-center gap-2">
                  <input
                    type="number"
                    min={0}
                    value={netForm.speed_limit_kbps || 0}
                    onChange={(e) =>
                      setNetForm({
                        ...netForm,
                        speed_limit_kbps: Math.max(0, parseInt(e.target.value || "0", 10) || 0),
                      })
                    }
                    className="w-36 rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 text-tg-ink font-mono"
                  />
                  {[0, 256, 512, 1024, 5120].map((v) => (
                    <button
                      key={v}
                      type="button"
                      onClick={() => setNetForm({ ...netForm, speed_limit_kbps: v })}
                      className={`rounded-lg border px-2.5 py-1 text-[11px] ${
                        (netForm.speed_limit_kbps || 0) === v
                          ? "border-tg-primary bg-tg-primary-container/20 text-tg-primary font-semibold"
                          : "border-tg-outline/40 bg-tg-high text-tg-muted hover:text-tg-ink"
                      }`}
                    >
                      {v === 0 ? "Tanpa Batas" : v >= 1024 ? `${v / 1024} MB/s` : `${v} KB/s`}
                    </button>
                  ))}
                </div>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div>
                  <label className="block text-tg-ink font-medium mb-1">Mode Proxy</label>
                  <select
                    value={netForm.proxy_mode}
                    onChange={(e) =>
                      setNetForm({
                        ...netForm,
                        proxy_mode: e.target.value as NetworkSettings["proxy_mode"],
                      })
                    }
                    className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 text-tg-ink"
                  >
                    <option value="system">Ikuti Sistem (Windows Proxy)</option>
                    <option value="direct">Langsung (Tanpa Proxy)</option>
                    <option value="manual">Manual (HTTP / HTTPS / SOCKS5)</option>
                  </select>
                </div>

                <div>
                  <label className="block text-tg-ink font-medium mb-1">Maks Koneksi per Host</label>
                  <input
                    type="number"
                    min={1}
                    max={32}
                    value={netForm.max_connections_per_host || 6}
                    onChange={(e) =>
                      setNetForm({
                        ...netForm,
                        max_connections_per_host: Math.min(32, Math.max(1, parseInt(e.target.value || "6", 10) || 6)),
                      })
                    }
                    className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 text-tg-ink font-mono"
                  />
                </div>
              </div>

              {netForm.proxy_mode === "manual" && (
                <div className="space-y-2 rounded-lg border border-tg-outline/30 bg-tg-lowest p-3">
                  <div>
                    <label className="block text-tg-muted mb-1">URL Proxy</label>
                    <input
                      type="text"
                      value={netForm.proxy_url || ""}
                      onChange={(e) => setNetForm({ ...netForm, proxy_url: e.target.value })}
                      placeholder="http://127.0.0.1:8888"
                      className="w-full rounded-lg border border-tg-outline/40 bg-tg-card px-2.5 py-1.5 text-tg-ink font-mono"
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <label className="block text-tg-muted mb-1">Username Proxy</label>
                      <input
                        type="text"
                        value={netForm.proxy_username || ""}
                        onChange={(e) =>
                          setNetForm({ ...netForm, proxy_username: e.target.value })
                        }
                        className="w-full rounded-lg border border-tg-outline/40 bg-tg-card px-2.5 py-1.5 text-tg-ink"
                      />
                    </div>
                    <div>
                      <label className="block text-tg-muted mb-1">Password Proxy</label>
                      <input
                        type="password"
                        value={netForm.proxy_password || ""}
                        onChange={(e) =>
                          setNetForm({ ...netForm, proxy_password: e.target.value })
                        }
                        className="w-full rounded-lg border border-tg-outline/40 bg-tg-card px-2.5 py-1.5 text-tg-ink"
                      />
                    </div>
                  </div>
                </div>
              )}

              <div className="flex items-center justify-between pt-2 border-t border-tg-outline/30">
                <button
                  onClick={handleCheckUpdate}
                  className="flex items-center gap-1.5 rounded-lg border border-tg-outline/40 bg-tg-high px-3 py-1.5 text-tg-ink hover:bg-tg-highest"
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                  Periksa Pembaruan
                </button>
                <button
                  onClick={handleSaveNetwork}
                  className="rounded-lg bg-tg-primary-container px-4 py-1.5 font-semibold text-white hover:brightness-110"
                >
                  Simpan Pengaturan Jaringan
                </button>
              </div>
              {updateMsg && <p className="text-[11px] text-tg-muted font-mono">{updateMsg}</p>}
            </div>
          )}

          {activeTab === "browsers" && (
            <div className="space-y-3.5">
              <div className="rounded-xl border border-tg-tertiary/40 bg-tg-tertiary/10 p-3 flex items-start gap-2.5">
                <AlertTriangle className="h-4 w-4 text-tg-tertiary mt-0.5 flex-shrink-0" />
                <div className="space-y-1 text-[11px] leading-relaxed">
                  <div className="font-semibold text-tg-tertiary">
                    Peringatan Wajib: Aktifkan Developer Mode pada Browser
                  </div>
                  <p className="text-tg-ink/90">
                    Untuk browser berbasis Chromium (<strong>Google Chrome</strong>, <strong>Microsoft Edge</strong>, dan <strong>Opera</strong>), setelah mengklik tombol <strong>Integrasikan</strong>, Anda wajib mengaktifkan <strong>Developer mode (Mode pengembang)</strong> di halaman Ekstensi browser lalu klik <strong>Load unpacked</strong> dan tempel (<code>Ctrl+V</code>) lokasi folder ekstensi.
                  </p>
                </div>
              </div>

              {browserNotice && (
                <div className="rounded-lg border border-tg-primary/40 bg-tg-primary-container/15 px-3 py-2 text-[11px] text-tg-ink">
                  {browserNotice}
                </div>
              )}

              {extensionDir && (
                <div className="flex items-center justify-between gap-2 rounded-lg border border-tg-outline/30 bg-tg-lowest px-3 py-2">
                  <div className="min-w-0">
                    <div className="text-[10px] uppercase tracking-wider text-tg-muted font-semibold">
                      Folder Ekstensi Lokal
                    </div>
                    <div className="font-mono text-[11px] text-tg-ink truncate">{extensionDir}</div>
                  </div>
                  <div className="flex items-center gap-1.5 flex-shrink-0">
                    <button
                      type="button"
                      onClick={async () => {
                        try {
                          await navigator.clipboard?.writeText(extensionDir);
                          setBrowserNotice("Path folder ekstensi disalin ke clipboard!");
                        } catch {
                          setBrowserNotice(`Salin manual: ${extensionDir}`);
                        }
                      }}
                      className="flex items-center gap-1 rounded-md border border-tg-outline/40 bg-tg-high px-2.5 py-1 text-[11px] text-tg-ink hover:bg-tg-highest"
                    >
                      <Copy className="h-3 w-3" />
                      Salin Path
                    </button>
                    <button
                      type="button"
                      onClick={() => openFolder(extensionDir)}
                      className="flex items-center gap-1 rounded-md border border-tg-outline/40 bg-tg-high px-2.5 py-1 text-[11px] text-tg-ink hover:bg-tg-highest"
                    >
                      <Folder className="h-3 w-3 text-tg-tertiary" />
                      Buka Folder
                    </button>
                  </div>
                </div>
              )}

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                {browsers.map((b) => (
                  <div
                    key={b.id}
                    className={`rounded-xl border p-3 flex flex-col justify-between gap-3 ${
                      b.coming_soon
                        ? "border-tg-outline/30 bg-tg-lowest/60 opacity-85"
                        : "border-tg-outline/40 bg-tg-lowest"
                    }`}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div>
                        <div className="font-semibold text-sm text-tg-ink">{b.name}</div>
                        <div className="text-[10.5px] font-mono text-tg-muted">
                          {b.coming_soon
                            ? "Addon Store Resmi"
                            : b.installed
                            ? "Browser terdeteksi di Windows"
                            : "Tidak terdeteksi otomatis"}
                        </div>
                      </div>
                      <span
                        className={`rounded-full px-2 py-0.5 text-[10px] font-semibold border whitespace-nowrap ${
                          b.coming_soon
                            ? "border-tg-tertiary/40 bg-tg-tertiary/15 text-tg-tertiary"
                            : b.extension_installed
                            ? "border-tg-secondary/40 bg-tg-secondary/15 text-tg-secondary"
                            : "border-tg-primary/40 bg-tg-primary-container/15 text-tg-primary"
                        }`}
                      >
                        {b.status_label}
                      </span>
                    </div>

                    {b.coming_soon ? (
                      <div className="space-y-1.5">
                        <p className="text-[11px] text-tg-muted">
                          Integrasi otomatis Mozilla Firefox ditunda hingga rilis resmi di toko add-on Firefox.
                        </p>
                        <button
                          type="button"
                          disabled
                          className="w-full rounded-lg border border-tg-outline/30 bg-tg-high/50 px-3 py-1.5 font-semibold text-tg-muted cursor-not-allowed"
                        >
                          Soon on Store Addon
                        </button>
                      </div>
                    ) : (
                      <div className="space-y-1.5">
                        <p className="text-[10.5px] text-tg-muted">
                          {b.supports_cli_autoload
                            ? "Mendukung muat otomatis (--load-extension) saat Edge/Opera dibuka, atau via Developer Mode."
                            : `Membuka panduan di ${b.name} + File Explorer folder ekstensi untuk Load unpacked (${b.extensions_url}).`}
                        </p>
                        <button
                          type="button"
                          onClick={async () => {
                            try {
                              const res = await integrateBrowser(b.id, false);
                              try {
                                await navigator.clipboard?.writeText(res.extension_dir);
                              } catch {
                                // Clipboard unavailable
                              }
                              setBrowserNotice(res.message);
                            } catch (e) {
                              setBrowserNotice(`Gagal integrasi: ${String(e)}`);
                            }
                          }}
                          className="w-full flex items-center justify-center gap-1.5 rounded-lg bg-tg-primary-container px-3 py-1.5 font-semibold text-white hover:brightness-110"
                        >
                          <ExternalLink className="h-3.5 w-3.5" />
                          {b.supports_cli_autoload && !b.running
                            ? "Pasang Otomatis (1-Klik)"
                            : b.extension_installed
                            ? "Buka Panduan & Folder"
                            : "Integrasikan Sekarang"}
                        </button>
                        {b.supports_cli_autoload && b.running && !b.extension_installed && (
                          <button
                            type="button"
                            onClick={async () => {
                              try {
                                const res = await integrateBrowser(b.id, true);
                                setBrowserNotice(res.message);
                              } catch (e) {
                                setBrowserNotice(`Gagal muat otomatis: ${String(e)}`);
                              }
                            }}
                            className="w-full flex items-center justify-center gap-1.5 rounded-lg border border-tg-secondary/40 bg-tg-secondary/15 px-3 py-1 text-[11px] font-semibold text-tg-secondary hover:bg-tg-secondary/25"
                          >
                            <RefreshCw className="h-3 w-3" />
                            Tutup {b.name} &amp; Pasang Otomatis
                          </button>
                        )}
                      </div>
                    )}
                  </div>
                ))}
              </div>

              <div className="flex justify-end pt-1">
                <button
                  type="button"
                  onClick={() => loadSystemIntegration()}
                  className="flex items-center gap-1.5 rounded-lg border border-tg-outline/40 bg-tg-high px-3 py-1.5 text-tg-ink hover:bg-tg-highest"
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                  Periksa Ulang Browser
                </button>
              </div>
            </div>
          )}

          {activeTab === "appearance" && (
            <div className="space-y-4">
              <div className="text-tg-muted">
                Pilih tema antarmuka Take File Manager untuk menyesuaikan kenyamanan mata Anda.
              </div>
              <div className="grid grid-cols-2 gap-3">
                <button
                  type="button"
                  onClick={() => setTheme("dark")}
                  className={`rounded-xl border p-3 text-left transition ${
                    theme === "dark"
                      ? "border-tg-primary bg-tg-primary-container/15"
                      : "border-tg-outline/40 bg-tg-lowest hover:bg-tg-high/50"
                  }`}
                >
                  <div className="flex items-center gap-2 font-semibold text-tg-ink mb-1">
                    <Moon className="h-4 w-4 text-tg-primary" />
                    Dark Mode (Nocturnal Cobalt)
                  </div>
                  <p className="text-[11px] text-tg-muted">
                    Palet obsidian #0B1326 berstandar telemetri Google Stitch.
                  </p>
                </button>
                <button
                  type="button"
                  onClick={() => setTheme("light")}
                  className={`rounded-xl border p-3 text-left transition ${
                    theme === "light"
                      ? "border-tg-primary bg-tg-primary-container/15"
                      : "border-tg-outline/40 bg-tg-lowest hover:bg-tg-high/50"
                  }`}
                >
                  <div className="flex items-center gap-2 font-semibold text-tg-ink mb-1">
                    <Sun className="h-4 w-4 text-tg-tertiary" />
                    Light Mode (Clean Daylight)
                  </div>
                  <p className="text-[11px] text-tg-muted">
                    Kontras tinggi cerah untuk penggunaan siang hari.
                  </p>
                </button>
              </div>
            </div>
          )}

          {activeTab === "categories" && (
            <div className="space-y-3">
              <div className="text-tg-muted">
                Sesuaikan ekstensi berkas otomatis dan folder tujuan default per kategori:
              </div>
              <div className="space-y-2">
                {catDrafts.map((cat, idx) => (
                  <div
                    key={cat.id}
                    className="grid grid-cols-12 gap-2 items-center rounded-lg border border-tg-outline/30 bg-tg-lowest p-2"
                  >
                    <div className="col-span-3 font-semibold text-tg-ink">{cat.name}</div>
                    <input
                      type="text"
                      value={cat.extensions}
                      onChange={(e) => {
                        const copy = [...catDrafts];
                        copy[idx] = { ...cat, extensions: e.target.value };
                        setCatDrafts(copy);
                      }}
                      className="col-span-4 rounded border border-tg-outline/40 bg-tg-card px-2 py-1 font-mono text-[11px] text-tg-ink"
                    />
                    <div className="col-span-5 flex gap-1 items-center">
                      <input
                        type="text"
                        value={cat.save_dir}
                        onChange={(e) => {
                          const copy = [...catDrafts];
                          copy[idx] = { ...cat, save_dir: e.target.value };
                          setCatDrafts(copy);
                        }}
                        className="flex-1 min-w-0 rounded border border-tg-outline/40 bg-tg-card px-2 py-1 font-mono text-[11px] text-tg-ink"
                      />
                      <button
                        type="button"
                        onClick={async () => {
                          const chosen = await pickFolder(cat.save_dir || undefined);
                          if (chosen) {
                            const copy = [...catDrafts];
                            copy[idx] = { ...cat, save_dir: chosen };
                            setCatDrafts(copy);
                          }
                        }}
                        title="Pilih Folder Kategori"
                        className="rounded border border-tg-outline/40 bg-tg-high p-1 text-tg-tertiary hover:bg-tg-highest"
                      >
                        <Folder className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
              <div className="flex justify-end pt-2 border-t border-tg-outline/30">
                <button
                  onClick={handleSaveCategories}
                  className="rounded-lg bg-tg-primary-container px-4 py-1.5 font-semibold text-white hover:brightness-110"
                >
                  Simpan Kategori
                </button>
              </div>
            </div>
          )}

          {activeTab === "agent" && (
            <div className="space-y-3">
              <div>
                <label className="block text-tg-ink font-medium mb-1">Mode Akses Agent</label>
                <select
                  value={policyForm.mode}
                  onChange={(e) =>
                    setPolicyForm({
                      ...policyForm,
                      mode: e.target.value as AgentAccessPolicy["mode"],
                    })
                  }
                  className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-3 py-1.5 text-tg-ink"
                >
                  <option value="deny">Tolak Semua (Deny)</option>
                  <option value="ask">Tanya Setiap Kali (Default / Ask)</option>
                  <option value="allowlist">Otomatis untuk Folder Diizinkan (Allowlist)</option>
                </select>
              </div>

              <div>
                <label className="block text-tg-ink font-medium mb-1">
                  Folder Diizinkan (satu path per baris)
                </label>
                <textarea
                  rows={3}
                  value={allowedDirsText}
                  onChange={(e) => setAllowedDirsText(e.target.value)}
                  className="w-full rounded-lg border border-tg-outline/40 bg-tg-lowest px-2.5 py-1.5 font-mono text-tg-ink"
                />
              </div>

              <label className="flex items-center gap-2 text-tg-ink">
                <input
                  type="checkbox"
                  checked={policyForm.block_private_ips}
                  onChange={(e) =>
                    setPolicyForm({ ...policyForm, block_private_ips: e.target.checked })
                  }
                />
                Blokir URL Jaringan Privat / Loopback (Proteksi SSRF)
              </label>

              <div className="flex justify-end pt-2 border-t border-tg-outline/30">
                <button
                  onClick={handleSavePolicy}
                  className="rounded-lg bg-tg-primary-container px-4 py-1.5 font-semibold text-white hover:brightness-110"
                >
                  Simpan Kebijakan Agent
                </button>
              </div>
            </div>
          )}

          {activeTab === "logs" && (
            <div className="space-y-2">
              <div className="flex justify-between items-center">
                <span className="text-tg-muted">Menampilkan riwayat sistem dan audit agent</span>
                <button
                  onClick={() => loadSettingsAndLogs()}
                  className="flex items-center gap-1 rounded-lg border border-tg-outline/40 bg-tg-high px-2.5 py-1 text-tg-ink hover:bg-tg-highest"
                >
                  <RefreshCw className="h-3 w-3" />
                  Muat Ulang
                </button>
              </div>
              <div className="rounded-lg border border-tg-outline/30 bg-tg-lowest overflow-hidden">
                <table className="w-full text-left border-collapse text-[11px] font-mono">
                  <thead className="bg-tg-low text-tg-muted border-b border-tg-outline/30">
                    <tr>
                      <th className="px-2 py-1.5">Waktu</th>
                      <th className="px-2 py-1.5">Level</th>
                      <th className="px-2 py-1.5">Pesan</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-tg-outline/20">
                    {auditLogs.length === 0 ? (
                      <tr>
                        <td colSpan={3} className="px-3 py-4 text-center text-tg-muted">
                          Belum ada log tercatat
                        </td>
                      </tr>
                    ) : (
                      auditLogs.map((log) => (
                        <tr key={log.id} className="hover:bg-tg-high/40">
                          <td className="px-2 py-1.5 text-tg-muted whitespace-nowrap">
                            {formatDate(log.ts)}
                          </td>
                          <td className="px-2 py-1.5">
                            <span
                              className={`rounded px-1.5 py-0.5 text-[10px] ${
                                log.level === "ERROR"
                                  ? "bg-tg-error/20 text-tg-error"
                                  : log.level === "AGENT_AUDIT"
                                  ? "bg-tg-tertiary/20 text-tg-tertiary"
                                  : "bg-tg-secondary/20 text-tg-secondary"
                              }`}
                            >
                              {log.level}
                            </span>
                          </td>
                          <td className="px-2 py-1.5 text-tg-ink break-all">{log.message}</td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
